use crate::{multiverse::multiverse_insert_and_gc, EventObject, GetNextFrom, PullFrom, Source};
use anyhow::{anyhow, Result};
use multiverse::{BestBlock, BestBlockSelectionRule, Variant};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashSet,
    fmt::{Debug, Display},
    hash::Hash,
};

#[derive(Debug)]
pub enum Event<Inner, EventId> {
    InnerEvent(Inner),
    Rollback(EventId),
}

/// Source that provides the tips + confirmed point to the cardano source when pulling, and also
/// detects rollbacks by keeping track of the last seen block. If a block is received from the
/// inner source that is not a direct succesor to the previous one, this source generates a
/// rollback event to the common ancestor with the previous branch.
pub struct ForkHandlingSource<K, V, InnerSource, Event> {
    multiverse: multiverse::Multiverse<K, V>,
    source: InnerSource,
    confirmation_depth: usize,
    confirmed: Option<K>,
    last: Option<K>,
    events: Vec<Event>,
}

impl<K, V, InnerSource, E> ForkHandlingSource<K, V, InnerSource, E> {
    pub fn new(
        multiverse: multiverse::Multiverse<K, V>,
        confirmation_depth: usize,
        inner_source: InnerSource,
    ) -> Self
    where
        K: AsRef<[u8]> + Eq + Hash + Debug + Clone + Sync,
        V: Variant<Key = K>,
    {
        let BestBlock {
            selected,
            discarded: _,
        } = {
            let _span =
                tracing::span!(tracing::Level::INFO, "selecting best root options").entered();
            multiverse.select_best_block(BestBlockSelectionRule::LongestChain {
                depth: confirmation_depth,
                // not going to delete anything here, so this doesn't matter
                age_gap: 0,
            })
        };

        let last = multiverse.iter().last().map(|entry| entry.id().clone());

        Self {
            multiverse,
            confirmation_depth,
            source: inner_source,
            confirmed: selected.map(|k| k.inner().clone()),
            last,
            events: Default::default(),
        }
    }

    pub fn into_inner(self) -> InnerSource {
        self.source
    }
}

#[async_trait::async_trait]
impl<K, V, InnerSource, ScalarInnerFrom> Source
    for ForkHandlingSource<K, V, InnerSource, Event<InnerSource::Event, ScalarInnerFrom>>
where
    InnerSource: Source<Event = V, From = Vec<ScalarInnerFrom>> + Send,
    ScalarInnerFrom: PullFrom + PartialEq + Clone + Sync + std::fmt::Debug + Eq + Hash,
    K: AsRef<[u8]>
        + Eq
        + Hash
        + Debug
        + Clone
        + Display
        + PullFrom
        + Sync
        + Serialize
        + DeserializeOwned,
    InnerSource::Event: GetNextFrom<From = ScalarInnerFrom>,
    InnerSource::Event: Variant<Key = K> + Clone + EventObject,
    V: Variant<Key = K> + Clone + EventObject + Debug,
    V: GetNextFrom<From = ScalarInnerFrom>,
{
    type Event = Event<InnerSource::Event, ScalarInnerFrom>;
    type From = Option<ScalarInnerFrom>;

    #[tracing::instrument(skip(self), fields(self.confirmed = ?self.confirmed))]
    async fn pull(&mut self, from: &Self::From) -> Result<Option<Self::Event>> {
        if let Some(event) = self.events.pop() {
            return Ok(Some(event));
        }

        let inner_from = {
            let mut checkpoints = HashSet::new();

            for k in self.multiverse.tips().iter() {
                let v = self
                    .multiverse
                    .get(k)
                    .ok_or_else(|| anyhow!("tip doesn't have an entry in the multiverse"))?
                    .next_from()
                    .unwrap()
                    .clone();

                checkpoints.insert(v);
            }

            if let Some(confirmed) = &self.confirmed {
                let confirmed = self.multiverse.get(confirmed).unwrap();

                checkpoints.insert(confirmed.next_from().unwrap());
            }

            if let Some(from) = from {
                checkpoints.insert(from.clone());
            }

            checkpoints.into_iter().collect()
        };

        let block = match self.source.pull(&inner_from).await? {
            Some(block) => {
                if block.is_blockchain_tip() {
                    return Ok(Some(Event::InnerEvent(block)));
                }

                if self.multiverse.get(block.id()).is_some() {
                    return Ok(None);
                } else {
                    block
                }
            }
            None => return Ok(None),
        };

        let parent_id = block.parent_id().clone();
        let block_id = block.id().clone();

        let new_stable_position =
            multiverse_insert_and_gc(block.clone(), &mut self.multiverse, self.confirmation_depth)?;

        if let Some(stable) = new_stable_position.filter(|stable| {
            self.confirmed
                .as_ref()
                .map(|confirmed| stable != confirmed)
                .unwrap_or(true)
        }) {
            self.confirmed.replace(stable);
        }

        let previous_tip = self.last.replace(block_id.clone());

        let new_event = if previous_tip
            .as_ref()
            // if the blocks are contiguous, we are always in the same branch
            .map(|last| last == &parent_id)
            .unwrap_or(false)
        {
            Event::InnerEvent(block)
        } else if let Some(mut previous_branch_cursor) = previous_tip {
            // algorithm:
            //
            // Traverse the previous branch backwards, building a hashset with all the hashes in
            // the way. This should stop at the confirmed block.
            //
            // Traverse the new branch backwards. For each block check if it was in the previous
            // branch.
            //
            // If it is not, then we push it on the stack. Because we are traversing backwards, the
            // top of the stack is the block with less height in the chain.
            //
            // If it is, we push a rollback event to this block on the stack.
            //
            // Then events are popped until the stack is empty.
            let mut blocks_in_previous_branch = HashSet::new();

            while let Some(entry) = self.multiverse.get(&previous_branch_cursor) {
                blocks_in_previous_branch.insert(entry.id());
                previous_branch_cursor = entry.parent_id().clone();
            }

            let mut current_branch_cursor = block_id;

            while let Some(entry) = self.multiverse.get(&current_branch_cursor) {
                if blocks_in_previous_branch.contains(entry.id()) {
                    self.events
                        .push(Event::Rollback(entry.next_from().unwrap()));
                    break;
                }

                self.events.push(Event::InnerEvent(entry.clone()));
                current_branch_cursor = entry.parent_id().clone();
            }

            // this will always be a rollback event.
            //
            // if it's empty, it probably means the confirmation depth is not big enough, in which
            // case there is nothing to do.
            //
            // we could return an error though.
            self.events
                .pop()
                .expect("no common ancestor between branches")
        } else {
            // if the db is empty, send a rollback event to the `from` argument, just to be
            // safe
            self.events.push(Event::InnerEvent(block));
            Event::Rollback(from.as_ref().unwrap().clone())
        };

        Ok(Some(new_event))
    }
}

impl<Inner: EventObject, EventId: Send> EventObject for Event<Inner, EventId> {
    fn is_blockchain_tip(&self) -> bool {
        if let Self::InnerEvent(e) = self {
            e.is_blockchain_tip()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiverse::tests::{K, V};
    use crate::Source;
    use anyhow::Result;
    use dcspark_core::BlockNumber;

    #[derive(Default, Debug)]
    struct TestSource {
        sorted: Vec<V>,
        last: usize,
    }

    impl TestSource {
        fn extend(&mut self, _parent: K, v: V) {
            self.sorted.push(v);
        }
    }

    #[async_trait::async_trait]
    impl Source for TestSource {
        type Event = V;
        type From = Vec<K>;

        async fn pull(&mut self, _from: &Self::From) -> Result<Option<Self::Event>> {
            let result = self.sorted.get(self.last);
            self.last += 1;

            Ok(result.cloned())
        }
    }

    fn add_fork(source: &mut TestSource, length: usize, forking_point: usize) {
        let first_id = source.sorted.len();

        let from = first_id;
        let to = first_id + length - forking_point;

        for i in from..=to {
            let parent_id = if i == from { forking_point } else { i - 1 };

            let parent_id = K(format!("s{0}", parent_id));

            source.extend(
                parent_id.clone(),
                V {
                    id: K(format!("s{0}", i)),
                    parent_id,
                    block_number: BlockNumber::new((i - forking_point) as u64),
                },
            )
        }
    }

    // this generates a DAG with 3 branches of a certain length (from tip to s0)
    //
    // The source will first return the central branch entirely.
    //
    // Then it will return the other branches, each one in its entirety.
    //
    // This would lead to the rollback source generating 2 rollback events.
    fn forking_chain(length: usize) -> TestSource {
        let mut source = TestSource::default();

        add_fork(&mut source, length, 0);
        add_fork(&mut source, length, 3);
        add_fork(&mut source, length, 4);

        source
    }

    #[tokio::test]
    async fn generates_rollback_event() {
        let min_depth = 3;

        let source = forking_chain(6);

        let mut multiverse: ForkHandlingSource<K, V, TestSource, Event<V, K>> =
            ForkHandlingSource::new(
                multiverse::Multiverse::temporary().unwrap(),
                min_depth,
                source,
            );

        let mut parent = K("s0".to_string());

        while let Some(event) = multiverse.pull(&Some(K("s0".to_string()))).await.unwrap() {
            match event {
                Event::InnerEvent(event) => {
                    assert_eq!(event.parent_id(), &parent);
                    parent = event.id().clone();
                }
                Event::Rollback(new_parent) => {
                    parent = new_parent.clone();
                }
            }
        }
    }
}
