use super::{time::Era, Point};
use dcspark_core::{BlockId, SlotNumber};
use std::borrow::Cow;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct NetworkConfiguration {
    pub chain_info: cml_chain::genesis::network_info::NetworkInfo,
    pub relay: (Cow<'static, str>, u16),
    pub from: Point,
    pub genesis_parent: BlockId,
    pub genesis: Point,
    pub shelley_era_config: Era,
}

impl NetworkConfiguration {
    pub fn mainnet() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::mainnet(),
            relay: (Cow::Borrowed("relays-new.cardano-mainnet.iohk.io."), 3001),
            from: Point::BlockHeader {
                slot_nb: SlotNumber::new(4492800),
                hash: BlockId::new(
                    "aa83acbf5904c0edfe4d79b3689d3d00fcfc553cf360fd2229b98d464c28e9de",
                ),
            },
            genesis_parent: BlockId::new(
                "5f20df933584822601f9e3f8c024eb5eb252fe8cefb24d1317dc3d432e940ebb",
            ),
            genesis: Point::BlockHeader {
                hash: BlockId::new(
                    "89d9b5a5b8ddc8d7e5a6795e9774d97faf1efea59b2caf7eaf9f8c5b32059df4",
                ),
                slot_nb: SlotNumber::new(0),
            },
            shelley_era_config: Era::SHELLEY_MAINNET,
        }
    }

    pub fn testnet() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::testnet(),
            relay: (
                Cow::Borrowed("relays-new.cardano-testnet.iohkdev.io."),
                3001,
            ),
            from: Point::BlockHeader {
                slot_nb: SlotNumber::new(1598400),
                hash: BlockId::new(
                    "02b1c561715da9e540411123a6135ee319b02f60b9a11a603d3305556c04329f",
                ),
            },
            genesis_parent: BlockId::new(
                "96fceff972c2c06bd3bb5243c39215333be6d56aaf4823073dca31afe5038471",
            ),
            genesis: Point::BlockHeader {
                hash: BlockId::new(
                    "8f8602837f7c6f8b8867dd1cbc1842cf51a27eaed2c70ef48325d00f8efb320f",
                ),
                slot_nb: SlotNumber::new(0),
            },
            shelley_era_config: Era::SHELLEY_TESTNET,
        }
    }

    pub fn preprod() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::preprod(),
            relay: (Cow::Borrowed("preprod-node.world.dev.cardano.org."), 30000),
            from: Point::BlockHeader {
                slot_nb: SlotNumber::new(86400),
                hash: BlockId::new(
                    "c4a1595c5cc7a31eda9e544986fe9387af4e3491afe0ca9a80714f01951bbd5c",
                ),
            },
            genesis_parent: BlockId::new(
                "d4b8de7a11d929a323373cbab6c1a9bdc931beffff11db111cf9d57356ee1937",
            ),
            genesis: Point::BlockHeader {
                hash: BlockId::new(
                    "9ad7ff320c9cf74e0f5ee78d22a85ce42bb0a487d0506bf60cfb5a91ea4497d2",
                ),
                slot_nb: SlotNumber::new(0),
            },
            shelley_era_config: Era::SHELLEY_PREPROD,
        }
    }

    pub fn preview() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::preview(),
            relay: (Cow::Borrowed("preview-node.world.dev.cardano.org."), 30002),
            from: Point::BlockHeader {
                slot_nb: SlotNumber::new(25400),
                hash: BlockId::new(
                    "8542d7f0b744f40f3de6164294b5feb0095307d46c7290acc8a5d9bd802acb8e",
                ),
            },
            genesis_parent: BlockId::new(
                "72593f260b66f26bef4fc50b38a8f24d3d3633ad2e854eaf73039eb9402706f1",
            ),
            genesis: Point::BlockHeader {
                hash: BlockId::new(
                    "268ae601af8f9214804735910a3301881fbe0eec9936db7d1fb9fc39e93d1e37",
                ),
                slot_nb: SlotNumber::new(0),
            },
            shelley_era_config: Era::SHELLEY_PREVIEW,
        }
    }

    pub fn sancho() -> Self {
        Self {
            chain_info: cml_chain::genesis::network_info::NetworkInfo::new(
                1,
                cml_core::network::ProtocolMagic::from(4),
            ),
            relay: (
                Cow::Borrowed("sanchonet-node.world.dev.cardano.org."),
                30004,
            ),
            from: Point::BlockHeader {
                slot_nb: SlotNumber::new(20),
                hash: BlockId::new(
                    "6a7d97aae2a65ca790fd14802808b7fce00a3362bd7b21c4ed4ccb4296783b98",
                ),
            },
            genesis_parent: BlockId::new(
                "785eb88427e136378a15b0a152a8bfbeec7a611529ccda29c43a1e60ffb48eaa",
            ),
            genesis: Point::BlockHeader {
                hash: BlockId::new(
                    "6a7d97aae2a65ca790fd14802808b7fce00a3362bd7b21c4ed4ccb4296783b98",
                ),
                slot_nb: SlotNumber::new(20),
            },
            shelley_era_config: Era::SHELLEY_SANCHO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NetworkConfiguration;

    #[test]
    fn custom_json_network() {
        println!(
            "{}",
            deps::serde_json::to_string_pretty(&NetworkConfiguration::mainnet()).unwrap()
        );
        let json = r#"{
            "chain_info": {
                "network_id": 1,
                "protocol_magic": 764824073
            },
            "relay": [
                "relays-new.cardano-mainnet.iohk.io.",
                3001
            ],
            "from": {
                "BlockHeader": {
                "slot_nb": 4492800,
                "hash": "aa83acbf5904c0edfe4d79b3689d3d00fcfc553cf360fd2229b98d464c28e9de"
                }
            },
            "genesis_parent": "5f20df933584822601f9e3f8c024eb5eb252fe8cefb24d1317dc3d432e940ebb",
            "genesis": {
                "BlockHeader": {
                "slot_nb": 0,
                "hash": "89d9b5a5b8ddc8d7e5a6795e9774d97faf1efea59b2caf7eaf9f8c5b32059df4"
                }
            },
            "shelley_era_config": {
                "first_slot": 4492800,
                "start_epoch": 208,
                "known_time": 1596059091,
                "slot_length": 1,
                "epoch_length_seconds": 432000
            }
            }"#;
        let _config: NetworkConfiguration = deps::serde_json::from_str(json).unwrap();
    }
}
