use dcspark_core::tx::{TransactionAsset, TransactionId, UTxODetails, UtxoPointer};
use dcspark_core::{Address, AssetName, OutputIndex, PolicyId, Regulated, TokenId, Value};
use std::sync::Arc;

#[allow(unused)]
pub fn create_utxo(
    tx: u64,
    index: u64,
    address: String,
    value: Value<Regulated>,
    assets: Vec<TransactionAsset>,
) -> UTxODetails {
    UTxODetails {
        pointer: UtxoPointer {
            transaction_id: TransactionId::new(tx.to_string()),
            output_index: OutputIndex::new(index),
        },
        address: Address::new(address),
        value,
        assets,
        metadata: Arc::new(Default::default()),
        extra: None,
    }
}

#[allow(unused)]
pub fn create_asset(fingerprint: String, quantity: Value<Regulated>) -> TransactionAsset {
    let fingerprint = TokenId::new(fingerprint);
    TransactionAsset {
        policy_id: PolicyId::new(fingerprint.as_ref().to_string()),
        asset_name: AssetName::new(fingerprint.as_ref().to_string()),
        fingerprint,
        quantity,
    }
}
