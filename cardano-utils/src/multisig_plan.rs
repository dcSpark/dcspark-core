use anyhow::{anyhow, Context as _};
use cardano_multiplatform_lib::address::{Address, EnterpriseAddress, StakeCredential};
use cardano_multiplatform_lib::crypto::{Ed25519KeyHash, ScriptHash};
use cardano_multiplatform_lib::{NativeScript, NativeScripts, ScriptNOfK, ScriptPubkey};
use deps::serde_json;
use serde::{Deserialize, Deserializer};
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct MultisigPlan {
    pub quorum: u32,
    pub keys: Vec<Hash>,
}

impl MultisigPlan {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)
            .with_context(|| anyhow!("failed to read multisig plan from {}", path.display()))?;
        serde_json::from_reader(file)
            .with_context(|| anyhow!("failed to read multisig plan from {}", path.display()))
    }

    pub fn hash(&self) -> ScriptHash {
        let script = self.to_script().get(0).hash().to_bytes();

        ScriptHash::from_bytes(script)
            .map_err(|error| anyhow!("Invalid hash {}", error))
            .expect("Script should be valid all the time already")
    }

    pub fn address(&self, network_id: u8) -> Address {
        let native_bytes = self.hash();

        let address = StakeCredential::from_scripthash(&native_bytes);
        EnterpriseAddress::new(network_id, &address).to_address()
    }

    pub fn to_script(&self) -> NativeScripts {
        let keys = {
            let mut scripts = NativeScripts::new();

            for key in self.keys.iter().map(|k| &k.0) {
                scripts.add(&NativeScript::new_script_pubkey(&ScriptPubkey::new(key)));
            }

            scripts
        };

        let mut scripts = NativeScripts::new();
        let script = ScriptNOfK::new(self.quorum, &keys);
        let script = NativeScript::new_script_n_of_k(&script);
        scripts.add(&script);

        scripts
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hash(#[serde(deserialize_with = "deserialize_key_hash")] pub Ed25519KeyHash);

fn deserialize_key_hash<'de, D>(deserializer: D) -> Result<Ed25519KeyHash, D::Error>
where
    D: Deserializer<'de>,
    D::Error: serde::de::Error,
{
    use serde::de::Error as _;

    let bytes = String::deserialize(deserializer)?;
    let bytes = hex::decode(bytes).map_err(D::Error::custom)?;

    Ed25519KeyHash::from_bytes(bytes).map_err(D::Error::custom)
}
