#![doc = include_str!("../README.md")]

mod entry;
mod error;
mod variant;
mod visitor;

// only exposes the test utils in test mode.
#[cfg(test)]
pub(crate) mod test_utils;

use self::entry::{Entry, EntryWeakRef};
pub use self::{
    entry::EntryRef, error::MultiverseError, variant::Variant, visitor::DepthOrderedIterator,
};
use dcspark_core::BlockNumber;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    collections::{btree_map, hash_map::Entry as HashMapEntry, BTreeMap, HashMap, HashSet},
    fmt,
    hash::Hash,
    path::Path,
    str,
    sync::Arc,
};

/// Configure the selection rule for the [`Multiverse::select_best_block`]
/// function
///
/// # serde
///
/// [serde] formatting is providing for the object in order to facilitate including
/// it in _JavaScript_ API or to get the value from a configuration file.
///
/// The Variant is given under the field `rule` and is encoded in `PascalCase`.
/// and the parameters of each variant are encoded in `snake_case`.
///
/// ```
/// # use multiverse::BestBlockSelectionRule;
/// # use deps::serde_json::{json, to_value};
/// # fn test() -> Result<(), deps::serde_json::Error> {
/// let expected = json!{{
///   "rule": "LongestChain",
///   "depth": 3,
///   "age_gap": 2,
/// }};
///
/// let value = BestBlockSelectionRule::LongestChain { depth: 3, age_gap: 2 };
/// # assert_eq!(to_value(value)?, expected);
/// # Ok(())
/// # }
/// # test().unwrap()
/// ```
///
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[serde(tag = "rule")]
pub enum BestBlockSelectionRule {
    /// this algorithm is pretty straight forward. We select the
    /// longest chain as the preferred fork. All we do is iterate
    /// through all the different tips and compare the
    /// [`block_number`](Variant::block_number) returned value the tips
    /// of the [`Multiverse`].
    ///
    /// * Time complexity: `O(t) where t are tips`
    /// * Space complexity: `O(1)`
    ///
    /// The drawback is while this chain might be the longest chain
    /// it is not necessarily the most active chain.
    ///
    /// It may be that two chains have the same length. Then the first
    /// one selected by the algorithm will conserve its place.
    ///
    #[serde(rename_all = "snake_case")]
    LongestChain {
        /// when the best block function this will be the value used to determined
        /// how many confirmations are required in order to consider a block
        /// confirmed enough
        depth: usize,
        /// if a block is confirmed based on the `depth` value. This will allow
        /// the [`Multiverse::select_best_block`] function to determine the blocks
        /// that may need to be garbage collected as too old and unlikely to
        /// be forked
        age_gap: usize,
    },
    /*
    TODO: one of the update we could add is to look at the Heaviest chain
          the chain that has the most activities on.

        /// Select the chain that is the heaviest in term of total
        /// activity: i.e. this is the chain that has received the most
        /// number of blocks. This is not necessarily the longest chain.
        ///
        /// * Time complexity: `O(n) where n is number of entries`;
        /// * space complexity: `O(n)`
        HeaviestChain,
    */
}

/// A multiverse, holder of the multiple timelines.
///
/// we are storing all of the entries `(K, V)` in a persistent
/// database so that if something happen during execution we can
/// re-start the operation with more or less better state.
///
pub struct Multiverse<K, V> {
    /// keep a hold of the [`sled::Db`] but it's really the
    /// tree we will be using.
    _db: sled::Db,

    tree: sled::Tree,

    all: HashMap<EntryRef<K>, Entry<K, V>>,
    ordered: BTreeMap<BlockNumber, HashSet<EntryRef<K>>>,
    tips: HashSet<EntryRef<K>>,
    roots: HashSet<EntryRef<K>>,

    store_from: BlockNumber,
}

/// Structure returned by [`Multiverse::select_best_block`] function.
pub struct BestBlock<K> {
    /// the selected best block if any.
    ///
    /// If this value is `None` it does not necessarily means there is
    /// no good blocks at all. It means that given the parameters given
    /// while calling [`Multiverse::select_best_block`] there were no block
    /// that could have been chosen.
    pub selected: Option<EntryRef<K>>,
    /// collection of blocks that may be discarded/garbage collected.
    ///
    /// Given the parameters passed to [`Multiverse::select_best_block`] this
    /// will contains the blocks that are no longer of interest and may be
    /// garbage collected.
    pub discarded: HashSet<EntryRef<K>>,
}

impl<K, V> Multiverse<K, V>
where
    K: Eq + Hash,
{
    /// list all the tips of the Multiverse
    pub fn tips(&self) -> HashSet<Arc<K>> {
        self.tips.iter().map(|e| Arc::clone(&e.key)).collect()
    }
}

impl<K, V> Multiverse<K, V>
where
    K: AsRef<[u8]>,
    V: serde::de::DeserializeOwned + serde::Serialize,
{
    /// create a Multiverse with the given sled database as
    /// core entry of the component
    ///
    /// The `domain` is used as an identifier within the Db.
    ///
    #[inline]
    fn new_with(db: sled::Db, domain: &str, store_from: BlockNumber) -> Self {
        let all = HashMap::new();
        let ordered = BTreeMap::new();
        let tips = HashSet::new();
        let roots = HashSet::new();

        let tree = db.open_tree(domain).unwrap();

        Self {
            _db: db,
            tree,
            all,
            ordered,
            tips,
            roots,
            store_from,
        }
    }

    /// create a pre-configured to be temporary Multiverse
    ///
    /// When using this nothing will be made persistent. Not to use in production
    /// but for dry-run and testing.
    pub fn temporary() -> Result<Self, MultiverseError> {
        // since we are not setting a path this
        // will be created in the /dev/shm on linux
        // and deleted on drop
        let db = sled::Config::new().temporary(true).open()?;

        Ok(Self::new_with(db, "temporary", BlockNumber::MIN))
    }

    fn db_remove(&mut self, counter: BlockNumber, key: &K) -> Result<bool, MultiverseError> {
        let key = mk_sled_key(counter, key);
        let b = self.tree.remove(key)?;

        Ok(b.is_some())
    }

    /// insert the given entry in the database
    ///
    /// returns true if the value is an original value
    fn db_insert(
        &mut self,
        counter: BlockNumber,
        key: &K,
        value: &V,
    ) -> Result<bool, MultiverseError> {
        if self.store_from <= counter {
            let key = mk_sled_key(counter, key);
            let b = self.tree.insert(key, deps::serde_json::to_vec(value)?)?;

            Ok(b.is_none())
        } else {
            Ok(false)
        }
    }

    pub fn clear(&mut self) -> Result<(), MultiverseError> {
        tracing::warn!("Irreversibly NUKE a multiverse");
        self.tree.clear()?;
        self.all.clear();
        self.ordered.clear();
        self.tips.clear();
        self.roots.clear();

        Ok(())
    }

    pub fn destroy(self) -> Result<(), MultiverseError> {
        tracing::warn!("Irreversibly LEVEL a multiverse");

        let name = self.tree.name();
        let dropped = self._db.drop_tree(name)?;

        if dropped {
            tracing::info!("Multiverse successfully destroyed");
        }

        Ok(())
    }
}

impl<K, V> Multiverse<K, V> {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.all.len()
    }
}

impl<K, V> Multiverse<K, V>
where
    K: AsRef<[u8]> + Eq + Hash,
{
    /// check if a given key `K` is present in the [`Multiverse`]
    #[tracing::instrument(skip(self, key), level = "trace")]
    #[inline]
    pub fn contains(&self, key: &K) -> bool {
        self.all.contains_key(key)
    }
}

impl<K, V> Multiverse<K, V>
where
    K: AsRef<[u8]> + Eq + Hash + fmt::Debug + Clone,
    V: Variant<Key = K>,
{
    /// create an iterator over the entries of the multiverse
    /// ordered by the associated [`BlockNumber`].
    ///
    /// We tie the iterator to the multiverse to prevent updating the
    /// storage while we are iterating over the entries.
    pub fn iter(&self) -> DepthOrderedIterator<'_, K, V> {
        DepthOrderedIterator::new(self)
    }

    /// load the multiverse from the given [`sled::Db`].
    ///
    /// the `domain` is the sub[`sled::Tree`] in the [`sled::Db`] that
    /// we will use to store our states in.
    ///
    /// The `domain` is used as an identifier within the Db.
    ///
    #[tracing::instrument(skip(db), level = "debug")]
    pub fn load_from(
        db: sled::Db,
        domain: &str,
        store_from: BlockNumber,
    ) -> Result<Self, MultiverseError> {
        let mut multiverse = Self::new_with(db, domain, store_from);

        for entry in multiverse.tree.iter().values() {
            let formatted_ir = entry?;
            let ir = deps::serde_json::from_slice(&formatted_ir)?;

            multiverse.insert_in_memory(ir)?;
        }

        Ok(multiverse)
    }

    /// open the multiverse, loading an existing persisted multiverse
    ///
    /// the `domain` is the sub[`sled::Tree`] in the [`sled::Db`] that
    /// we will use to store our states in.
    ///
    /// The `domain` is used as an identifier within the Db.
    ///
    pub fn open<P>(path: P, domain: &str, store_from: BlockNumber) -> Result<Self, MultiverseError>
    where
        P: AsRef<Path>,
    {
        let db = sled::Config::new().path(&path).open()?;

        Self::load_from(db, domain, store_from)
    }

    /// Returns a reference to the value corresponding to the key
    #[inline]
    pub fn get(&self, key: &K) -> Option<&V> {
        self.all.get(key).map(|entry| &entry.value)
    }

    #[tracing::instrument(skip(self, variant)
        level = "debug",
        err,
        fields(
            block.id = ?variant.id(),
            block.parent_id = ?variant.parent_id(),
            block.block_number = %variant.block_number(),
        )
    )]
    pub fn insert(&mut self, variant: V) -> Result<(), MultiverseError> {
        if !self.db_insert(variant.block_number(), variant.id(), &variant)? {
            if self.all.contains_key(&EntryRef::new(variant.id().clone())) {
                return Ok(());
            } else {
                tracing::debug!(counter = %variant.block_number(), key = ?variant.id(), "half backed insert");
            }
        }

        self.insert_in_memory(variant)
    }

    #[tracing::instrument(skip(self, variant)
        level = "debug",
        err,
        fields(
            block.id = ?variant.id(),
            block.parent_id = ?variant.parent_id(),
            block.block_number = %variant.block_number(),
        )
    )]
    fn insert_in_memory(&mut self, variant: V) -> Result<(), MultiverseError> {
        let entry_ref = EntryRef::new(variant.id().clone());
        let parent = EntryRef::new(variant.parent_id().clone());

        // get the [`ParentRef`] from the one present in the HashMap
        // or create a new one.
        let parent = if let HashMapEntry::Occupied(mut parent) = self.all.entry(parent) {
            // if the parent entry is still present in the multiverse, we
            // can update it to update the children list
            parent.get_mut().add_child(entry_ref.clone());

            // remove the parent from the tip (if any). It is possible we add
            // an entry as a child of an entry that is not at the tip. Joy of
            // blockchain technology: it's possible to fork at any point in
            // time (depending on consensus rules).
            let _removed = self.tips.remove(parent.key());

            parent.key().weak()
        } else {
            // an entry without a parent is a root.
            // we can ignore if the root was already inserted (it is not
            assert!(
                self.roots.insert(entry_ref.clone()),
                "We expect to insert this new entry in the multiverse. \
                This should not happen because we already checked the \
                result of db_insert earlier"
            );

            // create an empty weak reference counter to that parent that
            // does not exist.
            EntryWeakRef::new()
        };

        self.ordered
            .entry(variant.block_number())
            .or_default()
            .insert(entry_ref.clone());
        let entry = Entry::new(parent, variant);
        self.all.insert(entry_ref.clone(), entry);

        // by default all new insertion are a tip. This is because it is the first
        // time we are meeting it.
        if !self.tips.insert(entry_ref) {
            tracing::warn!(
                "we expected to insert the new entry in the multiverse. This should not happen because of the db_insert check we did earlier."
            )
        }

        Ok(())
    }

    pub fn remove(&mut self, key: &EntryRef<K>) -> Result<V, MultiverseError> {
        let entry = if let Some(entry) = self.all.remove(key) {
            entry
        } else {
            return Err(MultiverseError::NotFound);
        };

        if self.roots.remove(key) {
            // Removing the entry makes all the children "orphaned". So they
            // need to become root themselves. Iterate through all the children
            // and add them in the root set
            for child in entry.children {
                assert!(
                    self.roots.insert(child.clone()),
                    "Somehow a child ({child:?}) was already in the set of root entries. \
                This should not happen in normal circumstances.",
                );
            }
        }

        // if the entry had a parent, it then may become a tip (if that
        // parent has no children entries)
        //
        if let Some(parent_ref) = entry.parent.upgrade() {
            if let Some(parent) = self.all.get_mut(&parent_ref) {
                assert!(
                    parent.children.remove(key),
                    "Removing this child should always be true"
                );

                if parent.children.is_empty() {
                    assert!(
                        self.tips.insert(parent_ref),
                        "We just removed the last child from that node so we should \
                        not have it in the tip set already."
                    )
                }
            }
        }

        let counter = entry.value.block_number();

        if let btree_map::Entry::Occupied(mut occupied) = self.ordered.entry(counter) {
            occupied.get_mut().remove(key);
            if occupied.get().is_empty() {
                occupied.remove();
            }
        };

        let _removed = self.tips.remove(key);
        self.db_remove(counter, key.borrow())?;

        Ok(entry.value)
    }

    /// from the given block `tip` retrieve the ancestor that is `min_depth`
    /// "parent" to the given `tip`.
    ///
    /// This function is `O(min_depth)` in time and `O(1)` in space.
    ///
    #[tracing::instrument(skip(self, tip), level = "debug")]
    fn ancestor(&self, tip: &EntryRef<K>, min_depth: usize) -> Option<EntryRef<K>> {
        let mut ancestor = tip.clone();
        for _ in 0..min_depth {
            let entry = self
                .all
                .get(&ancestor)
                .expect("Entry should be already there at this point");

            ancestor = entry.parent.upgrade()?;
        }

        Some(ancestor)
    }

    /// function to compute the [`BestBlock`] based on the given parameters
    /// See [`BestBlockSelectionRule`] for mor information about the available
    /// algorithms.
    ///
    pub fn select_best_block(&self, rule: BestBlockSelectionRule) -> BestBlock<K> {
        match rule {
            BestBlockSelectionRule::LongestChain { depth, age_gap } => {
                self.select_best_block_longest_chain(depth, age_gap)
            }
        }
    }

    fn select_best_block_longest_chain(&self, depth: usize, age_gap: usize) -> BestBlock<K> {
        // take the blocks that have the highest `BlockNumber`
        // these are the most likely tips at the given time
        let selected = if let Some((_, tips)) = self.ordered.iter().last() {
            if let Some(tip) = tips.iter().next() {
                self.ancestor(tip, depth)
            } else {
                None
            }
        } else {
            None
        };

        let mut discarded = HashSet::new();
        if let Some(selected) = selected.as_ref() {
            if let Some(selected) = self.all.get(selected) {
                let _span =
                    tracing::span!(tracing::Level::DEBUG, "compute root to discard").entered();

                let max = selected.value.block_number().saturating_sub(age_gap as u64);

                for (number, set) in self.ordered.range(BlockNumber::MIN..max) {
                    debug_assert!(number <= &max);
                    discarded.extend(set.iter().cloned());
                }
            }
        }

        BestBlock {
            selected,
            discarded,
        }
    }

    /// select a fork (a tip) of the multiverse based on the [`BestBlockSelectionRule`]
    /// algorithm.
    ///
    /// see [`BestBlockSelectionRule`] for more information about the different options
    /// and the trade off.
    pub fn preferred_fork_tip(&self, rule: BestBlockSelectionRule) -> Option<EntryRef<K>> {
        match rule {
            BestBlockSelectionRule::LongestChain { .. } => self.prefer_longest_chain_fork_tip(),
        }
    }

    fn prefer_longest_chain_fork_tip(&self) -> Option<EntryRef<K>> {
        let mut tips = self.tips.iter();
        let mut result = tips.next().cloned()?;

        let mut longest = self
            .all
            .get(&result)
            .expect("entries in the `tips` should be in the `all`")
            .value
            .block_number();

        for tip_ref in tips {
            let tip = self
                .all
                .get(tip_ref)
                .expect("entries in the `tips` should be in the `all`");

            if tip.value.block_number() > longest {
                longest = tip.value.block_number();
                result = tip_ref.clone();
            }
        }

        Some(result)
    }
}

/// the sled::Db iterator allows to load in an ordered fashion. So
/// long we decide to use a `key` format that makes sense we should
/// be just fine.
///
/// Something along the line of `<block number>-<block id>`
/// should work fine since the block are supposed to be ordered by
/// block number anyway. So we should always go from parent to children
/// and the block id will be used as differentiator in case of
/// <block number> collisions (forks).
///
fn mk_sled_key(counter: BlockNumber, key: impl AsRef<[u8]>) -> Vec<u8> {
    let mut bytes = vec![];

    // leverage [`sled`](https://crates.io/crates/sled) lexicographic
    // ordering by using big endian
    bytes.extend(counter.into_inner().to_be_bytes());

    // add the separator to help with human readable and to detect
    // malformation of key in the db (a bit like a magic number)
    bytes.extend(b"-");

    // just store whatever was given as the key
    bytes.extend(key.as_ref());

    bytes
}

impl<K> Default for BestBlock<K> {
    fn default() -> Self {
        Self {
            selected: None,
            discarded: HashSet::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;
    use crate::declare_blockchain;
    use anyhow::{bail, ensure, Context as _, Result};

    #[test]
    fn ancestor_0_is_self() {
        let mut m: Multiverse<K, V> = Multiverse::temporary().unwrap();
        let blockchain = declare_blockchain! {
            "Root" <= "1" <= "2",
                      "1" <= "3"
        };

        for block in blockchain {
            m.insert(block).unwrap();
        }

        let root = EntryRef::new(K::new("Root"));
        let one = EntryRef::new(K::new("1"));
        let two = EntryRef::new(K::new("2"));
        let three = EntryRef::new(K::new("3"));

        assert_eq!(m.ancestor(&root, 0), Some(root));
        assert_eq!(m.ancestor(&one, 0), Some(one));
        assert_eq!(m.ancestor(&two, 0), Some(two));
        assert_eq!(m.ancestor(&three, 0), Some(three));
    }

    #[test]
    fn ancestor_1_is_parent() {
        let mut m: Multiverse<K, V> = Multiverse::temporary().unwrap();
        let blockchain = declare_blockchain! {
            "Root" <= "1" <= "2",
                      "1" <= "3"
        };

        for block in blockchain {
            m.insert(block).unwrap();
        }

        let root = EntryRef::new(K::new("Root"));
        let one = EntryRef::new(K::new("1"));
        let two = EntryRef::new(K::new("2"));
        let three = EntryRef::new(K::new("3"));

        assert_eq!(m.ancestor(&root, 1), None);
        assert_eq!(m.ancestor(&one, 1), Some(root));
        assert_eq!(m.ancestor(&two, 1), Some(one.clone()));
        assert_eq!(m.ancestor(&three, 1), Some(one));
    }

    #[test]
    fn ancestor_1_is_grand_parent() {
        let mut m: Multiverse<K, V> = Multiverse::temporary().unwrap();
        let blockchain = declare_blockchain! {
            "Root" <= "1" <= "2",
                      "1" <= "3"
        };

        for block in blockchain {
            m.insert(block).unwrap();
        }

        let root = EntryRef::new(K::new("Root"));
        let one = EntryRef::new(K::new("1"));
        let two = EntryRef::new(K::new("2"));
        let three = EntryRef::new(K::new("3"));

        assert_eq!(m.ancestor(&root, 2), None);
        assert_eq!(m.ancestor(&one, 2), None);
        assert_eq!(m.ancestor(&two, 2), Some(root.clone()));
        assert_eq!(m.ancestor(&three, 2), Some(root));
    }

    /// test the assumption that the lexicographic ordering is
    /// what we expect in when we create the [`mk_sled_key`]:
    /// we want the counter to be the primary key ordering entry
    /// and that it is consistent in the serialised and deserialised
    /// form.
    #[test]
    fn mk_sled_key_ordered() {
        use std::cmp::Ordering::{self, Equal, Greater, Less};

        fn assumption(
            left: (BlockNumber, &[u8]),
            right: (BlockNumber, &[u8]),
            ordering: Ordering,
        ) -> bool {
            let left = {
                let (counter, bytes) = left;
                mk_sled_key(counter, bytes)
            };

            let right = {
                let (counter, bytes) = right;
                mk_sled_key(counter, bytes)
            };

            left.cmp(&right) == ordering
        }

        assert!(assumption(
            (BlockNumber::new(0), &[0]),
            (BlockNumber::new(0), &[0]),
            Equal
        ));
        assert!(assumption(
            (BlockNumber::new(0), &[0]),
            (BlockNumber::new(0), &[1]),
            Less
        ));
        assert!(assumption(
            (BlockNumber::new(0), &[1]),
            (BlockNumber::new(0), &[0]),
            Greater
        ));

        assert!(assumption(
            (BlockNumber::new(0x1F00), &[0x00]),
            (BlockNumber::new(0x0FFF), &[0xFF, 0xFF]),
            Greater
        ));
    }

    /// perform some basic insert/remove operation in the database
    ///
    /// mainly testing when the insert/remove are supposed to return
    /// `true` or `false`.
    #[test]
    fn multiverse_basic_db_operations() {
        let mut m: Multiverse<Vec<u8>, Vec<u8>> = Multiverse::temporary().unwrap();

        assert!(m
            .db_insert(BlockNumber::new(0u64), &vec![0], &vec![0])
            .unwrap());
        assert!(!m
            .db_insert(BlockNumber::new(0u64), &vec![0], &vec![0])
            .unwrap());

        assert!(m
            .db_insert(BlockNumber::new(1u64), &vec![1], &vec![1])
            .unwrap());

        assert!(m.db_remove(BlockNumber::new(0u64), &vec![0]).unwrap());
        assert!(m.db_remove(BlockNumber::new(1u64), &vec![1]).unwrap());

        assert!(!m.db_remove(BlockNumber::new(1u64), &vec![1]).unwrap());
    }

    #[test]
    fn multiverse_linked_list_of_1() {
        let mut m: Multiverse<K, V> = Multiverse::temporary().unwrap();

        let blockchain = declare_blockchain! { "Root" };

        for block in blockchain {
            m.insert(block).unwrap();
        }

        assert_eq!(m.all.len(), 1);
        assert!(m.tips.contains(&K::new("Root")));
        assert!(m.roots.contains(&K::new("Root")));
    }

    #[test]
    fn multiverse_linked_list_of_2() {
        let mut m: Multiverse<K, V> = Multiverse::temporary().unwrap();

        let blockchain = declare_blockchain! { "Root" <= "Child" };

        for block in blockchain {
            m.insert(block).unwrap();
        }

        assert_eq!(m.all.len(), 2);

        {
            let root = m.all.get(&K::new("Root")).unwrap();
            assert!(root.children.contains(&K::new("Child")));
            assert!(root.parent.clone().upgrade().is_none());
        }

        {
            let child = m.all.get(&K::new("Child")).unwrap();
            assert!(child.children.is_empty());
            assert_eq!(
                child.parent.clone().upgrade(),
                Some(EntryRef::new(K::new("Root")))
            );
        }

        let BestBlock {
            selected,
            discarded,
        } = m.select_best_block(BestBlockSelectionRule::LongestChain {
            depth: 1,
            age_gap: 1,
        });
        assert_eq!(selected, Some(EntryRef::new(K::new("Root"))));
        assert!(discarded.is_empty());
    }

    #[test]
    fn multiverse_insert_twice() {
        let mut m: Multiverse<K, V> = Multiverse::temporary().unwrap();

        for _ in 0..2 {
            let blockchain = declare_blockchain! { "Root" };

            for block in blockchain {
                m.insert(block).unwrap();
            }
        }
    }

    #[test]
    fn entries_are_loaded_in_main_when_restoring() {
        let db = sled::Config::new().temporary(true).open().unwrap();

        let blockchain = declare_blockchain! { "Root" };

        let mut multiverse = Multiverse::new_with(db.clone(), "temporary", BlockNumber::MIN);

        for block in blockchain {
            multiverse.insert(block).unwrap();
        }

        std::mem::drop(multiverse);

        let multiverse: Multiverse<K, V> =
            Multiverse::load_from(db, "temporary", BlockNumber::MIN).unwrap();

        multiverse
            .get(&K::new("Root"))
            .expect("entries were not restored from db");
    }

    struct Simulation {
        multiverse: Multiverse<K, V>,
        selection_rule: BestBlockSelectionRule,
        selected: Option<K>,
    }

    impl Simulation {
        const COUNTER_START: u64 = u64::MIN;
        pub fn push(&mut self, id: &'static str) -> Result<()> {
            let node = V::new(id, Self::COUNTER_START);
            self.multiverse
                .insert(node)
                .with_context(|| format!("Failed to insert root node {id}"))?;
            self.purge()?;
            Ok(())
        }

        pub fn contains(&self, key: &'static str) -> bool {
            self.multiverse.contains(&K::new(key))
        }

        pub fn purge(&mut self) -> Result<()> {
            let BestBlock {
                selected,
                discarded,
            } = self.multiverse.select_best_block(self.selection_rule);

            self.selected = selected.map(|k| k.inner().clone());

            for discarded in discarded {
                let id = discarded.inner();
                self.multiverse
                    .remove(&discarded)
                    .with_context(|| format!("failed to discarded node {id:?}"))?;
            }

            Ok(())
        }

        pub fn assert_selected(&self, expected: Option<&'static str>) -> Result<()> {
            match (self.selected.as_ref(), expected) {
                (None, None) => (),
                (None, Some(expected)) => bail!(
                    "expected to have {expected} as selected root",
                    expected = expected
                ),
                (Some(selected), None) => bail!(
                    "Expected no selected root but we have {selected:?}",
                    selected = selected
                ),
                (Some(selected), Some(expected)) => {
                    ensure!(
                        selected.is(expected),
                        "Expected node ({expected}) is different from the selected node ({selected:?})",
                        expected = expected,
                        selected = selected
                    );
                }
            }
            Ok(())
        }

        pub fn insert(&mut self, parent: &'static str, id: &'static str) -> Result<()> {
            let parent = if let Some(parent) = self.multiverse.get(&K::new(parent)) {
                parent.clone()
            } else {
                anyhow::bail!(
                    "Missing parent {parent} of block {id}",
                    parent = parent,
                    id = id
                )
            };
            let node = parent.mk_child(id);
            self.multiverse.insert(node).with_context(|| {
                format!(
                    "Failed to insert node {id} with parent {parent:?}",
                    id = id,
                    parent = parent.id()
                )
            })?;
            self.purge()?;
            Ok(())
        }
    }

    impl Default for Simulation {
        fn default() -> Self {
            Self {
                multiverse: Multiverse::temporary().unwrap(),
                selection_rule: BestBlockSelectionRule::LongestChain {
                    depth: 3,
                    age_gap: 1,
                },
                selected: None,
            }
        }
    }

    /// so we have:
    ///
    /// ```text
    ///             /-A---B---C
    ///            |
    ///     Root*--+--A'--B'--C'
    /// ```
    ///
    /// We receive a new block for branch A. And so we have a  depth of 3 and a delta of 2: So we expect to:
    /// keep root
    /// make A our selected block
    ///
    /// ```text
    ///         /-A*--B---C---D
    ///        |
    /// Root --+--A'--B'--C'
    /// ```
    ///
    /// We receive another block, it goes on branch A. And so we have a  depth of 3 and a delta of 2: So we expect to:
    /// we keep Root
    /// make B our selected block
    ///
    /// ```text
    ///         /-A---B*--C---D---E
    ///        |
    /// Root --+--A'--B'--C'
    /// ```
    ///
    /// We receive another block, it goes on branch A. And so we have a  depth of 3 and a delta of 2: So we expect to:
    /// we remote Root
    /// make C our selected block
    ///
    /// ```text
    /// A---B---C*--D---E---F
    /// A'--B'--C'
    /// ```
    ///
    #[test]
    fn multiverse_sim_1() -> anyhow::Result<()> {
        let mut sim = Simulation::default();

        //         /-A---B---C
        //        |
        // Root*--+--A'--B'--C'
        sim.push("Root")?;
        sim.insert("Root", "A")?;
        sim.insert("A", "B")?;
        sim.insert("B", "C")?;
        sim.insert("Root", "A'")?;
        sim.insert("A'", "B'")?;
        sim.insert("B'", "C'")?;

        sim.assert_selected(Some("Root"))?;

        //         /-A*--B---C---D
        //        |
        // Root --+--A'--B'--C'
        sim.insert("C", "D")?;

        sim.assert_selected(Some("A"))?;

        //         /-A---B*--C---D---E
        //        |
        // Root --+--A'--B'--C'
        sim.insert("D", "E")?;

        sim.assert_selected(Some("B"))?;

        //           A---B---C*--D---E---F
        //           A'--B'--C'
        sim.insert("E", "F")?;
        sim.assert_selected(Some("C"))?;
        assert!(!sim.contains("Root"), "`Root` should have been removed");

        Ok(())
    }
}
