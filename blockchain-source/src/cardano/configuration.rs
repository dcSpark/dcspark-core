use super::time::Era;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct NetworkConfiguration {
    pub chain_info: cml_chain::genesis::network_info::NetworkInfo,
    pub relay: (String, u16),
    pub shelley_era_config: Era,
}

impl NetworkConfiguration {
    pub fn mainnet() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::mainnet(),
            relay: ("relays-new.cardano-mainnet.iohk.io.".to_string(), 3001),
            shelley_era_config: Era::SHELLEY_MAINNET,
        }
    }

    pub fn testnet() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::testnet(),
            relay: ("relays-new.cardano-testnet.iohkdev.io.".to_string(), 3001),
            shelley_era_config: Era::SHELLEY_TESTNET,
        }
    }

    pub fn preprod() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::preprod(),
            relay: ("preprod-node.world.dev.cardano.org.".to_string(), 30000),
            shelley_era_config: Era::SHELLEY_PREPROD,
        }
    }

    pub fn preview() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::preview(),
            relay: ("preview-node.world.dev.cardano.org.".to_string(), 30002),
            shelley_era_config: Era::SHELLEY_PREVIEW,
        }
    }

    pub fn sancho() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::new(
                1,
                cml_core::network::ProtocolMagic::from(4),
            ),
            relay: ("sanchonet-node.world.dev.cardano.org.".to_string(), 30004),
            shelley_era_config: Era::SHELLEY_SANCHO,
        }
    }
}
