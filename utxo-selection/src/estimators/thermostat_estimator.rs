use crate::TransactionFeeEstimator;
use dcspark_core::multisig_plan::MultisigPlan;
use dcspark_core::network_id::NetworkInfo;
use dcspark_core::tx::{UTxOBuilder, UTxODetails};
use dcspark_core::{Balance, Regulated, TokenId, Value};
use std::collections::HashMap;

pub struct ThermostatFeeEstimator {
    network_info: NetworkInfo,

    cost_empty: Value<Regulated>,
    cost_input: Value<Regulated>,
    cost_output: Value<Regulated>,
    cost_metadata: Value<Regulated>,

    current_size: usize,
    max_size: usize,
    size_of_one_input: usize,
    size_of_one_output: usize,

    outputs: Vec<UTxOBuilder>,
    inputs: Vec<UTxODetails>,

    asset_balance: HashMap<TokenId, Balance<Regulated>>,
}

impl ThermostatFeeEstimator {
    #[allow(unused)]
    pub fn new(network_info: NetworkInfo, plan: &MultisigPlan) -> Self {
        // compute the cost of an empty transaction this is with the
        // the native script included so we know what it will cost
        // already from there.
        // it also contains the `quorum` number of witnesses.
        let mut cost_empty = {
            let v = network_info.assumed_empty_transaction();
            v.to_str().parse().unwrap()
        };
        let cost_script: Value<Regulated> = {
            let v = network_info.assumed_cost_native_script(plan);
            v.to_str().parse().unwrap()
        };
        let cost_witness: Value<Regulated> = {
            let v = network_info.assumed_cost_one_witness();
            v.to_str().parse().unwrap()
        };
        cost_empty += cost_script + (&cost_witness * plan.quorum);

        let cost_input = {
            let v = network_info.assumed_cost_one_input();
            v.to_str().parse().unwrap()
        };
        let cost_output = {
            let v = network_info.assumed_cost_one_output();
            v.to_str().parse().unwrap()
        };

        let current_size =
            network_info.estimated_size_empty() + network_info.estimate_size_overhead(plan);
        let max_size = network_info.max_tx_size();
        let size_of_one_input = network_info.estimated_size_input();
        let size_of_one_output = network_info.estimated_size_output();
        Self {
            network_info,

            cost_empty,
            cost_input,
            cost_output,
            cost_metadata: Value::zero(),

            current_size,
            max_size,
            size_of_one_input,
            size_of_one_output,

            outputs: Vec::new(),
            inputs: Vec::new(),
            asset_balance: HashMap::new(),
        }
    }

    #[allow(unused)]
    pub fn add_protocol_magic(&mut self, protocol_magic: impl AsRef<str>) {
        self.current_size = protocol_magic.as_ref().len() + 5;
        self.cost_metadata = {
            let v = self
                .network_info
                .assumed_cost_metadata_protocol_magic(protocol_magic);

            v.to_str().parse().unwrap()
        };
    }
}

impl TransactionFeeEstimator for ThermostatFeeEstimator {
    type InputUtxo = UTxODetails;
    type OutputUtxo = UTxOBuilder;

    fn min_required_fee(&self) -> anyhow::Result<Value<Regulated>> {
        let num_outputs = self.outputs.len();
        let num_inputs = self.inputs.len();
        Ok(&self.cost_empty
            + &self.cost_metadata
            + (&self.cost_output * num_outputs)
            + (&self.cost_input * num_inputs))
    }

    fn fee_for_input(&self, _input: &Self::InputUtxo) -> anyhow::Result<Value<Regulated>> {
        Ok(self.cost_input.clone())
    }

    fn add_input(&mut self, input: Self::InputUtxo) -> anyhow::Result<()> {
        for asset in input.assets.iter() {
            let balance = self
                .asset_balance
                .entry(asset.fingerprint.clone())
                .or_default();

            *balance += &asset.quantity;
        }

        self.current_size += self.size_of_one_input;
        self.inputs.push(input);
        Ok(())
    }

    fn fee_for_output(&self, _output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>> {
        Ok(self.cost_output.clone())
    }

    fn add_output(&mut self, output: Self::OutputUtxo) -> anyhow::Result<()> {
        for asset in output.assets.iter() {
            let balance = self
                .asset_balance
                .entry(asset.fingerprint.clone())
                .or_default();
            *balance -= &asset.quantity;
        }
        self.current_size += self.size_of_one_output;
        self.outputs.push(output);
        Ok(())
    }

    fn current_size(&self) -> anyhow::Result<usize> {
        Ok(self.current_size)
    }

    fn max_size(&self) -> anyhow::Result<usize> {
        Ok(self.max_size)
    }
}
