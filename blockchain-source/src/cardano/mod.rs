mod configuration;
mod event;
mod point;
pub mod time;

use std::time::Instant;

pub use self::event::{BlockEvent, CardanoNetworkEvent};
use crate::Source;
use anyhow::{Context as _, Result};
use cardano_net::{NetworkDescription, NetworkHandle};
pub use cardano_sdk::protocol::Tip;
use cardano_sdk::protocol::Version;
pub use configuration::NetworkConfiguration;
use dcspark_core::critical_error;
pub use point::*;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use tracing::{debug, error, info, warn, Instrument};

const TX_PROCESSING_CHANNEL_BOUND: usize = 1000;

type Event = CardanoNetworkEvent<BlockEvent, Tip>;

pub struct CardanoSource {
    service: mpsc::Sender<(Vec<Point>, mpsc::Sender<Result<Event>>)>,
    current: Option<mpsc::Receiver<Result<Event>>>,
    exit_rx: oneshot::Receiver<()>,
    // If the provided Checkpoints is empty, then this is the starting point.
    //
    // This can happen in the first pull, since the Multiverse doesn't have a block to provide, so
    // we take it from the network settings.
    default_from: Point,
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
            vec![self.default_from.clone()]
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
        let (url, port) = &network_config.relay;

        let config = NetworkDescription {
            anchor_hosts: vec![(url.to_string(), *port)],
            chain_info: cardano_sdk::chaininfo::ChainInfo {
                protocol_magic: cardano_sdk::protocol::Magic(u32::from(
                    network_config.chain_info.protocol_magic(),
                ) as u64),
                network_id: network_config.chain_info.network_id(),
                bech32_hrp_address: if network_config.chain_info
                    == cml_chain::genesis::network_info::NetworkInfo::mainnet()
                {
                    "addr"
                } else {
                    "addr_test"
                },
            },
            net_versions: vec![Version::V6, Version::V7, Version::V8],
            known_points: vec![],
        };

        let handle = NetworkHandle::start(&config)
            .await
            .context("Failed to establish connection with the node")?;

        let (tx, rx) = mpsc::channel(1);
        let (exit_tx, exit_rx) = oneshot::channel();

        // we don't need the handle, since we can signalkill the task by just dropping the request
        // channel, and the task can't error.
        tokio::task::spawn(
            request_handler(
                handle,
                rx,
                exit_tx,
                tip_update_pace,
                network_config.clone(),
                config,
            )
            .instrument(tracing::info_span!("request handler")),
        );

        Ok(Self {
            service: tx,
            current: None,
            exit_rx,
            default_from: network_config.from.clone(),
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
    handle: NetworkHandle,
    mut requests: mpsc::Receiver<(Vec<Point>, mpsc::Sender<Result<Event>>)>,
    exit_signal: oneshot::Sender<()>,
    tip_update_pace: Duration,
    network_config: NetworkConfiguration,
    config: NetworkDescription,
) {
    // initially set this to a time in the past, which guarantees an event in the tip fetch.
    let mut last_tip_event = Instant::now()
        .checked_sub(tip_update_pace)
        .expect("overflow when substracting from Instant::now");

    let mut handle = Some(handle);

    while let Some((from, channel)) = requests.recv().await {
        if handle.is_none() {
            info!("trying to reestablish connection with the node");

            match NetworkHandle::start(&config).await {
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

        let (from, ignore_first_block) = if from
            == vec![Point::BlockHeader {
                slot_nb: 0.into(),
                hash: network_config.genesis_parent.clone(),
            }] {
            (vec![network_config.genesis.clone()], false)
        } else {
            (from, true)
        };

        if let Err(e) = block_fetch(
            &mut current_handle,
            from,
            &channel,
            &mut last_tip_event,
            tip_update_pace,
            &network_config,
            ignore_first_block,
        )
        .await
        {
            warn!(error = %e, "dropping connection handle");
            current_handle.stop().await;
        } else {
            handle = Some(current_handle);
        }
    }

    let _ = exit_signal.send(());
}

#[tracing::instrument(skip(handle, channel))]
async fn block_fetch(
    handle: &mut NetworkHandle,
    from: Vec<Point>,
    channel: &mpsc::Sender<Result<Event, anyhow::Error>>,
    last_tip_event: &mut Instant,
    tip_update_pace: Duration,
    network_config: &NetworkConfiguration,
    ignore_first_block: bool,
) -> Result<()> {
    let points: Result<Vec<_>> = from
        .into_iter()
        .map(cardano_sdk::protocol::Point::try_from)
        .collect();

    if points.is_err() {
        error!("invalid point found, this shouldn't happen");
    }

    let mut points = points?;

    points.sort_by_key(|b: &cardano_sdk::protocol::Point| std::cmp::Reverse(b.slot_nb()));

    debug!("sending intersection request");

    let (from, tip) = match handle.chainsync.intersect(points).await? {
        cardano_net::ChainIntersection::Found(from, tip) => {
            info!(%from, %tip, "intersection found");
            (from, tip)
        }
        cardano_net::ChainIntersection::NotFound(tip) => {
            // this would cause `pull` to return None, which the 'puller' could potentially use as
            // a signal to change update the from argument the next time.
            warn!(%tip, "couldn't find a starting point in the node's current branch");
            return Ok(());
        }
    };

    if tip.point == from {
        info!("source is up to date, nothing to pull");
        return Ok(());
    }

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

    info!(%from, %tip, "making block range request");

    let mut block_fetcher = match handle.blockfetch.request_range(from, tip.point).await? {
        Some(block_fetcher) => block_fetcher,
        None => {
            debug!("no blocks found in range");
            return Ok(());
        }
    };

    if ignore_first_block {
        // the from in request_range is inclusive, but the from in `pull` is not supposed to be
        // included, so skip the first block (which will be one of the checkpoints)
        let _ = block_fetcher.next().await?;
    }

    while let Some(raw_block) = block_fetcher.next().await? {
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
