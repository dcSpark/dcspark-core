use crate::tx::{TransactionAsset, TransactionId};
use crate::{Address, AssetName, OutputIndex, PolicyId, Regulated, TokenId, Value};
use anyhow::anyhow;
use cardano_multiplatform_lib::builders::input_builder::{InputBuilderResult, SingleInputBuilder};
use cardano_multiplatform_lib::builders::witness_builder::{
    NativeScriptWitnessInfo, PartialPlutusWitness,
};
use cardano_multiplatform_lib::crypto::TransactionHash;
use cardano_multiplatform_lib::ledger::common::value::{BigNum, Coin};
use cardano_multiplatform_lib::plutus::{PlutusData, ScriptRef};
use cardano_multiplatform_lib::{
    Datum, MultiAsset, NativeScript, PolicyID, RequiredSigners, TransactionInput, TransactionOutput,
};
use deps::bigdecimal::ToPrimitive;
use deps::serde_json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Points to particular UTxO for some ['TransactionId'].
/// We can have multiple pointers with different indexes for the same transaction.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct UtxoPointer {
    pub transaction_id: TransactionId,
    pub output_index: OutputIndex,
}

impl fmt::Display for UtxoPointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{hash}@{index}",
            hash = self.transaction_id,
            index = self.output_index,
        )
    }
}

/// list the details of the UTxO
///
/// this is the information that we will collect int he UTxO store
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct UTxODetails {
    pub pointer: UtxoPointer,
    pub address: Address,
    pub value: Value<Regulated>,

    #[serde(default)]
    pub assets: Vec<TransactionAsset>,
    pub metadata: Arc<serde_json::Value>,

    #[serde(default)]
    pub extra: Option<String>,
}

#[derive(Debug, Clone)]
pub enum CardanoPaymentCredentials {
    PlutusScript {
        partial_witness: PartialPlutusWitness,
        required_signers: RequiredSigners,
        datum: PlutusData,
    },
    PaymentKey,
    NativeScript {
        native_script: NativeScript,
        witness_info: NativeScriptWitnessInfo,
    },
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CardanoUTxOExtra {
    script_ref: Option<ScriptRef>,
    datum: Option<Datum>,
}

impl UTxODetails {
    pub fn to_cml_input(
        &self,
        creds_kind: &CardanoPaymentCredentials,
    ) -> anyhow::Result<InputBuilderResult> {
        let transaction_id = TransactionHash::from_hex(self.pointer.transaction_id.as_ref())
            .map_err(|err| anyhow!("can't convert input during hash conversion: {}", err))?;
        let index = BigNum::from(u64::from(self.pointer.output_index));

        let address =
            cardano_multiplatform_lib::address::Address::from_bech32(self.address.as_ref())
                .map_err(|err| anyhow!("can't convert input during address conversion: {}", err))?;

        let mut assets_map = HashMap::new();
        self.assets.iter().for_each(|asset: &TransactionAsset| {
            assets_map.insert(asset.fingerprint.clone(), asset.clone());
        });

        let value = tokens_to_csl_value(&self.value, &assets_map)
            .map_err(|err| anyhow!("can't convert value: {}", err))?;

        let mut output = TransactionOutput::new(&address, &value);
        if let Some(extra) = &self.extra {
            let utxo_extra: CardanoUTxOExtra = serde_json::from_str(extra)
                .map_err(|err| anyhow!("can't parse cardano extra: {}", err))?;
            if let Some(script_ref) = &utxo_extra.script_ref {
                output.set_script_ref(script_ref);
            }
            if let Some(datum) = &utxo_extra.datum {
                output.set_datum(datum);
            }
        }

        let builder =
            SingleInputBuilder::new(&TransactionInput::new(&transaction_id, &index), &output);

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
}

impl TryFrom<(TransactionInput, TransactionOutput)> for UTxODetails {
    type Error = anyhow::Error;

    fn try_from(value: (TransactionInput, TransactionOutput)) -> Result<Self, Self::Error> {
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
}

pub fn value_to_csl_coin(value: &Value<Regulated>) -> anyhow::Result<Coin> {
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

pub fn csl_coin_to_value(value: &Coin) -> anyhow::Result<Value<Regulated>> {
    let value = Value::new(deps::bigdecimal::BigDecimal::from(u64::from(*value)));

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
        let fingerprint = crate::cardano_utils::fingerprint(&policy_id, &asset_name)
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

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct UTxOBuilder {
    pub address: Address,
    pub value: Value<Regulated>,
    pub assets: Vec<TransactionAsset>,

    #[serde(default)]
    pub extra: Option<String>,
}

impl UTxOBuilder {
    pub fn new(address: Address, value: Value<Regulated>, assets: Vec<TransactionAsset>) -> Self {
        Self {
            address,
            value,
            assets,
            extra: None,
        }
    }

    pub fn to_cml_output(&self) -> anyhow::Result<TransactionOutput> {
        let address =
            cardano_multiplatform_lib::address::Address::from_bech32(self.address.as_ref())
                .map_err(|err| {
                    anyhow!("can't convert output during address conversion: {}", err)
                })?;

        let mut assets_map = HashMap::new();
        self.assets.iter().for_each(|asset: &TransactionAsset| {
            assets_map.insert(asset.fingerprint.clone(), asset.clone());
        });

        let value = tokens_to_csl_value(&self.value, &assets_map)
            .map_err(|err| anyhow!("can't convert value: {}", err))?;

        let mut output = TransactionOutput::new(&address, &value);

        if let Some(extra) = &self.extra {
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
}

impl TryFrom<TransactionOutput> for UTxOBuilder {
    type Error = anyhow::Error;

    fn try_from(value: TransactionOutput) -> Result<Self, Self::Error> {
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
}
