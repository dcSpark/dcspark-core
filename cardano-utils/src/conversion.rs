use anyhow::anyhow;
use cardano_multiplatform_lib::ledger::common::value::{BigNum, Coin};
use cardano_multiplatform_lib::{MultiAsset, PolicyID};
use dcspark_core::tx::TransactionAsset;
use dcspark_core::{AssetName, PolicyId, Regulated, TokenId, Value};
use deps::bigdecimal::ToPrimitive;
use std::collections::HashMap;

pub fn value_to_csl_coin(value: &Value<Regulated>) -> anyhow::Result<Coin> {
    Ok(
        cardano_multiplatform_lib::ledger::common::value::Coin::from(
            value
                .to_u64()
                .ok_or_else(|| anyhow!("Can't convert input balance to u64"))?,
        ),
    )
}

pub fn csl_coin_to_value(value: &Coin) -> anyhow::Result<Value<Regulated>> {
    Ok(Value::new(deps::bigdecimal::BigDecimal::from(u64::from(
        *value,
    ))))
}

pub fn multiasset_iter<F>(
    value: &cardano_multiplatform_lib::ledger::common::value::Value,
    mut f: F,
) -> anyhow::Result<()>
where
    F: FnMut(
        &PolicyID,
        &cardano_multiplatform_lib::AssetName,
        &Option<BigNum>,
    ) -> anyhow::Result<()>,
{
    if let Some(ma) = &value.multiasset() {
        let policy_ids = ma.keys();
        for policy_id_index in 0..policy_ids.len() {
            let policy_id = policy_ids.get(policy_id_index);
            let assets = if let Some(assets) = ma.get(&policy_id) {
                assets
            } else {
                continue;
            };
            let asset_names = assets.keys();
            for asset_name_index in 0..asset_names.len() {
                let asset_name = asset_names.get(asset_name_index);
                let quantity = assets.get(&asset_name);
                f(&policy_id, &asset_name, &quantity)?;
            }
        }
    }
    Ok(())
}

pub fn tokens_to_csl_value(
    coin: &Value<Regulated>,
    assets: &HashMap<TokenId, TransactionAsset>,
) -> anyhow::Result<cardano_multiplatform_lib::ledger::common::value::Value> {
    let coin = value_to_csl_coin(coin)?;
    let mut value = cardano_multiplatform_lib::ledger::common::value::Value::new(&coin);
    if !assets.is_empty() {
        let mut multi_assets = MultiAsset::new();
        for (_, asset) in assets.iter() {
            let policy_id = PolicyID::from_bytes(
                hex::decode(asset.policy_id.to_string())
                    .map_err(|err| anyhow!("Failed to decode the policy id: hex error {err}"))?,
            )
            .map_err(|error| anyhow!("Failed to decode the policy id: {error}"))?;
            let asset_name = cardano_multiplatform_lib::AssetName::new(
                hex::decode(asset.asset_name.as_ref())
                    .map_err(|err| anyhow!("Failed to decode the asset name {err}"))?,
            )
            .map_err(|err| anyhow!("can't decode asset_name {err}"))?;

            let value =
                BigNum::from_str(&asset.quantity.truncate().to_string()).map_err(|error| {
                    anyhow!(
                        "Value {value} ({fingerprint}) was not within the boundaries: {error}",
                        value = asset.quantity.truncate(),
                        fingerprint = asset.fingerprint,
                    )
                })?;

            multi_assets.set_asset(&policy_id, &asset_name, &value);
        }

        value.set_multiasset(&multi_assets);
    }

    Ok(value)
}

pub fn csl_value_to_tokens(
    value: &cardano_multiplatform_lib::ledger::common::value::Value,
) -> anyhow::Result<(Value<Regulated>, HashMap<TokenId, TransactionAsset>)> {
    let coin = csl_coin_to_value(&value.coin())?;
    let mut tokens = HashMap::<TokenId, TransactionAsset>::new();
    multiasset_iter(value, |policy_id, asset_name, quantity| {
        let policy_id = PolicyId::new(hex::encode(policy_id.to_bytes()));
        let asset_name = AssetName::new(hex::encode(asset_name.to_bytes()));
        let fingerprint = crate::cip14::fingerprint(&policy_id, &asset_name)
            .map_err(|err| anyhow!("Can't create fingerprint {err}"))?;
        let quantity = quantity.ok_or_else(|| anyhow!("not found asset quantity"))?;
        let asset = TransactionAsset {
            policy_id,
            asset_name,
            fingerprint: fingerprint.clone(),
            quantity: csl_coin_to_value(&quantity)?,
        };
        tokens.insert(fingerprint, asset);
        Ok(())
    })?;
    Ok((coin, tokens))
}
