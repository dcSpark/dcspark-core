mod configuration;
mod event;
mod point;
pub mod time;

pub use self::event::{BlockEvent, CardanoNetworkEvent};
use crate::Source;
use anyhow::{Context as _, Result};
use cml_chain::block::Header;
use cml_chain::Deserialize;
use cml_multi_era::byron::block::{ByronBlockHeader, EbbHead};
use cml_multi_era::shelley::ShelleyHeader;
pub use configuration::NetworkConfiguration;
use cryptoxide::hashing::blake2b_256;
use dcspark_core::critical_error;
use pallas_network::facades::PeerClient;
use pallas_network::miniprotocols::chainsync;
use pallas_network::miniprotocols::chainsync::Tip;
pub use point::*;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use tracing::{debug, error, info, warn, Instrument};

const TX_PROCESSING_CHANNEL_BOUND: usize = 1000;

type Event = CardanoNetworkEvent<BlockEvent, Tip>;

pub struct CardanoSource {
    service: mpsc::Sender<(Vec<Point>, mpsc::Sender<Result<Event>>)>,
    current: Option<mpsc::Receiver<Result<Event>>>,
    exit_rx: oneshot::Receiver<()>,
}

#[async_trait::async_trait]
impl Source for CardanoSource {
    type Event = Event;

    type From = Vec<Point>;

    /// This will either return a transaction from the buffer, or start a new network request to
    /// sync with the node's current tip.
    ///
    /// Since this could potentially end up fetching and buffering the entire chain, this function
    /// will return as soon as a block is available, and blocks will be pulled in the background.
    ///
    /// This function will return None in two cases:
    ///
    /// * None of the points provided in from are in the current branch.
    /// * One of the points provided is the current tip.
    ///
    #[tracing::instrument(skip(self))]
    async fn pull(&mut self, from: &Self::From) -> Result<Option<Self::Event>> {
        // If there is a request in flight, then we try to get one of those blocks.
        //
        // In this case, the `from` argument is basically ignored, we just serve from the buffer.
        // If there is nothing there we just block on it.
        if let Some(channel) = &mut self.current {
            let next = channel.recv().await;
            if next.is_some() {
                return next.transpose();
            }
        }

        // Here we either:
        //
        //      * Haven't started a request yet (self.current was None)
        //      * The previous one just ended (`next` was None)
        //
        // Then, we enqueue a new fetch from `from` to the current tip, and block on that.

        let (tx, rx) = mpsc::channel(TX_PROCESSING_CHANNEL_BOUND);

        let from = if from.is_empty() {
            vec![Point::Origin]
        } else {
            from.clone()
        };

        if self.service.send((from, tx)).await.is_err() {
            error!("block processing service stopped");
            return Err(anyhow::anyhow!("request handler stoped"));
        }

        self.current.replace(rx);

        // this unwrap is safe, since we just called `replace`
        self.current.as_mut().unwrap().recv().await.transpose()
    }
}

impl CardanoSource {
    pub async fn connect(
        network_config: &NetworkConfiguration,
        tip_update_pace: Duration,
    ) -> Result<Self> {
        let handle = PeerClient::connect(
            &network_config.relay,
            u32::from(network_config.chain_info.protocol_magic()).into(),
        )
        .await
        .context("Failed to establish connection with the node")?;

        let (tx, rx) = mpsc::channel(1);
        let (exit_tx, exit_rx) = oneshot::channel();

        // we don't need the handle, since we can signalkill the task by just dropping the request
        // channel, and the task can't error.
        tokio::task::spawn(
            request_handler(handle, rx, exit_tx, tip_update_pace, network_config.clone())
                .instrument(tracing::info_span!("request handler")),
        );

        Ok(Self {
            service: tx,
            current: None,
            exit_rx,
        })
    }

    /// This will cause the task's request loop to eventually exit, but if there is a request in
    /// process then this will wait for that to finish.
    pub async fn stop(self) {
        std::mem::drop(self.service);
        std::mem::drop(self.current);

        let _ = self.exit_rx.await;
    }

    /// This will clear all the currently buffered transactions. Since there is no cancellations in
    /// the underlying protocol, blocks for any ongoing request will still need to be received, but
    /// those will be inmediately discarded. This means that new requests will block until the
    /// current BlockFetcher is fully consumed.
    pub fn clear_buffers(&mut self) {
        self.current = None
    }
}

async fn request_handler(
    handle: PeerClient,
    mut requests: mpsc::Receiver<(Vec<Point>, mpsc::Sender<Result<Event>>)>,
    exit_signal: oneshot::Sender<()>,
    tip_update_pace: Duration,
    network_config: NetworkConfiguration,
) {
    // initially set this to a time in the past, which guarantees an event in the tip fetch.
    let mut last_tip_event = Instant::now()
        .checked_sub(tip_update_pace)
        .expect("overflow when substracting from Instant::now");

    let mut handle = Some(handle);

    while let Some((from, channel)) = requests.recv().await {
        if handle.is_none() {
            info!("trying to reestablish connection with the node");

            match PeerClient::connect(
                &network_config.relay,
                u32::from(network_config.chain_info.protocol_magic()).into(),
            )
            .await
            {
                Ok(new_handle) => {
                    info!("connection reestablished succesfully");
                    handle.replace(new_handle);
                }
                Err(error) => {
                    error!(%error, "failed to reestablish connection with the node");

                    // this will make the `pull` return None.
                    //
                    // so waiting between retries will depend on the polling frequency
                    continue;
                }
            }
        }

        let mut current_handle = handle.take().unwrap();

        if let Err(e) = block_fetch(
            &mut current_handle,
            from,
            &channel,
            &mut last_tip_event,
            tip_update_pace,
            &network_config,
        )
        .await
        {
            warn!(error = %e, "dropping connection handle");
            current_handle.abort().await;
        } else {
            handle = Some(current_handle);
        }
    }

    let _ = exit_signal.send(());
}

#[tracing::instrument(skip(handle, channel))]
async fn block_fetch(
    handle: &mut PeerClient,
    from: Vec<Point>,
    channel: &mpsc::Sender<Result<Event, anyhow::Error>>,
    last_tip_event: &mut Instant,
    tip_update_pace: Duration,
    network_config: &NetworkConfiguration,
) -> Result<()> {
    let points: Result<Vec<_>> = from
        .into_iter()
        .map(pallas_network::miniprotocols::Point::try_from)
        .collect();

    if points.is_err() {
        error!("invalid point found, this shouldn't happen");
    }

    let mut points = points?;

    points.sort_by_key(|b: &pallas_network::miniprotocols::Point| {
        std::cmp::Reverse(b.slot_or_default())
    });

    debug!("sending intersection request");

    let (from, tip) = handle.chainsync.find_intersect(points).await?;

    let Some(from) = from else {
        // this would cause `pull` to return None, which the 'puller' could potentially use as
        // a signal to change update the from argument the next time.
        warn!(
            "couldn't find a starting point in the node's current branch: {:#?}",
            tip.0,
        );

        return Ok(());
    };

    if last_tip_event.elapsed() >= tip_update_pace {
        if channel
            .send(Ok(CardanoNetworkEvent::Tip(tip.clone())))
            .await
            .is_err()
        {
            debug!("can't send tip event, request response channel was closed");
        };

        *last_tip_event = Instant::now();
    }

    let raw_header = match handle.chainsync.request_next().await? {
        chainsync::NextResponse::RollForward(block_header, _) => block_header,
        chainsync::NextResponse::RollBackward(point, _) => {
            if point == pallas_network::miniprotocols::Point::Origin || point == from {
                match handle.chainsync.request_next().await? {
                    chainsync::NextResponse::RollForward(block_header, _) => block_header,
                    chainsync::NextResponse::RollBackward(_, _) => {
                        // there shouldn't be a way to get this again after already rolling back to Origin or to the intersect point
                        return Err(
                            anyhow::anyhow!("Unexpected RollBackward").context(critical_error!())
                        );
                    }
                    chainsync::NextResponse::Await => {
                        info!("source is up to date, nothing to pull");
                        return Ok(());
                    }
                }
            } else {
                todo!();
            }
        }
        chainsync::NextResponse::Await => {
            info!("source is up to date, nothing to pull");
            return Ok(());
        }
    };

    let (slot, hash) = match raw_header.variant {
        0 => {
            let mut tagged_bytes = vec![0x82];

            match raw_header.byron_prefix {
                Some((0, _)) => {
                    let header = EbbHead::from_cbor_bytes(&raw_header.cbor)
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to deserialize byron epoch boundary header: {e}"
                            )
                        })
                        .context(critical_error!())?;

                    // TODO: kinda copied from cml, but we don't have the block here, only the header
                    tagged_bytes.push(0x00);
                    tagged_bytes.extend(&raw_header.cbor);

                    (
                        header.consensus_data.byron_difficulty.u64,
                        blake2b_256(&tagged_bytes),
                    )
                }
                _ => {
                    let header = ByronBlockHeader::from_cbor_bytes(&raw_header.cbor)
                        .map_err(|e| anyhow::anyhow!("Failed to deserialize byron header: {e}"))
                        .context(critical_error!())?;

                    tagged_bytes.push(0x00);
                    tagged_bytes.extend(&raw_header.cbor);

                    (
                        header.consensus_data.byron_difficulty.u64,
                        blake2b_256(&tagged_bytes),
                    )
                }
            }
        }
        1..=4 => {
            let header = ShelleyHeader::from_cbor_bytes(&raw_header.cbor)
                .map_err(|e| anyhow::anyhow!("Failed to deserialize shelley header: {e}"))
                .context(critical_error!())?;

            (header.body.slot, blake2b_256(&raw_header.cbor))
        }
        _ => {
            let header = Header::from_cbor_bytes(&raw_header.cbor)
                .map_err(|e| anyhow::anyhow!("Failed to deserialize header: {e}"))
                .context(critical_error!())?;

            (header.header_body.slot, blake2b_256(&raw_header.cbor))
        }
    };

    // The source works by always providing the last known block as the from
    // argument, however the block fetch protocol is inclusive in both ends of
    // the range. Normally after sending the intersection message, the server
    // would send the header of the next header through the chainsync protocol
    // (see the code above this). However for some reason this doesn't seem to
    // work correctly in the byron era, so in that case we just ignore the first
    // block.
    let (skip_one_block, from) = if from.slot_or_default() > slot {
        (true, from)
    } else {
        (
            false,
            pallas_network::miniprotocols::Point::Specific(slot, hash.to_vec()),
        )
    };

    info!(?from, ?tip, "making block range request");

    if handle
        .blockfetch
        .request_range((from, tip.0))
        .await?
        .is_none()
    {
        debug!("no blocks found in range");
        return Ok(());
    };

    if skip_one_block {
        handle.blockfetch.recv_while_streaming().await?;
    }

    while let Some(raw_block) = handle.blockfetch.recv_while_streaming().await? {
        let event = BlockEvent::from_serialized_block(
            raw_block.as_ref(),
            &network_config.shelley_era_config,
        )
        .context(critical_error!());

        if channel
            .send(event.map(CardanoNetworkEvent::Block))
            .await
            .is_err()
        {
            return Err(anyhow::anyhow!("request response channel was closed"));
        }
    }

    debug!("block range request finished successfully");

    Ok(())
}
