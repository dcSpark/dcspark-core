use crate::fingerprint;
use crate::payment_credentials::CardanoPaymentCredentials;
use anyhow::anyhow;
use cardano_multiplatform_lib::builders::input_builder::{InputBuilderResult, SingleInputBuilder};
use cardano_multiplatform_lib::crypto::TransactionHash;
use cardano_multiplatform_lib::ledger::common::value::{BigNum, Coin};
use cardano_multiplatform_lib::plutus::ScriptRef;
use cardano_multiplatform_lib::{Datum, MultiAsset, PolicyID, TransactionInput, TransactionOutput};
use dcspark_core::tx::{TransactionAsset, TransactionId, UTxOBuilder, UTxODetails, UtxoPointer};
use dcspark_core::{Address, AssetName, OutputIndex, PolicyId, Regulated, TokenId, Value};
use deps::bigdecimal::ToPrimitive;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CardanoUTxOExtra {
    script_ref: Option<ScriptRef>,
    datum: Option<Datum>,
}

pub fn utxo_details_to_cml_input(
    details: &UTxODetails,
    creds_kind: &CardanoPaymentCredentials,
) -> anyhow::Result<InputBuilderResult> {
    let transaction_id = TransactionHash::from_hex(details.pointer.transaction_id.as_ref())
        .map_err(|err| anyhow!("can't convert input during hash conversion: {}", err))?;
    let index = BigNum::from(u64::from(details.pointer.output_index));

    let address =
        cardano_multiplatform_lib::address::Address::from_bech32(details.address.as_ref())
            .map_err(|err| anyhow!("can't convert input during address conversion: {}", err))?;

    let mut assets_map = HashMap::new();
    details.assets.iter().for_each(|asset: &TransactionAsset| {
        assets_map.insert(asset.fingerprint.clone(), asset.clone());
    });

    let value = tokens_to_csl_value(&details.value, &assets_map)
        .map_err(|err| anyhow!("can't convert value: {}", err))?;

    let mut output = TransactionOutput::new(&address, &value);
    if let Some(extra) = &details.extra {
        let utxo_extra: CardanoUTxOExtra = serde_json::from_str(extra)
            .map_err(|err| anyhow!("can't parse cardano extra: {}", err))?;
        if let Some(script_ref) = &utxo_extra.script_ref {
            output.set_script_ref(script_ref);
        }
        if let Some(datum) = &utxo_extra.datum {
            output.set_datum(datum);
        }
    }

    let builder = SingleInputBuilder::new(&TransactionInput::new(&transaction_id, &index), &output);

    match creds_kind {
        CardanoPaymentCredentials::PaymentKey => builder
            .payment_key()
            .map_err(|err| anyhow!("can't build utxo input by public key: {}", err)),
        CardanoPaymentCredentials::PlutusScript {
            partial_witness,
            required_signers,
            datum,
        } => builder
            .plutus_script(partial_witness, required_signers, datum)
            .map_err(|err| anyhow!("can't build utxo input by plutus script: {}", err)),
        CardanoPaymentCredentials::NativeScript {
            native_script,
            witness_info,
        } => builder
            .native_script(native_script, witness_info)
            .map_err(|err| anyhow!("can't build utxo input by native script: {}", err)),
    }
}

pub fn utxo_details_from_io(
    value: (TransactionInput, TransactionOutput),
) -> anyhow::Result<UTxODetails> {
    let (ada_value, tokens) = csl_value_to_tokens(&value.1.amount())?;
    Ok(UTxODetails {
        pointer: UtxoPointer {
            transaction_id: TransactionId::new(value.0.transaction_id().to_string()),
            output_index: OutputIndex::new(u64::from(value.0.index())),
        },
        address: Address::new(
            value
                .1
                .address()
                .to_bech32(None)
                .map_err(|err| anyhow!("can't convert address {}", err))?,
        ),
        value: ada_value,
        assets: tokens.values().cloned().collect::<Vec<_>>(),
        metadata: Arc::new(Default::default()),
        extra: Some(
            serde_json::to_string(&CardanoUTxOExtra {
                script_ref: value.1.script_ref(),
                datum: value.1.datum(),
            })
            .map_err(|err| anyhow!("can't serialize extra: {}", err))?,
        ),
    })
}

fn value_to_csl_coin(value: &Value<Regulated>) -> anyhow::Result<Coin> {
    Ok(Coin::from(value.to_u64().ok_or_else(|| {
        anyhow!("Can't convert input balance to u64")
    })?))
}

fn tokens_to_csl_value(
    coin: &Value<Regulated>,
    assets: &HashMap<TokenId, TransactionAsset>,
) -> anyhow::Result<cardano_multiplatform_lib::ledger::common::value::Value> {
    let coin = value_to_csl_coin(coin)?;
    let mut value = cardano_multiplatform_lib::ledger::common::value::Value::new(&coin);
    if !assets.is_empty() {
        let mut multi_assets = MultiAsset::new();
        for (_, asset) in assets.iter() {
            let decoded_policy_id = hex::decode(asset.policy_id.to_string())
                .map_err(|err| anyhow!("Failed to decode the policy id: hex error {err}"))?;

            let policy_id = PolicyID::from_bytes(decoded_policy_id)
                .map_err(|error| anyhow!("Failed to decode the policy id: {error}"))?;

            let decoded_asset_name = hex::decode(asset.asset_name.as_ref())
                .map_err(|err| anyhow!("Failed to decode the asset name: hex error {err}"))?;
            let asset_name = cardano_multiplatform_lib::AssetName::new(decoded_asset_name)
                .map_err(|err| anyhow!("Failed to decode asset name: {err}"))?;

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

fn multiasset_iter<F>(
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

fn csl_coin_to_value(value: &Coin) -> anyhow::Result<Value<Regulated>> {
    let value = Value::new(deps::bigdecimal::BigDecimal::from(u64::from(*value)));

    Ok(value)
}

fn csl_value_to_tokens(
    value: &cardano_multiplatform_lib::ledger::common::value::Value,
) -> anyhow::Result<(Value<Regulated>, HashMap<TokenId, TransactionAsset>)> {
    let coin = csl_coin_to_value(&value.coin())?;
    let mut tokens = HashMap::<TokenId, TransactionAsset>::new();
    multiasset_iter(value, |policy_id, asset_name, quantity| {
        let policy_id = PolicyId::new(hex::encode(policy_id.to_bytes()));
        let asset_name = AssetName::new(hex::encode(asset_name.to_bytes()));
        let fingerprint = fingerprint(&policy_id, &asset_name)
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

pub fn utxo_builder_to_cml_output(builder: &UTxOBuilder) -> anyhow::Result<TransactionOutput> {
    let address =
        cardano_multiplatform_lib::address::Address::from_bech32(builder.address.as_ref())
            .map_err(|err| anyhow!("can't convert output during address conversion: {}", err))?;

    let mut assets_map = HashMap::new();
    builder.assets.iter().for_each(|asset: &TransactionAsset| {
        assets_map.insert(asset.fingerprint.clone(), asset.clone());
    });

    let value = tokens_to_csl_value(&builder.value, &assets_map)
        .map_err(|err| anyhow!("can't convert value: {}", err))?;

    let mut output = TransactionOutput::new(&address, &value);

    if let Some(extra) = &builder.extra {
        let utxo_extra: CardanoUTxOExtra = serde_json::from_str(extra)
            .map_err(|err| anyhow!("can't parse cardano extra: {}", err))?;
        if let Some(script_ref) = &utxo_extra.script_ref {
            output.set_script_ref(script_ref);
        }
        if let Some(datum) = &utxo_extra.datum {
            output.set_datum(datum);
        }
    }

    Ok(output)
}

pub fn utxo_builder_from_output(value: TransactionOutput) -> anyhow::Result<UTxOBuilder> {
    let (ada_value, tokens) = csl_value_to_tokens(&value.amount())?;
    Ok(UTxOBuilder {
        address: Address::new(
            value
                .address()
                .to_bech32(None)
                .map_err(|err| anyhow!("can't convert address {}", err))?,
        ),
        value: ada_value,
        assets: tokens.values().cloned().collect::<Vec<_>>(),
        extra: Some(
            serde_json::to_string(&CardanoUTxOExtra {
                script_ref: value.script_ref(),
                datum: value.datum(),
            })
            .map_err(|err| anyhow!("can't serialize extra: {}", err))?,
        ),
    })
}
