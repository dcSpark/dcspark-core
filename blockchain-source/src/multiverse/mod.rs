pub mod rollback;

use crate::{EventObject, GetNextFrom, PullFrom, Source};
use anyhow::{anyhow, Result};
use multiverse::{BestBlock, BestBlockSelectionRule, Variant};
use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

pub struct MultiverseSource<K, V, InnerSource> {
    multiverse: multiverse::Multiverse<K, V>,
    source: InnerSource,
    confirmation_depth: usize,
    confirmed: Option<K>,
}

impl<K, V, InnerSource> MultiverseSource<K, V, InnerSource> {
    pub fn new(
        multiverse: multiverse::Multiverse<K, V>,
        confirmation_depth: usize,
        inner_source: InnerSource,
    ) -> Self
    where
        K: AsRef<[u8]> + Eq + Hash + Debug + Clone + Sync,
        V: Variant<Key = K> + Clone,
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

        Self {
            multiverse,
            confirmation_depth,
            source: inner_source,
            confirmed: selected.map(|k| k.inner().clone()),
        }
    }

    pub fn into_inner(self) -> InnerSource {
        self.source
    }
}

#[async_trait::async_trait]
impl<K, V, InnerSource, ScalarInnerFrom> Source for MultiverseSource<K, V, InnerSource>
where
    InnerSource: Source<Event = V, From = Vec<ScalarInnerFrom>> + Send,
    ScalarInnerFrom: PullFrom + PartialEq + Clone + Sync + std::fmt::Debug,
    V: GetNextFrom<From = ScalarInnerFrom>,
    K: AsRef<[u8]> + Eq + Hash + Debug + Clone + Display + PullFrom + Sync,
    V: Variant<Key = K> + Clone + EventObject,
{
    type Event = InnerSource::Event;
    type From = Option<ScalarInnerFrom>;

    #[tracing::instrument(skip(self), fields(self.confirmed = ?self.confirmed))]
    async fn pull(&mut self, from: &Self::From) -> Result<Option<Self::Event>> {
        let confirmed_with_parent = self
            .confirmed
            .as_ref()
            .and_then(|confirmed| self.multiverse.get(confirmed))
            .and_then(|confirmed| confirmed.next_from().map(|point| (confirmed, point)))
            .map(|(confirmed, point)| {
                let parent = self
                    .multiverse
                    .get(confirmed.parent_id())
                    .and_then(V::next_from);

                Ok::<_, anyhow::Error>((parent, confirmed.clone(), point))
            })
            .transpose()?;

        // For Cardano, this is a bit of a waste of cpu cycles during the initial (long) sync, but
        // should be fine once we are caught up. The reason is that there will be already a block
        // range request in progress, and it needs to be consumed entirely. There are ways of
        // optimizing this, but it shouldn't have a huge effect.
        let inner_from =
            {
                let mut checkpoints = Vec::new();

                // add all the tips to the list of known points. To allow the wrapped source to start
                // pulling from there (since we already have those blocks, we just haven't forwarded them
                // to upper layers yet).
                for k in self.multiverse.tips().iter() {
                    let v =
                        self.multiverse.get(k).unwrap().next_from().ok_or_else(|| {
                            anyhow!("tip doesn't have an entry in the multiverse")
                        })?;

                    checkpoints.push(v);
                }

                if let Some((parent, confirmed, confirmed_point)) = confirmed_with_parent {
                    if from.as_ref() == parent.as_ref() {
                        // if `from` is the parent from the confirmed block, just return the confirmed
                        // block
                        //
                        // doing this for greater depths is possible, but there is no quick way of
                        // checking if the block belongs to the same branch right now.
                        return Ok(Some(confirmed));
                    } else if let Some(from) = from {
                        anyhow::ensure!(
                            from == &confirmed_point,
                            "non continuous pull not supported yet"
                        );

                        // TODO: re-check this
                        checkpoints.push(from.clone());
                    }
                } else if let Some(from) = from {
                    checkpoints.push(from.clone());
                }

                checkpoints
            };

        let block = match self.source.pull(&inner_from).await? {
            Some(block) => {
                if block.is_blockchain_tip() {
                    return Ok(Some(block));
                }

                // make sure we don't insert twice for now
                // ideally, this shouldn't happen
                if self.multiverse.get(block.id()).is_some() {
                    return Ok(None);
                } else {
                    block
                }
            }
            None => return Ok(None),
        };

        let new_stable_position =
            multiverse_insert_and_gc(block, &mut self.multiverse, self.confirmation_depth)?;

        if let Some(stable) = new_stable_position.filter(|stable| {
            self.confirmed
                .as_ref()
                .map(|confirmed| stable != confirmed)
                .unwrap_or(true)
        }) {
            let block = self
                .multiverse
                .get(&stable)
                .expect("select_best_root returned a block that is not inserted in the multiverse");

            self.confirmed.replace(stable);

            Ok(Some(block.clone()))
        } else {
            Ok(None)
        }
    }
}

pub(crate) fn multiverse_insert_and_gc<K, V>(
    event: V,
    multiverse: &mut multiverse::Multiverse<K, V>,
    confirmation_depth: usize,
) -> Result<Option<K>>
where
    K: AsRef<[u8]> + Eq + Hash + Debug + Clone + Display + Sync,
    V: Variant<Key = K>,
{
    tracing::debug!(id = ?event.id(), parent = ?event.parent_id(), "inserting block in the multiverse");

    multiverse.insert(event)?;

    let BestBlock {
        selected,
        discarded,
    } = {
        let _span = tracing::span!(tracing::Level::INFO, "selecting best root options").entered();
        multiverse.select_best_block(BestBlockSelectionRule::LongestChain {
            depth: confirmation_depth,
            age_gap: 1,
        })
    };

    {
        let _span =
                tracing::span!(tracing::Level::DEBUG, "pruning discarded branches", num_discarded = %discarded.len()).entered();
        for discarded in discarded {
            tracing::debug!(block_id = %discarded, "pruning branch");

            multiverse.remove(&discarded)?;
        }
    }

    Ok(selected.map(|entry_ref| entry_ref.inner().clone()))
}

#[async_trait::async_trait]
impl dcspark_core::StoppableService
    for MultiverseSource<
        dcspark_core::BlockId,
        crate::cardano::CardanoNetworkEvent<crate::cardano::BlockEvent, cardano_sdk::protocol::Tip>,
        crate::cardano::CardanoSource,
    >
{
    async fn stop(self) -> anyhow::Result<()> {
        self.into_inner().stop().await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventObject, GetNextFrom, PullFrom, Source};
    use anyhow::Result;
    use dcspark_core::BlockNumber;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
    pub struct K(pub String);

    impl PullFrom for K {}

    impl std::fmt::Display for K {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(&self.0, f)
        }
    }

    impl AsRef<[u8]> for K {
        fn as_ref(&self) -> &[u8] {
            self.0.as_ref()
        }
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct V {
        pub id: K,
        pub parent_id: K,
        pub block_number: BlockNumber,
    }

    impl EventObject for V {
        fn is_blockchain_tip(&self) -> bool {
            false
        }
    }

    impl Variant for V {
        type Key = K;

        fn id(&self) -> &Self::Key {
            &self.id
        }

        fn parent_id(&self) -> &Self::Key {
            &self.parent_id
        }

        fn block_number(&self) -> BlockNumber {
            self.block_number
        }
    }

    impl GetNextFrom for V {
        type From = K;

        fn next_from(&self) -> Option<Self::From> {
            Some(self.id.clone())
        }
    }

    #[derive(Default)]
    struct TestSource {
        last: Option<K>,
        chain: HashMap<Option<K>, V>,
    }

    impl TestSource {
        fn extend_tip(&mut self, v: V) {
            let new_last = v.id.clone();
            self.chain.insert(self.last.clone(), v);

            self.last.replace(new_last);
        }
    }

    #[async_trait::async_trait]
    impl Source for TestSource {
        type Event = V;
        type From = Vec<K>;

        async fn pull(&mut self, from: &Self::From) -> Result<Option<Self::Event>> {
            Ok(self.chain.get(&from.get(0).cloned()).cloned())
        }
    }

    fn linear_chain(length: usize) -> TestSource {
        let mut source = TestSource::default();
        for i in 1..=length {
            source.extend_tip(V {
                id: K(format!("s{i}")),
                parent_id: K(format!("s{0}", i - 1)),
                block_number: BlockNumber::new(i as u64),
            })
        }

        source
    }

    #[tokio::test]
    async fn multiverse_source_filters_unstable_blocks_linear_blockchain() {
        let min_depth = 3;

        let source = linear_chain(6);

        let mut multiverse: MultiverseSource<K, V, TestSource> = MultiverseSource {
            multiverse: multiverse::Multiverse::temporary().unwrap(),
            source,
            confirmation_depth: min_depth,
            confirmed: None,
        };

        let mut from = None;

        for _ in 0..min_depth {
            assert_eq!(multiverse.pull(&from).await.unwrap(), None);
        }

        for i in 1..=min_depth {
            let event = multiverse.pull(&from).await.unwrap().unwrap();

            from.replace(event.id().clone());

            assert_eq!(event.block_number(), BlockNumber::new(i as u64));
        }

        assert_eq!(multiverse.pull(&from).await.unwrap(), None);
    }

    #[tokio::test]
    async fn multiverse_source_repeat_pull() {
        let min_depth = 3;

        let source = linear_chain(6);

        let mut multiverse: MultiverseSource<K, V, TestSource> = MultiverseSource {
            multiverse: multiverse::Multiverse::temporary().unwrap(),
            source,
            confirmation_depth: min_depth,
            confirmed: None,
        };

        let mut from = None;

        for _ in 0..min_depth {
            assert_eq!(multiverse.pull(&from).await.unwrap(), None);
        }

        for _ in 1..=min_depth {
            let event1 = multiverse.pull(&from).await.unwrap().unwrap();
            let event2 = multiverse.pull(&from).await.unwrap().unwrap();

            assert_eq!(event1, event2);

            from.replace(event2.id().clone());
        }
    }
}
