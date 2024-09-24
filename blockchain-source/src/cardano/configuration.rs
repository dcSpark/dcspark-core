use super::{time::Era, Point};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
pub enum Relay {
    UrlPort(String, u16),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct NetworkConfiguration {
    pub chain_info: cml_chain::genesis::network_info::NetworkInfo,
    pub relay: Relay,
    pub shelley_era_config: Option<Era>,
    pub from: Option<Point>,
}

impl NetworkConfiguration {
    pub fn mainnet() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::mainnet(),
            relay: Relay::UrlPort("relays-new.cardano-mainnet.iohk.io.".to_string(), 3001),
            shelley_era_config: Some(Era::SHELLEY_MAINNET),
            from: None,
        }
    }

    pub fn testnet() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::testnet(),
            relay: Relay::UrlPort("relays-new.cardano-testnet.iohkdev.io.".to_string(), 3001),
            shelley_era_config: Some(Era::SHELLEY_TESTNET),
            from: None,
        }
    }

    pub fn preprod() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::preprod(),
            relay: Relay::UrlPort("preprod-node.world.dev.cardano.org.".to_string(), 30000),
            shelley_era_config: Some(Era::SHELLEY_PREPROD),
            from: None,
        }
    }

    pub fn preview() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::preview(),
            relay: Relay::UrlPort("preview-node.world.dev.cardano.org.".to_string(), 30002),
            shelley_era_config: Some(Era::SHELLEY_PREVIEW),
            from: None,
        }
    }

    pub fn sancho() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::new(
                1,
                cml_core::network::ProtocolMagic::from(4),
            ),
            relay: Relay::UrlPort("sanchonet-node.world.dev.cardano.org.".to_string(), 30004),
            shelley_era_config: Some(Era::SHELLEY_SANCHO),
            from: None,
        }
    }
}
