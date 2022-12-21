use std::path::PathBuf;

use anyhow::{anyhow, Context};
use cardano_utils::multisig_plan::MultisigPlan;
use cardano_utils::network_id::NetworkInfo;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp::{max, min};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use utxo_selection::algorithms::{ThermostatAlgoConfig, ThermostatFeeEstimator};

use utxo_selection::benchmark::cardano_benchmark::run_algorithm_benchmark;

#[derive(Parser, Debug)]
#[clap(version)]
pub struct Cli {
    /// path to config file
    #[clap(long, value_parser)]
    input_path: PathBuf,

    /// path to output file
    #[clap(long, value_parser)]
    output_path: PathBuf,
}

#[tokio::main]
async fn main() {
    let result = _main().await;
    result.unwrap();
}

async fn _main() -> anyhow::Result<()> {
    let Cli {
        input_path,
        output_path,
    } = Cli::parse();

    let io_selection_algo =
        utxo_selection::algorithms::Thermostat::new(ThermostatAlgoConfig::default());
    let change_balance_algo =
        utxo_selection::algorithms::Thermostat::new(ThermostatAlgoConfig::default());
    run_algorithm_benchmark(
        io_selection_algo,
        change_balance_algo,
        || {
            Ok(ThermostatFeeEstimator::new(
                NetworkInfo::Mainnet,
                &MultisigPlan {
                    quorum: 0,
                    keys: vec![],
                },
            ))
        },
        input_path,
        output_path,
    )
}
