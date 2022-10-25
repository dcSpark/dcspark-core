use super::Point;
use cardano_sdk::protocol::Magic;
use dcspark_core::{BlockId, SlotNumber};
use std::borrow::Cow;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct NetworkConfiguration {
    pub chain_info: ChainInfo,
    pub relay: (Cow<'static, str>, u16),
    pub from: Point,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub enum ChainInfo {
    Mainnet,
    Preprod,
    Preview,
    Custom { protocol_magic: u64, network_id: u8 },
}

impl From<ChainInfo> for cardano_sdk::chaininfo::ChainInfo {
    fn from(info: ChainInfo) -> Self {
        match info {
            ChainInfo::Mainnet => cardano_sdk::chaininfo::ChainInfo::MAINNET,
            ChainInfo::Preprod => cardano_sdk::chaininfo::ChainInfo::PREPROD,
            ChainInfo::Preview => cardano_sdk::chaininfo::ChainInfo {
                protocol_magic: Magic(2),
                network_id: 0b0000,
                bech32_hrp_address: "addr_test",
            },
            ChainInfo::Custom {
                protocol_magic,
                network_id,
            } => cardano_sdk::chaininfo::ChainInfo {
                protocol_magic: Magic(protocol_magic),
                network_id,
                bech32_hrp_address: "addr_test",
            },
        }
    }
}

impl NetworkConfiguration {
    pub fn mainnet() -> Self {
        Self {
            chain_info: ChainInfo::Mainnet,
            relay: (Cow::Borrowed("relays-new.cardano-mainnet.iohk.io."), 3001),
            from: Point::BlockHeader {
                slot_nb: SlotNumber::new(4492800),
                hash: BlockId::new(
                    "aa83acbf5904c0edfe4d79b3689d3d00fcfc553cf360fd2229b98d464c28e9de",
                ),
            },
        }
    }

    pub fn preprod() -> Self {
        Self {
            chain_info: ChainInfo::Preprod,
            relay: (Cow::Borrowed("preprod-node.world.dev.cardano.org."), 30000),
            from: Point::BlockHeader {
                slot_nb: SlotNumber::new(86400),
                hash: BlockId::new(
                    "c4a1595c5cc7a31eda9e544986fe9387af4e3491afe0ca9a80714f01951bbd5c",
                ),
            },
        }
    }

    pub fn preview() -> Self {
        Self {
            chain_info: ChainInfo::Preview,
            relay: (Cow::Borrowed("preview-node.world.dev.cardano.org."), 30002),
            from: Point::BlockHeader {
                slot_nb: SlotNumber::new(25400),
                hash: BlockId::new(
                    "8542d7f0b744f40f3de6164294b5feb0095307d46c7290acc8a5d9bd802acb8e",
                ),
            },
        }
    }
}
