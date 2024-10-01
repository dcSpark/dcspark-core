use super::{NetworkConfiguration, Point};
use crate::{
    cardano::{configuration, BlockEvent},
    EventObject, GetNextFrom, Source,
};
use anyhow::{anyhow, Result};
use dcspark_core::StoppableService;
use pallas_network::{facades::NodeClient, miniprotocols::chainsync};
use tracing::{debug, info};

#[derive(Debug)]
pub enum Event {
    RollBack { block_slot: u64, block_hash: String },
    Block(BlockEvent),
}

impl GetNextFrom for Event {
    type From = ();

    fn next_from(&self) -> Option<Self::From> {
        None
    }
}

impl EventObject for Event {
    fn is_blockchain_tip(&self) -> bool {
        false
    }
}

pub struct N2CSource {
    rx: tokio::sync::mpsc::Receiver<Result<Event>>,
}

impl N2CSource {
    pub async fn connect(
        config: NetworkConfiguration,
        start_from: Vec<Point>,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(1024);

        let socket = match &config.relay {
            configuration::Relay::UnixSocket(socket) => socket,
            configuration::Relay::UrlPort(_, _) => anyhow::bail!(
                "N2C protocol doesn't support tcp connection, use the N2N source instead"
            ),
        };

        let mut client =
            NodeClient::connect(socket, u32::from(config.chain_info.protocol_magic()).into())
                .await
                .unwrap();

        let points: anyhow::Result<Vec<_>> = start_from
            .into_iter()
            .map(pallas_network::miniprotocols::Point::try_from)
            .collect();

        let mut points = points?;

        if points.is_empty() {
            points.push(pallas_network::miniprotocols::Point::Origin);
        }

        if points.len() == 1 && points[0].slot_or_default() == 0 {
            points[0] = pallas_network::miniprotocols::Point::Origin;
        }

        let (point, _) = client.chainsync().find_intersect(points).await.unwrap();

        info!("intersected point is {:?}", point);

        let _ = client.chainsync().request_or_await_next().await.unwrap();

        tokio::spawn(async move {
            loop {
                let next = client.chainsync().request_or_await_next().await.unwrap();

                match next {
                    chainsync::NextResponse::RollForward(raw_block, _) => {
                        let block_event = BlockEvent::from_serialized_block(
                            &raw_block,
                            config.shelley_era_config.as_ref(),
                        )
                        .map(Event::Block);

                        tx.send(block_event).await.unwrap();
                    }
                    chainsync::NextResponse::RollBackward(point, _) => match point {
                        pallas_network::miniprotocols::Point::Origin => {}
                        pallas_network::miniprotocols::Point::Specific(slot, hash) => {
                            tx.send(Ok(Event::RollBack {
                                block_slot: slot,
                                block_hash: hex::encode(hash),
                            }))
                            .await
                            .unwrap();
                        }
                    },
                    chainsync::NextResponse::Await => debug!("tip of chain reached"),
                };
            }
        });

        Ok(Self { rx })
    }
}

#[async_trait::async_trait]
impl Source for N2CSource {
    type Event = Event;
    type From = ();

    async fn pull(&mut self, _from: &Self::From) -> anyhow::Result<Option<Self::Event>> {
        self.rx
            .recv()
            .await
            .ok_or(anyhow!("Background tokio task finished unexpectedly"))?
            .map(Some)
    }
}

#[async_trait::async_trait]
impl StoppableService for N2CSource {
    async fn stop(self) -> anyhow::Result<()> {
        Ok(())
    }
}
