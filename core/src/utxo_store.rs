use crate::tx::{UTxODetails, UtxoPointer};
use crate::{AssetName, PolicyId, Regulated, TokenId, Value};
use anyhow::anyhow;
use imbl::{hashmap::Entry, HashMap};
use std::collections::BTreeMap;
use std::ops::{AddAssign, SubAssign};
use std::sync::Arc;

/// store for Unspent Transaction Output
///
/// efficient storage of UTxO for the multiverse data model
/// using a Hamt to efficiently share the memory between
/// the different states of the UTxO within the Multiverse
#[derive(Default, Clone)]
pub struct UTxOStore {
    utxos: UTxOSet,
    by_policy_id: HashMap<TokenId, UTxOSet>,

    /// keep the hashmap of the known TokenId/AssetName
    ///
    dictionary: HashMap<TokenId, (PolicyId, AssetName)>,
}

#[derive(Default, Clone)]
struct UTxOSet {
    token_id: TokenId,
    balance: Value<Regulated>,
    set: HashMap<UtxoPointer, Arc<UTxODetails>>,
    ordered_by_value: BTreeMap<Value<Regulated>, HashMap<UtxoPointer, Arc<UTxODetails>>>,
}

pub struct UTxOStoreMut {
    utxos: UTxOSet,
    by_policy_id: HashMap<TokenId, UTxOSet>,
    dictionary: HashMap<TokenId, (PolicyId, AssetName)>,
}

impl UTxOSet {
    pub fn remove_from_asset(&mut self, pointer: &UtxoPointer) -> Option<Arc<UTxODetails>> {
        let utxo: Arc<UTxODetails> = self.set.remove(pointer)?;

        let value = if self.token_id == TokenId::MAIN {
            utxo.value.clone()
        } else {
            utxo.assets
                .iter()
                .find(|ta| ta.fingerprint == self.token_id)
                .map(|ta| &ta.quantity)
                .cloned()
                .expect("We know we already have the value")
        };

        self.finish_remove(value, pointer);

        Some(utxo)
    }
}

impl UTxOSet {
    pub fn remove_from_main(&mut self, pointer: &UtxoPointer) -> Option<Arc<UTxODetails>> {
        let utxo: Arc<UTxODetails> = self.set.remove(pointer)?;
        let value = utxo.value.clone();

        self.finish_remove(value, pointer);

        Some(utxo)
    }
}

impl UTxOSet {
    pub fn add_value(&mut self, token: &TokenId, value: Value<Regulated>, utxo: Arc<UTxODetails>) {
        self.token_id = token.clone();
        self.balance.add_assign(value.clone());
        self.set.insert(utxo.pointer.clone(), utxo.clone());
        match self.ordered_by_value.entry(value) {
            std::collections::btree_map::Entry::Vacant(vacant) => {
                let mut set = HashMap::new();
                set.insert(utxo.pointer.clone(), utxo);
                vacant.insert(set);
            }
            std::collections::btree_map::Entry::Occupied(mut occupied) => {
                let inner: &mut HashMap<UtxoPointer, Arc<UTxODetails>> = occupied.get_mut();
                inner.insert(utxo.pointer.clone(), utxo);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.set.len() == 0
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    #[inline]
    pub fn contains_key(&self, utxo: &UtxoPointer) -> bool {
        self.set.contains_key(utxo)
    }

    #[inline]
    pub fn get(&self, utxo: &UtxoPointer) -> Option<&UTxODetails> {
        self.set.get(utxo).map(|v| v.as_ref())
    }

    #[inline]
    pub fn iter(&self) -> imbl::hashmap::Iter<'_, UtxoPointer, Arc<UTxODetails>> {
        self.set.iter()
    }

    pub fn ordered_utxo_iterator(&self) -> impl Iterator<Item = &UTxODetails> {
        self.ordered_by_value
            .values()
            .flat_map(|i| i.values().map(|v| v.as_ref()))
    }

    pub fn ordered_utxo_iterator_rev(&self) -> impl Iterator<Item = &UTxODetails> {
        self.ordered_by_value
            .values()
            .rev()
            .flat_map(|i| i.values().map(|v| v.as_ref()))
    }

    fn finish_remove(&mut self, value: Value<Regulated>, pointer: &UtxoPointer) {
        if let std::collections::btree_map::Entry::Occupied(mut occupied) =
            self.ordered_by_value.entry(value.clone())
        {
            let inner: &mut HashMap<UtxoPointer, Arc<UTxODetails>> = occupied.get_mut();
            inner.remove(pointer);
            if inner.is_empty() {
                occupied.remove();
            }
        }
        self.balance.sub_assign(value);
    }
}

impl UTxOStore {
    /// create a new, empty, state
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// returns true is the State does not contains any UTxOs
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.utxos.is_empty()
    }

    /// returns the number of UTxO in the state
    #[inline]
    pub fn len(&self) -> usize {
        self.utxos.len()
    }

    /// Get the number of UTxOs for the given token.
    ///
    #[inline]
    pub fn number_utxos_for_token(&self, token: &TokenId) -> usize {
        self.by_token_id(token)
            .map(|set| set.len())
            .unwrap_or_default()
    }

    /// check if the State contains the given UTxO
    #[inline]
    pub fn contains(&self, utxo: &UtxoPointer) -> bool {
        self.utxos.contains_key(utxo)
    }

    /// check if the State contains a given [`TokenId`]
    #[inline]
    pub fn contains_token(&self, token_id: &TokenId) -> bool {
        token_id == &self.utxos.token_id || self.by_policy_id.contains_key(token_id)
    }

    /// retrieve the asset identifier from the TokenId
    #[inline]
    pub fn get_asset_ids(&self, token_id: &TokenId) -> Option<&(PolicyId, AssetName)> {
        self.dictionary.get(token_id)
    }

    #[must_use = "This function does not modify the internal state"]
    pub fn thaw(&self) -> UTxOStoreMut {
        UTxOStoreMut {
            utxos: self.utxos.clone(),
            by_policy_id: self.by_policy_id.clone(),
            dictionary: self.dictionary.clone(),
        }
    }

    /// get the [`UTxODetails`] associated to the [`UtxoPointer`]
    ///
    /// Returns [`None`] if the utxo is not present in the state
    #[inline]
    pub fn get(&self, utxo: &UtxoPointer) -> Option<&UTxODetails> {
        self.utxos.get(utxo)
    }

    #[inline]
    pub fn iter(&self) -> imbl::hashmap::Iter<'_, UtxoPointer, Arc<UTxODetails>> {
        self.utxos.iter()
    }

    #[inline]
    pub fn iter_ordered_by_wmain(&self) -> impl Iterator<Item = &UTxODetails> {
        self.utxos.ordered_utxo_iterator()
    }

    /// list all UTxO that are associated to the given [`TokenId`]
    ///
    /// The iterator may be empty if there is no [`TokenId`] present
    /// in the store
    #[inline]
    pub fn iter_token(
        &self,
        token_id: &TokenId,
    ) -> impl Iterator<Item = (&UtxoPointer, &Arc<UTxODetails>)> {
        self.by_token_id(token_id)
            .into_iter()
            .flat_map(|c| c.iter())
    }

    /// list all UTxO that are associated to the given [`TokenId`] ordered by total balance, in
    /// *ascending* way.
    ///
    /// The iterator may be empty if there is no [`TokenId`] present in the store
    #[inline]
    pub fn iter_token_ordered_by_value(
        &self,
        token_id: &TokenId,
    ) -> impl Iterator<Item = &UTxODetails> {
        self.by_token_id(token_id)
            .into_iter()
            .flat_map(|set| set.ordered_utxo_iterator())
    }

    /// list all UTxO that are associated to the given [`TokenId`] ordered by total balance, in
    /// *descending* way.
    ///
    /// The iterator may be empty if there is no [`TokenId`] present in the store
    #[inline]
    pub fn iter_token_ordered_by_value_rev(
        &self,
        token_id: &TokenId,
    ) -> impl Iterator<Item = &UTxODetails> {
        self.by_token_id(token_id)
            .into_iter()
            .flat_map(|set| set.ordered_utxo_iterator_rev())
    }

    /// get the balance of a given asset
    #[inline]
    pub fn get_balance_of(&self, token: &TokenId) -> Option<Value<Regulated>> {
        self.by_token_id(token).map(|set| set.balance.clone())
    }

    /// get the utxo set for the given token, considering both the primary/main token and the
    /// assets
    #[inline]
    fn by_token_id(&self, token: &TokenId) -> Option<&UTxOSet> {
        self.by_policy_id.get(token)
    }
}

impl UTxOStoreMut {
    #[inline]
    pub fn remove(&mut self, utxo: &UtxoPointer) -> anyhow::Result<()> {
        if let Some(value) = self.utxos.remove_from_main(utxo) {
            for policy_id in value
                .assets
                .iter()
                .map(|a| &a.fingerprint)
                .chain(&[TokenId::MAIN])
            {
                let entry = self.by_policy_id.entry(policy_id.clone()).and_modify(|c| {
                    c.remove_from_asset(utxo);
                });

                // if we have removed the last entry from the UTxO we want to remove the entry
                // from the `by_policy_id`. It's a small optimization
                if let Entry::Occupied(occupied) = entry {
                    if occupied.get().is_empty() {
                        occupied.remove();
                    }
                } else {
                    // Nothing to do here: this situation is not worth handling.
                }
            }

            Ok(())
        } else {
            Err(anyhow!("Utxo is not found {:?}", utxo.clone()))
        }
    }

    #[inline]
    pub fn insert_compat(&mut self, utxo: UTxODetails) -> anyhow::Result<()> {
        self.insert(utxo)
    }

    /// insert the given UTxO in the mutable state
    #[inline]
    pub fn insert(&mut self, utxo: UTxODetails) -> anyhow::Result<()> {
        let pointer = utxo.pointer.clone();
        let utxo_details = Arc::new(utxo);
        if self.utxos.contains_key(&pointer) {
            Err(anyhow!("Pointer {pointer} is inserted already"))
        } else {
            let value = utxo_details.value.clone();

            self.utxos
                .add_value(&TokenId::MAIN, value, utxo_details.clone());

            if utxo_details.assets.is_empty() {
                // so we know this is a pure Ada UTxO so we need to be able to select
                // this value in the `by_policy_id`.

                self.by_policy_id
                    .entry(TokenId::MAIN)
                    .or_default()
                    .add_value(
                        &TokenId::MAIN,
                        utxo_details.value.clone(),
                        utxo_details.clone(),
                    );
            }

            for asset in utxo_details.assets.iter() {
                self.by_policy_id
                    .entry(asset.fingerprint.clone())
                    .or_default()
                    .add_value(
                        &asset.fingerprint,
                        asset.quantity.clone(),
                        utxo_details.clone(),
                    );

                // We want to populate the dictionary only if we don't
                // already have the entry
                self.dictionary
                    .entry(asset.fingerprint.clone())
                    .or_insert_with(|| (asset.policy_id.clone(), asset.asset_name.clone()));
            }
            Ok(())
        }
    }

    pub fn token_balance(&self, token_id: &TokenId) -> Option<Value<Regulated>> {
        self.by_policy_id
            .get(token_id)
            .map(|set| set.balance.clone())
    }

    pub fn balance(&self) -> Value<Regulated> {
        self.utxos.balance.clone()
    }

    /// consume the mutable state releasing a new state that is immutable
    ///
    /// this function does not modify any other state and the returned value
    /// is the result of the freeze.
    #[must_use = "This function does not modify the internal state"]
    pub fn freeze(self) -> UTxOStore {
        UTxOStore {
            utxos: self.utxos,
            by_policy_id: self.by_policy_id,
            dictionary: self.dictionary,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tx::{TransactionAsset, TransactionId, UTxODetails, UtxoPointer};
    use crate::utxo_store::UTxOSet;
    use crate::{
        cardano, Address, AssetName, OutputIndex, PolicyId, Regulated, TokenId, UTxOStore, Value,
    };
    use deps::bigdecimal::BigDecimal;
    use rand::{thread_rng, RngCore};
    use std::sync::Arc;

    #[test]
    fn check_add_spend() {
        let store = UTxOStore::new();
        let mut mut_store = store.thaw();

        let sushi_quantity = Value::new(BigDecimal::from(15u64));
        let sushi_policy_id = PolicyId::new_static("policy_sushi");
        let sushi_token_id = TokenId::new_static("sushi");
        let shib_quantity = Value::new(BigDecimal::from(25u64));
        let shib_policy_id = PolicyId::new_static("shib");
        let shib_token_id = TokenId::new_static("shib");
        let ada_quantity = Value::<cardano::Lovelace>::new(BigDecimal::from(10_000_000u64));
        let first_pointer = UtxoPointer {
            transaction_id: TransactionId::new_static("first tx"),
            output_index: OutputIndex::new(0),
        };
        let utxo = UTxODetails {
            pointer: first_pointer.clone(),
            address: Address::new_static("wallet_address"),
            value: ada_quantity.to_regulated(),
            assets: vec![
                TransactionAsset {
                    policy_id: sushi_policy_id.clone(),
                    fingerprint: sushi_token_id.clone(),
                    asset_name: AssetName::new_static("sushi"),
                    quantity: sushi_quantity.clone(),
                },
                TransactionAsset {
                    policy_id: shib_policy_id,
                    fingerprint: shib_token_id.clone(),
                    asset_name: AssetName::new_static("shib"),
                    quantity: shib_quantity.clone(),
                },
            ],
            metadata: Default::default(),
        };
        assert!(mut_store.insert(utxo).is_ok());
        assert_eq!(
            mut_store.token_balance(&shib_token_id),
            Some(shib_quantity.clone()),
        );
        assert_eq!(
            mut_store.token_balance(&sushi_token_id),
            Some(sushi_quantity.clone()),
        );
        assert_eq!(mut_store.balance(), ada_quantity.to_regulated());
        let new_utxo = UTxODetails {
            pointer: UtxoPointer {
                transaction_id: TransactionId::new_static("second tx"),
                output_index: OutputIndex::new(0),
            },
            address: Address::new_static("wallet_address"),
            value: ada_quantity.to_regulated(),
            assets: vec![TransactionAsset {
                policy_id: sushi_policy_id,
                fingerprint: sushi_token_id.clone(),
                asset_name: AssetName::new_static("sushi"),
                quantity: sushi_quantity.clone(),
            }],
            metadata: Default::default(),
        };
        assert!(mut_store.insert(new_utxo).is_ok());
        assert_eq!(
            mut_store.token_balance(&sushi_token_id),
            Some(sushi_quantity.clone() + sushi_quantity.clone())
        );
        assert_eq!(mut_store.token_balance(&shib_token_id), Some(shib_quantity));
        assert_eq!(
            mut_store.balance(),
            (ada_quantity.clone() + ada_quantity.clone()).to_regulated()
        );

        assert!(mut_store.remove(&first_pointer).is_ok());
        assert_eq!(
            mut_store.token_balance(&sushi_token_id),
            Some(sushi_quantity.clone())
        );
        assert_eq!(mut_store.token_balance(&shib_token_id), None);
        assert_eq!(mut_store.balance(), ada_quantity.to_regulated());
        let frozen = mut_store.freeze();
        let mut_store = frozen.thaw();
        assert_eq!(
            mut_store.token_balance(&sushi_token_id),
            Some(sushi_quantity)
        );
        assert_eq!(mut_store.token_balance(&shib_token_id), None);
        assert_eq!(mut_store.balance(), ada_quantity.to_regulated());
    }

    fn check_sorted(vec: Vec<Value<Regulated>>) -> bool {
        for i in 1..vec.len() {
            if vec[i - 1] > vec[i] {
                return false;
            }
        }
        true
    }

    fn generate_utxo_set_and_check_order(values: Vec<Value<cardano::Ada>>) {
        let mut utxo_set = UTxOSet::default();
        for (i, value) in values.iter().enumerate() {
            let pointer = UtxoPointer {
                transaction_id: TransactionId::new_static("first tx"),
                output_index: OutputIndex::new(i as u64),
            };
            let utxo = UTxODetails {
                pointer,
                address: Address::new_static("wallet_address"),
                value: value.to_lovelace().to_regulated(),
                assets: vec![],
                metadata: Default::default(),
            };
            utxo_set.add_value(
                &TokenId::MAIN,
                value.to_lovelace().to_regulated(),
                Arc::new(utxo),
            );
        }

        let values = utxo_set
            .ordered_utxo_iterator()
            .map(|utxo| utxo.value.clone())
            .collect();
        assert!(check_sorted(values));
    }

    #[test]
    fn check_ordered_walk1() {
        generate_utxo_set_and_check_order(
            (0..5).map(|i| Value::from(BigDecimal::from(i))).collect(),
        );
        generate_utxo_set_and_check_order(
            (0..5)
                .map(|i| Value::from(BigDecimal::from(100 - i)))
                .collect(),
        );
        let mut rng = thread_rng();
        generate_utxo_set_and_check_order(
            (0..10)
                .map(|_| Value::from(BigDecimal::from(rng.next_u64())))
                .collect(),
        )
    }
}
