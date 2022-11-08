use std::borrow::BorrowMut;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{Debug, format};
use anyhow::{anyhow, Context};
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::error::JsError;
use cardano_utils::multisig_plan::MultisigPlan;
use cardano_utils::network_id::NetworkInfo;
use dcspark_core::{Address, Balance, Regulated, TokenId, UTxOStore, Value};
use dcspark_core::tx::{TransactionAsset, UTxODetails, UtxoPointer};
use crate::{InputOutputSetup, InputSelectionAlgorithm, InputSelectionResult, TransactionFeeEstimator, UTxOBuilder};

pub struct ThermostatAlgoConfig {
    num_accumulators: usize,
    num_accumulators_assets: usize,
    native_utxo_thermostat_min: Value<Regulated>,
    native_utxo_thermostat_max: Value<Regulated>,
    main_token: TokenId,
}

pub struct Thermostat {
    outputs: Vec<UTxOBuilder>,

    optional_change_address: Option<Address>,
    changes: HashMap<TokenId, UTxOBuilder>,
    extra_changes: Vec<UTxOBuilder>,

    selected_inputs: Vec<UTxODetails>,
    selected_inputs_value: Value<Regulated>,

    balance: Balance<Regulated>,
    asset_balance: HashMap<TokenId, Balance<Regulated>>,
    config: ThermostatAlgoConfig,
    available_utxos: UTxOStore,
}

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

    balance: Balance<Regulated>,
    asset_balance: HashMap<TokenId, Balance<Regulated>>,
}

impl ThermostatFeeEstimator {
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
            balance: Balance::Balanced,
            asset_balance: HashMap::new(),
        }
    }

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

    fn fee_for_input(&self, input: &Self::InputUtxo) -> anyhow::Result<Value<Regulated>> {
        Ok(self.cost_input.clone())
    }

    fn add_input(&mut self, input: Self::InputUtxo) -> anyhow::Result<()> {
        self.balance += &input.value;

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

    fn remaining_number_inputs_allowed(&mut self) -> anyhow::Result<usize> {
        // compute if we need to reserve ROOM for a change address
        // for the main asset
        let reserved_room = self.size_of_one_output * self.asset_balance.len();
        // we add that we might have two output per change if we have to split an accumulator
        // in two in order to preserver distribution
        let reserved_room = reserved_room * 2;

        Ok(self.max_size
            .saturating_sub(self.current_size.saturating_add(reserved_room))
            / self.size_of_one_input)
    }

    fn fee_for_output(&self, output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>> {
        Ok(self.cost_output.clone())
    }

    fn add_output(&mut self, output: Self::OutputUtxo) -> anyhow::Result<()> {
        self.balance -= &output.value;
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
}

impl Thermostat {
    pub fn new(config: ThermostatAlgoConfig) -> Self {
        Self {
            outputs: Vec::new(),

            optional_change_address: None,
            changes: HashMap::new(),
            extra_changes: vec![],

            selected_inputs: Vec::new(),

            selected_inputs_value: Value::zero(),
            balance: Balance::Balanced,
            asset_balance: HashMap::new(),
            config,
            available_utxos: Default::default(),
        }
    }

    fn add_output(&mut self, output: UTxOBuilder) {
        self.balance -= &output.value;
        for asset in output.assets.iter() {
            let balance = self
                .asset_balance
                .entry(asset.fingerprint.clone())
                .or_default();
            *balance -= &asset.quantity;
        }
        self.outputs.push(output);
    }

    fn add_input(&mut self, input: UTxODetails) {
        self.selected_inputs_value += input.value.clone();
        self.balance += input.value.clone();

        for asset in input.assets.iter() {
            let balance = self
                .asset_balance
                .entry(asset.fingerprint.clone())
                .or_default();

            *balance += &asset.quantity;
        }

        self.selected_inputs.push(input);
    }

    fn current_balance<Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>>(&self, estimate: &mut Estimate) -> anyhow::Result<Balance<Regulated>> {
        Ok(&self.balance - &estimate.min_required_fee()?)
    }

    fn current_balance_of(&self, asset: &TokenId) -> Balance<Regulated> {
        self.asset_balance
            .get(asset)
            .cloned()
            .expect("We should have a balance for this since we are tracking it down")
    }

    fn select_input_for<Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>>(
        &mut self,
        utxos: UTxOStore,
        asset: &TokenId,
        estimate: &mut Estimate,
    ) -> anyhow::Result<UTxOStore> {
        let utxo = utxos
            // here we take the largest available UTxO for this given
            // asset.
            .iter_token_ordered_by_value_rev(asset)
            .next()
            .cloned()
            .ok_or_else(|| anyhow!("No more input to select for {asset}"))?;


        estimate.add_input(utxo.clone())?;
        self.add_input(utxo.clone());

        // remove the UTxO from our copy of the UTxO Store
        //
        // this will allow to make sure we don't reuse the
        // same input twice selecting inputs for the transaction
        let mut utxos = utxos.thaw();
        utxos.remove(&utxo.pointer)?;

        Ok(utxos.freeze())
    }

    /// function to select inputs until we have balanced the transaction
    /// for a given asset
    ///
    /// This will returned the updated UTxOStore: without the
    /// selected inputs if any
    ///
    /// If there are no inputs to select about this specific item
    /// then the function will fail.
    ///
    fn select_input_for_asset_until_balanced<Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>>(
        &mut self,
        mut utxos: UTxOStore,
        asset: &TokenId,
        estimate: &mut Estimate,
    ) -> anyhow::Result<UTxOStore> {

        while let Balance::Debt(debt) = self.current_balance_of(asset) {
            utxos = self
                .select_input_for(utxos, asset, estimate)
                .with_context(|| anyhow!("Could not get inputs to fund {debt} for {asset}"))?;
        }

        Ok(utxos)
    }

    /// function to select inputs until we have balanced the transaction
    /// for the main asset
    ///
    /// This will returned the updated UTxOStore: without the
    /// selected inputs if any
    ///
    /// If there are no inputs to select about this specific item
    /// then the function will fail.
    ///
    fn select_input_for_main_until_balanced<Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>>(
        &mut self,
        mut utxos: UTxOStore,
        estimate: &mut Estimate,
    ) -> anyhow::Result<UTxOStore> {
        let main_token = self.config.main_token.clone();
        while let Balance::Debt(debt) = self.current_balance(estimate)? {
            utxos = self
                .select_input_for(utxos, &main_token, estimate)
                .with_context(|| anyhow!("Could not get inputs to fund {debt} coin"))?;
        }

        Ok(utxos)
    }

    /// balance the excess of a given asset (if any)
    ///
    /// This function let us know if the native asset was added in the change
    fn balance_excess_of_asset<Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>>(&mut self, utxos: &UTxOStore, asset: TokenId,estimate: &mut Estimate) -> anyhow::Result<()> {
        if let Balance::Excess(excess) = self.current_balance_of(&asset) {
            let address = self
                .optional_change_address
                .as_ref()
                .ok_or_else(|| anyhow!("Change address required"))?;

            let default = UTxOBuilder::new(
                address.clone(),
                Value::zero(),
                vec![],
            )?;

            {
                // setting the entry in a scope so it does not prevent us from
                // manipulating `self` later
                let entry = self.changes.entry(asset.clone());
                let entry: &mut UTxOBuilder = match entry {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => {
                        estimate.add_output(default.clone()).map_err(|err| anyhow!(err))?;
                        entry.insert(default)
                    }
                };

                let balance = self.asset_balance.entry(asset.clone()).or_default();
                *balance -= &excess;

                if let Some(asset) = entry.assets.get_mut(0) {
                    asset.quantity += excess;
                } else {
                    let (policy_id, asset_name) = utxos
                        .get_asset_ids(&asset)
                        .cloned()
                        .expect("Always expect to retrieve the policy_id/asset_name");

                    entry.assets.push(TransactionAsset {
                        policy_id,
                        asset_name,
                        fingerprint: asset.clone(),
                        quantity: excess,
                    });
                }
            }

            let excess = if let Balance::Excess(wmain) = self.current_balance(estimate)? {
                wmain
            } else {
                Value::zero()
            };

            let entry = self
                .changes
                .get_mut(&asset)
                .expect("We cannot have a None here since we just added it before");

            entry.value += &excess;
            self.balance -= excess;

            // TODO: the entry.value should be set to the self.current_balance() excess
            // minus cost we might have needed to add the new output change
            //
            // we might want to free the `entry` from the reference
            // so we have something to work with with a current value
            // because right now we are setting all the excess without
            // balancing it properly

            if entry.value < self.config.native_utxo_thermostat_min {
                let difference = &self.config.native_utxo_thermostat_max - &entry.value;
                entry.value = self.config.native_utxo_thermostat_max.clone();
                self.balance -= difference;
            } else if entry.value > self.config.native_utxo_thermostat_max {
                let difference = &entry.value - &self.config.native_utxo_thermostat_max;
                entry.value = (self.config.native_utxo_thermostat_max).clone();
                self.balance += difference;
            }
        }

        Ok(())
    }

    /// balance the excess of a given asset (if any)
    ///
    /// This function let us know if the native asset was added in the change
    fn balance_excess<Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>>(&mut self, estimate: &mut Estimate) -> anyhow::Result<()> {
        if let Balance::Excess(_excess) = self.current_balance(estimate)? {
            let address = self
                .optional_change_address
                .as_ref()
                .ok_or_else(|| anyhow!("Change address required"))?;

            let default = UTxOBuilder::new(
                address.clone(),
                Value::zero(),
                vec![],
            )?;

            let _ = match self.changes.entry(self.config.main_token.clone()) {
                Entry::Vacant(entry) => {
                    estimate.add_output(default.clone());
                    entry.insert(default);
                }
                _ => {}
            };
        }

        if let Balance::Excess(excess) = self.current_balance(estimate)? {
            let address = self
                .optional_change_address
                .as_ref()
                .ok_or_else(|| anyhow!("Change address required"))?;

            let default = UTxOBuilder::new(
                address.clone(),
                Value::zero(),
                vec![],
            )?;

            let entry = match self.changes.entry(self.config.main_token.clone()) {
                Entry::Vacant(entry) => {
                    estimate.add_output(default.clone());
                    entry.insert(default)
                }
                Entry::Occupied(mut entry) => entry.into_mut(),
            };

            entry.value += &excess;
            self.balance -= excess;
        }

        Ok(())
    }

    /// split the accumulator (the changes) if needed
    fn split_accumulators<Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>>(&mut self, utxos: &UTxOStore, estimate: &mut Estimate) -> anyhow::Result<()> {
        for (token_id, change) in self.changes.iter_mut() {
            if let Some(total_current_balance) = utxos.get_balance_of(token_id) {
                let mut new = change.clone();

                if let Some(asset) = change.assets.get_mut(0) {
                    // be careful to re-accumulate the newly created UTxO otherwise we would
                    // be missing out in a potential large chunk of value when computing
                    // the pivot
                    let total_current_balance = total_current_balance + &asset.quantity;
                    let pivot = total_current_balance / self.config.num_accumulators_assets;

                    if asset.quantity > pivot {
                        let quantity = &mut new.assets.get_mut(0).unwrap().quantity;
                        *quantity = (quantity.clone() / 2).truncate();
                        asset.quantity -= quantity.clone();

                        let fee_for_output = estimate.fee_for_output(&new)?;
                        let value = &new.value - &fee_for_output;
                        new.value = (value / 2).truncate();
                        change.value -= &new.value;
                        self.balance += &fee_for_output;

                        estimate.add_output(new.clone())?;
                        self.extra_changes.push(new);
                    }
                } else if token_id == &self.config.main_token {
                    // be careful to re-accumulate the newly created UTxO otherwise we would
                    // be missing out in a potential large chunk of value when computing
                    // the pivot
                    let total_current_balance = total_current_balance;
                    let total_current_balance = total_current_balance + &change.value;
                    let pivot = total_current_balance / self.config.num_accumulators;
                    let fee_for_output = estimate.fee_for_output(&new)?;
                    let current = &change.value - &fee_for_output;

                    if current > pivot {
                        new.value = (&current / 2).truncate();
                        change.value -= &new.value;
                        self.balance += &fee_for_output;

                        self.extra_changes.push(new.clone());
                        estimate.add_output(new)?;
                    }
                } else {
                    // ignore
                }
            }
        }

        Ok(())
    }

    fn select<Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>>(&mut self, estimator: &mut Estimate) -> anyhow::Result<()> {
        let mut utxos = self.available_utxos.clone();

        let mut assets: Vec<_> = self.asset_balance.keys().cloned().collect();
        for asset in &assets {
            // 1. we select the inputs for the asset until we have greater or equal to the
            //    balance for that asset
            utxos = self.select_input_for_asset_until_balanced(utxos, asset, estimator)?;

            // 2. if we have excess we balance the excess to the appropriate change
            //    address.
            //    if we already have excess of the `MAIN_ASSET` we balance it on the
            //    output for the asset **IF and only IF** the output has less then
            //    a certain amount already.
            //
            //    at this stage we will do the re-balancing of the funds
            //    since we will create the change output for this asset
            //    only now.
            self.balance_excess_of_asset(&utxos, asset.clone(), estimator)?;
        }

        // 1. select inputs for the `MAIN_ASSET` until balanced or in excess:
        //    until we have enough to covert the transaction fee.
        //
        //    we don't balance the excess yet
        utxos = self.select_input_for_main_until_balanced(utxos, estimator)?;

        // 2. check if there are enough space for more inputs.

        // here we are adding the TokenId::MAIN at the end of the array of
        // remaining inputs allowed.
        //
        // This way it is not the priority yet, we will try to do the operation
        // on the native asset we have first. But then we will also try to
        // add an extra input in it too.
        //
        // this value is later popped out so we don't do something silly on the
        // handling of re-balancing the excess
        if self.asset_balance.is_empty() {
            assets.push(self.config.main_token.clone());
        }
        let mut empty = vec![false; assets.len()];
        let mut index = 0;

        while estimator.remaining_number_inputs_allowed()? > 0 {
            let asset = assets
                .get(index)
                .expect("We created it with the available values and index is capped by the len");

            if !empty[index] {
                if utxos.number_utxos_for_token(asset) <= self.config.num_accumulators {
                    empty[index] = true;
                } else if let Ok(u) = self.select_input_for(utxos.clone(), asset, estimator) {
                    utxos = u;
                } else {
                    empty[index] = true;
                }
            } else if empty.iter().copied().all(|b| b) {
                break;
            }

            // rem_euclid will panic if len is null. but this won't happen
            // since we know there is at least one item in the assets array
            index = index.saturating_add(1) % assets.len();
        }
        // We pop out the TokenId::MAIN so we don't do something silly on the
        // handling of re-balancing the excess
        if self.asset_balance.is_empty() {
            let _ = assets.pop();
        }

        for asset in assets {
            self.balance_excess_of_asset(&utxos, asset, estimator)?;
        }

        self.balance_excess(estimator)?;

        self.split_accumulators(&utxos, estimator)
    }

    pub fn set_utxos(
        &mut self,
        available_inputs: UTxOStore,
    ) -> anyhow::Result<()> {
        self.available_utxos = available_inputs;
        Ok(())
    }
}

impl InputSelectionAlgorithm for Thermostat
{
    type InputUtxo = UTxODetails;
    type OutputUtxo = UTxOBuilder;

    fn set_available_inputs(
        &mut self,
        available_inputs: Vec<Self::InputUtxo>,
    ) -> anyhow::Result<()> {
        let mut utxos = self.available_utxos.thaw();
        for input in available_inputs.into_iter() {
            utxos.insert(input)?
        }
        self.available_utxos = utxos.freeze();
        Ok(())
    }

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>> {
        for (token, asset) in input_output_setup.input_asset_balance.iter() {
            *self.asset_balance.entry(token.clone()).or_insert(Balance::zero()) += asset.quantity.clone();
        }
        for (token, asset) in input_output_setup.output_asset_balance.iter() {
            *self.asset_balance.entry(token.clone()).or_insert(Balance::zero()) -= asset.quantity.clone();
        }
        self.balance += &input_output_setup.input_balance;
        self.balance -= &input_output_setup.output_balance;
        self.optional_change_address = input_output_setup.change_address;

        self.select(estimator)?;

        let input_balance = &input_output_setup.input_balance + &self.selected_inputs_value;
        let mut input_asset_balance = input_output_setup.input_asset_balance;
        for input in self.selected_inputs.iter() {
            for asset in input.assets.iter() {
                input_asset_balance.entry(asset.fingerprint.clone()).or_insert(TransactionAsset {
                    policy_id: asset.policy_id.clone(),
                    asset_name: asset.asset_name.clone(),
                    fingerprint: asset.fingerprint.clone(),
                    quantity: Value::zero(),
                }).quantity += &asset.quantity;
            }
        }
        let mut output_balance = input_output_setup.output_balance;
        let mut output_asset_balance = input_output_setup.output_asset_balance;
        for input in self.changes.values().chain(self.extra_changes.iter()) {
            for asset in input.assets.iter() {
                output_asset_balance.entry(asset.fingerprint.clone()).or_insert(TransactionAsset {
                    policy_id: asset.policy_id.clone(),
                    asset_name: asset.asset_name.clone(),
                    fingerprint: asset.fingerprint.clone(),
                    quantity: Value::zero(),
                }).quantity += &asset.quantity;
            }
            output_balance += &input.value;
        }
        Ok(InputSelectionResult {
            input_balance,
            input_asset_balance,
            output_balance,
            output_asset_balance,
            balance: self.balance.clone(),
            asset_balance: self.asset_balance.clone(),
            fixed_inputs: input_output_setup.fixed_inputs,
            fixed_outputs: input_output_setup.fixed_outputs,
            chosen_inputs: self.selected_inputs.clone(),
            changes: self.changes.values().chain(self.extra_changes.iter()).cloned().collect(),
            fee: estimator.min_required_fee()?,
        })
    }

    fn available_inputs(&self) -> Vec<Self::InputUtxo> {
        todo!()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use dcspark_core::cardano::Ada;
    use dcspark_core::{Normalized, AssetName, PolicyId, OutputIndex};
    use dcspark_core::tx::TransactionId;
    use deps::serde_json;

    fn thermostat_config() -> ThermostatAlgoConfig {
        ThermostatAlgoConfig {
            num_accumulators: 20,
            num_accumulators_assets: 20,
            native_utxo_thermostat_min: Value::<Regulated>::from(50_000_000),
            native_utxo_thermostat_max: Value::<Regulated>::from(200_000_000),
            main_token: TokenId::MAIN,
        }
    }
    /// helper function to prepare a basic `Selection` structure
    /// with a basic `MultisigPlan`.
    ///
    /// This will be used to emulate the base cost of having the
    /// native script in cardano.
    ///
    fn selection() -> (Thermostat, ThermostatFeeEstimator) {
        let plan: MultisigPlan = serde_json::from_value(serde_json::json! {
            {
                "quorum": 2u8,
                "keys": [
                    "00000000000000000000000000000000000000000000000000000000",
                    "00000000000000000000000000000000000000000000000000000001",
                    "00000000000000000000000000000000000000000000000000000002",
                ]
            }
        })
            .unwrap();

        let thermostat = Thermostat::new(thermostat_config());
        let estimator = ThermostatFeeEstimator::new(NetworkInfo::Testnet, &plan);
        (thermostat, estimator)
    }

    /// macro to create a UTxO Asset
    ///
    /// ```
    /// let u = utxo_asset_sample!("Asset", "10");
    /// ```
    ///
    /// the first parameter is a fingerprint and will be used to identify the
    /// asset amongst the Store.
    ///
    /// The second parameter is the quantity. Expect the quantity to be a string
    /// because underlying it is using the UTxO data from CSL. Use the lowest
    /// value possible (if you desire to have a value with "1.01 Something"
    /// => then you will need to encode it as follow: "1010000")
    ///
    /// Maybe you will also need to define a RULE in order to have the right
    /// conversion and computation for that value. Though the input selection
    /// is not using this at all and is expecting to have the UTxO already
    /// encoded in the necessary value.
    ///
    macro_rules! utxo_asset_sample {
        () => {
            vec![]
        };
        ($fingerprint:literal, $quantity:literal) => {
            vec![TransactionAsset {
                policy_id: PolicyId::new(
                    "00000000000000000000000000000000000000000000000000000000",
                ),
                asset_name: AssetName::new("00000000"),
                fingerprint: TokenId::new($fingerprint),
                quantity: $quantity.parse().unwrap(),
            }]
        };
    }

    /// create a utxo sample
    ///
    /// add in the utxo store `utxo_store` a new UTxO:
    ///
    /// * pointer ("tx id", 1) (tx and output index)
    /// * value for main asset: 500 (500_000_000 in lovelace)
    /// * address is hard coded to be `addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj`
    ///   which is expected to be the address we control.
    ///
    /// ```
    /// utxo_sample!(utxo_store, "tx id", 1, "500",)
    /// ```
    ///
    /// add in the utxo store `utxo_store` a new UTxO:
    ///
    /// * pointer ("tx id", 1) (tx and output index)
    /// * value for main asset: 2 (2_000_000 in Lovelace)
    /// * a native asset "tDRIP" of "1000" unit
    /// * address is hard coded to be `addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj`
    ///   which is expected to be the address we control.
    ///
    /// ```
    /// utxo_sample!(utxo_store, "tx id", 0, "2", "tDRIP", "1000");
    /// ```
    macro_rules! utxo_sample {
        ($utxo_store:ident, $TxId:expr, $OutputIndex:expr, $value:literal, $($assets:tt)* ) => {
            $utxo_store
                .insert(UTxODetails {
                    pointer: UtxoPointer {
                        transaction_id: TransactionId::new($TxId),
                        output_index: OutputIndex::new($OutputIndex),
                    },
                    address: Address::new_static(
                        "addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj",
                    ),
                    value: $value.parse().unwrap(),
                    assets: utxo_asset_sample!($($assets)*),
                    metadata: Arc::new(serde_json::Value::Null),
                })
                .unwrap();
        };
    }

    fn utxos(len: usize) -> UTxOStore {
        let mut utxo_store = UTxOStore::new().thaw();

        for i in 0..len {
            let tx_id = format!("{i:032}");
            utxo_sample!(utxo_store, tx_id.clone(), 0, "2.5", "tDRIP", "1000");
            utxo_sample!(utxo_store, tx_id.clone(), 1, "500",);
            utxo_sample!(utxo_store, tx_id.clone(), 2, "500", "tDRIP", "1000000");
            utxo_sample!(utxo_store, tx_id.clone(), 3, "5",);
        }

        utxo_store.freeze()
    }

    fn sample_output(
        id: u64,
    ) -> (
        Address,
        Value<Regulated>,
        Vec<TransactionAsset>,
    ) {
        let address =
            Address::new("addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj");
        let value: Value<Ada> = "3".parse().unwrap();
        let assets = Vec::new();
        (address, value.to_lovelace().to_regulated(), assets)
    }

    /// if we have enough WMAIN we don't refill unless we go under
    #[test]
    fn test_thermostat_within_boundary() {
        let (mut thermostat, mut estimator) = selection();

        const USER_ADDRESS: &str =
            "addr_test1qqpftzcepsz6c4ecapkr8vzxmyev8yqlny53xp3kxd4p3kuzn0g6ackzyh9r2kj9kgdqx6npjulm3fy6fe9v6unwxxkqxjer8j";

        let mut utxos = UTxOStore::new().thaw();
        utxo_sample!(
            utxos,
            "transaction 1",
            0,
            "200_000_000",
            "My Token",
            "9_000_000_000_000"
        );
        utxo_sample!(utxos, "transaction 2", 0, "9000000_000_000",);
        let utxos = utxos.freeze();

        let address = Address::new(USER_ADDRESS);
        let value: Value<Regulated> = "3_000_000".parse().unwrap();
        let my_token = TokenId::new_static("My Token");
        let assets = utxo_asset_sample!("My Token", "1_000_000");

        estimator.add_protocol_magic("unittest.cardano-evm.c1");
        let output = UTxOBuilder::new(address, value, assets).unwrap();
        let setup = InputOutputSetup::<UTxODetails, UTxOBuilder> {
            input_balance: Default::default(),
            input_asset_balance: Default::default(),
            output_balance: output.value.clone(),
            output_asset_balance: HashMap::from([(my_token, output.assets.first().cloned().unwrap())]),
            fixed_inputs: vec![],
            fixed_outputs: vec![output.clone()],
            change_address: Some(Address::new(
                "addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj",
            ))
        };

        thermostat.set_utxos(utxos).unwrap();
        estimator.add_output(output).unwrap();

        let result = thermostat.select_inputs(&mut estimator, setup).unwrap();

        let inputs = result.chosen_inputs;
        assert_eq!(inputs.len(), 1);
        let input = inputs[0].clone();

        assert_eq!(
            input.pointer.transaction_id,
            TransactionId::new("transaction 1")
        );
        assert_eq!(input.assets[0].fingerprint, TokenId::new("My Token"));

        let outputs = result.fixed_outputs;
        assert_eq!(outputs.len(), 1);
        let output = outputs[0].clone();
        assert_eq!(output.address, Address::new(USER_ADDRESS));
        assert_eq!(output.value, Value::<Regulated>::from(3_000_000));
        assert_eq!(output.assets.len(), 1);
        let output_asset = output.assets[0].clone();
        assert_eq!(output_asset.fingerprint, TokenId::new("My Token"));
        assert_eq!(output_asset.quantity, Value::from(1_000_000));

        let changes = result.changes;
        assert_eq!(changes.len(), 1);
        let change = changes.first().cloned().unwrap();
        assert_eq!(
            change.address,
            Address::new("addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj")
        );
        assert!(change.value < Value::<Regulated>::from(197_000_000));
        assert!(change.value > Value::<Regulated>::from(195_000_000));
        assert_eq!(change.assets.len(), 1);
        let change_asset = change.assets[0].clone();
        assert_eq!(change_asset.fingerprint, TokenId::new("My Token"));
        assert_eq!(
            change_asset.quantity,
            Value::from(9_000_000_000_000 - 1_000_000)
        );
    }

    /// test we have the correct minimum number of UTxOs value
    /// and we don't add new inputs if NUM_ACCUMULATORS is reached
    #[test]
    fn test_min_utxo_untouched() {
        const USER_ADDRESS: &str =
            "addr_test1qqpftzcepsz6c4ecapkr8vzxmyev8yqlny53xp3kxd4p3kuzn0g6ackzyh9r2kj9kgdqx6npjulm3fy6fe9v6unwxxkqxjer8j";

        let mut utxos = UTxOStore::new().thaw();
        for i in 2..(thermostat_config().num_accumulators + 2) {
            utxo_sample!(
                utxos,
                "7eb432314c5b25609ec7c708a4615a9ee7546aacb0118915ef965c092091ce54",
                i as u64,
                "9_475_783636258",
            );
        }
        utxo_sample!(
            utxos,
            "7eb432314c5b25609ec7c708a4615a9ee7546aacb0118915ef965c092091ce54",
            1,
            "196797803",
            "m10s18",
            "9_530_179_629_629_632"
        );
        let utxos = utxos.freeze();

        let address = Address::new(USER_ADDRESS);
        let value: Value<Regulated> = "3_000_000".parse().unwrap();
        let assets = utxo_asset_sample!("m10s18", "100_123_456_789");

        let (mut thermostat, mut estimator) = selection();
        estimator.add_protocol_magic("unittest.cardano-evm.c1");

        let output = UTxOBuilder::new(address, value, assets).unwrap();
        let setup = InputOutputSetup::<UTxODetails, UTxOBuilder> {
            input_balance: Default::default(),
            input_asset_balance: Default::default(),
            output_balance: output.value.clone(),
            output_asset_balance: HashMap::from([(TokenId::new("m10s18"), output.assets.first().cloned().unwrap())]),
            fixed_inputs: vec![],
            fixed_outputs: vec![output.clone()],
            change_address: Some(Address::new(
                "addr_test1wz6lvjg3anml96vl22mls5vae3x2cgaqwy2ewp5gj3fcxdcw652wz",
            ))
        };

        thermostat.set_utxos(utxos).unwrap();
        estimator.add_output(output).unwrap();

        let result = thermostat.select_inputs(&mut estimator, setup).unwrap();

        let inputs = result.chosen_inputs;
        assert_eq!(inputs.len(), 1);
        let input = inputs[0].clone();

        assert_eq!(
            input.pointer.transaction_id,
            TransactionId::new("7eb432314c5b25609ec7c708a4615a9ee7546aacb0118915ef965c092091ce54")
        );
        assert_eq!(input.assets[0].fingerprint, TokenId::new("m10s18"));

        let outputs = result.fixed_outputs;
        assert_eq!(outputs.len(), 1);
        let output = outputs[0].clone();
        assert_eq!(output.address, Address::new(USER_ADDRESS));
        assert_eq!(output.value, Value::<Regulated>::from(3_000_000));
        assert_eq!(output.assets.len(), 1);
        let output_asset = output.assets[0].clone();
        assert_eq!(output_asset.fingerprint, TokenId::new("m10s18"));
        assert_eq!(output_asset.quantity, Value::from(100_123_456_789));

        let changes = result.changes;
        assert_eq!(changes.len(), 1);
        let change = changes.first().cloned().unwrap();
        assert_eq!(
            change.address,
            Address::new("addr_test1wz6lvjg3anml96vl22mls5vae3x2cgaqwy2ewp5gj3fcxdcw652wz")
        );
        assert!(change.value < Value::<Regulated>::from(195_000_000),);
        assert!(change.value > Value::<Regulated>::from(193_000_000),);
        assert_eq!(change.assets.len(), 1);
        let change_asset = change.assets[0].clone();
        assert_eq!(change_asset.fingerprint, TokenId::new("m10s18"));
        assert_eq!(
            change_asset.quantity,
            Value::from(9_530_179_629_629_632 - 100_123_456_789)
        );
    }

    /// test the thermostat that if we go under the min threshold we request
    /// more ada to go in the UTxO
    #[test]
    fn test_thermostat_min_boundary() {
        const USER_ADDRESS: &str =
            "addr_test1qqpftzcepsz6c4ecapkr8vzxmyev8yqlny53xp3kxd4p3kuzn0g6ackzyh9r2kj9kgdqx6npjulm3fy6fe9v6unwxxkqxjer8j";

        let mut utxos = UTxOStore::new().thaw();
        utxo_sample!(
            utxos,
            "transaction 1",
            0,
            "51_000000", // the min threshold 50
            "My Token",
            "9_000_000_000_000"
        );
        utxo_sample!(utxos, "transaction 2", 0, "9_000_000_000000",);
        let utxos = utxos.freeze();

        let address = Address::new(USER_ADDRESS);
        let value: Value<Regulated> = "3_000000".parse().unwrap();
        let assets = utxo_asset_sample!("My Token", "1_000_000");

        let (mut thermostat, mut estimator) = selection();
        estimator.add_protocol_magic("unittest.cardano-evm.c1");

        let output = UTxOBuilder::new(address, value, assets).unwrap();
        let setup = InputOutputSetup::<UTxODetails, UTxOBuilder> {
            input_balance: Default::default(),
            input_asset_balance: Default::default(),
            output_balance: output.value.clone(),
            output_asset_balance: HashMap::from([(TokenId::new("My Token"), output.assets.first().cloned().unwrap())]),
            fixed_inputs: vec![],
            fixed_outputs: vec![output.clone()],
            change_address: Some(Address::new(
                "addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj",
            ))
        };

        thermostat.set_utxos(utxos).unwrap();
        estimator.add_output(output).unwrap();

        let result = thermostat.select_inputs(&mut estimator, setup).unwrap();

        let inputs = result.chosen_inputs;
        assert_eq!(inputs.len(), 2);
        let input = inputs[0].clone();
        assert_eq!(
            input.pointer.transaction_id,
            TransactionId::new("transaction 1")
        );
        assert_eq!(input.assets[0].fingerprint, TokenId::new("My Token"));
        let input = inputs[1].clone();
        assert_eq!(
            input.pointer.transaction_id,
            TransactionId::new("transaction 2")
        );
        assert!(input.assets.is_empty());

        let outputs = result.fixed_outputs;
        assert_eq!(outputs.len(), 1);
        let output = outputs[0].clone();
        assert_eq!(output.address, Address::new(USER_ADDRESS));
        assert_eq!(output.value, Value::<Regulated>::from(3_000_000));
        assert_eq!(output.assets.len(), 1);
        let output_asset = output.assets[0].clone();
        assert_eq!(output_asset.fingerprint, TokenId::new("My Token"));
        assert_eq!(output_asset.quantity, Value::from(1_000_000));

        let changes = result.changes;
        assert_eq!(changes.len(), 2);
        let change = changes.iter().find(|change| !change.assets.is_empty() && change.assets.iter().any(|asset| asset.fingerprint == TokenId::new("My Token"))).unwrap();
        assert_eq!(
            change.address,
            Address::new("addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj")
        );
        assert_eq!(change.value, thermostat_config().native_utxo_thermostat_max);
        assert_eq!(change.assets.len(), 1);
        let change_asset = change.assets[0].clone();
        assert_eq!(change_asset.fingerprint, TokenId::new("My Token"));
        assert_eq!(
            change_asset.quantity,
            Value::from(9_000_000_000_000 - 1_000_000)
        );

        let change = changes.iter().find(|change| change.assets.is_empty()).unwrap();

        assert_eq!(
            change.address,
            Address::new("addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj")
        );
        let value: Value<Regulated> = Value::<Ada>::from(9_000_000).to_lovelace().to_regulated();
        let value = value - Value::<Ada>::from(152).to_lovelace().to_regulated();
        let value_after_fees = &value - &Value::<Ada>::from(2).to_lovelace().to_regulated();
        assert!(change.value < value);
        assert!(
            change.value > value_after_fees,
            "{value} > {value_after_fees}",
            value = change.value,
        );
        assert_eq!(change.assets.len(), 0);
    }

    // /// test splitting in two without regrouping
    // #[test]
    // fn test_mindblower_3() {
    //     let mut selection = selection();
    //     let pending_state = PendingState::temporary();
    //
    //     const USER_ADDRESS: &str =
    //         "addr_test1qqpftzcepsz6c4ecapkr8vzxmyev8yqlny53xp3kxd4p3kuzn0g6ackzyh9r2kj9kgdqx6npjulm3fy6fe9v6unwxxkqxjer8j";
    //
    //     let mut utxos = UTxOStore::new().thaw();
    //     utxo_sample!(utxos, "transaction 1", 0, "9_100_000",);
    //     for i in 0..9 {
    //         let tx = format!("{i}");
    //         utxo_sample!(utxos, tx, 0, "100_000",);
    //     }
    //     let utxos = utxos.freeze();
    //
    //     // total ada
    //     let total_ada = Value::<bridge_ir::cardano::Lovelace>::from_regulated(
    //         &utxos.get_balance_of(&TokenId::MAIN).unwrap(),
    //     )
    //         .to_ada();
    //     assert_eq!(total_ada, Value::from(10_000_000));
    //     assert_eq!(total_ada / *NUM_ACCUMULATORS, Value::from(500_000));
    //
    //     let request = ActionId::new("My Request");
    //     let address = Address::new(USER_ADDRESS);
    //     let value: Value<Normalized> = "3".parse().unwrap();
    //     let assets = vec![];
    //
    //     selection.add_protocol_magic("unittest.cardano-evm.c1");
    //     selection.add_optional_change_address(Address::new(
    //         "addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj",
    //     ));
    //     selection.add_output(UTxOBuilder::new(address, value, assets, Some(request)).unwrap());
    //
    //     selection.select(utxos, &pending_state).unwrap();
    //
    //     let inputs = selection.inputs();
    //     assert_eq!(inputs.len(), 1);
    //     let input = inputs[0].clone();
    //     assert_eq!(
    //         input.pointer.transaction_id,
    //         TransactionId::new("transaction 1")
    //     );
    //
    //     let outputs = selection.outputs();
    //     assert_eq!(outputs.len(), 1);
    //
    //     let ada_changes: Vec<_> = selection
    //         .changes()
    //         .filter(|txb| txb.assets.is_empty())
    //         .cloned()
    //         .collect();
    //     assert_eq!(ada_changes.len(), 2);
    //     let max_expected_value = Value::<Ada>::from(9_100_000 / 2).to_lovelace();
    //     let min_expected_value = Value::<Ada>::from((9_100_000 - 3 - 2) / 2).to_lovelace();
    //     for (i, change) in ada_changes.into_iter().enumerate() {
    //         assert!(
    //             change.value > min_expected_value,
    //             "change[{i}]({value}) > {min_expected_value}",
    //             value = change.value,
    //         );
    //         assert!(
    //             change.value < max_expected_value,
    //             "change[{i}]({value}) < {max_expected_value}",
    //             value = change.value,
    //         );
    //     }
    // }
    //
    // /// test splitting in two with regrouping
    // #[test]
    // fn test_mindblower_4() {
    //     let mut selection = selection();
    //     let pending_state = PendingState::temporary();
    //
    //     const USER_ADDRESS: &str =
    //         "addr_test1qqpftzcepsz6c4ecapkr8vzxmyev8yqlny53xp3kxd4p3kuzn0g6ackzyh9r2kj9kgdqx6npjulm3fy6fe9v6unwxxkqxjer8j";
    //
    //     let mut utxos = UTxOStore::new().thaw();
    //     utxo_sample!(utxos, "transaction 2", 0, "9_000_000",);
    //     for i in 0..100 {
    //         let tx = format!("{i}");
    //         utxo_sample!(utxos, tx, 0, "10_000",);
    //     }
    //     let utxos = utxos.freeze();
    //
    //     // total ada
    //     let total_ada = Value::<bridge_ir::cardano::Lovelace>::from_regulated(
    //         &utxos.get_balance_of(&TokenId::MAIN).unwrap(),
    //     )
    //         .to_ada();
    //     assert_eq!(total_ada, Value::from(10_000_000));
    //     assert_eq!(total_ada / *NUM_ACCUMULATORS, Value::from(500_000));
    //
    //     let request = ActionId::new("My Request");
    //     let address = Address::new(USER_ADDRESS);
    //     let value: Value<Normalized> = "3".parse().unwrap();
    //     let assets = vec![];
    //
    //     selection.add_protocol_magic("unittest.cardano-evm.c1");
    //     selection.add_optional_change_address(Address::new(
    //         "addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj",
    //     ));
    //     selection.add_output(UTxOBuilder::new(address, value, assets, Some(request)).unwrap());
    //     selection.select(utxos, &pending_state).unwrap();
    //
    //     let inputs = selection.inputs();
    //     assert_eq!(inputs.len(), 81);
    //
    //     let outputs = selection.outputs();
    //     assert_eq!(outputs.len(), 1);
    //
    //     let ada_changes: Vec<_> = selection
    //         .changes()
    //         .filter(|txb| txb.assets.is_empty())
    //         .cloned()
    //         .collect();
    //     assert_eq!(ada_changes.len(), 2);
    //     let max_expected_value = Value::<Ada>::from(9_800_000 / 2).to_lovelace();
    //     let min_expected_value = Value::<Ada>::from((9_800_000 - 3 - 2) / 2).to_lovelace();
    //     for (i, change) in ada_changes.into_iter().enumerate() {
    //         assert!(
    //             change.value > min_expected_value,
    //             "change[{i}]({value}) > {min_expected_value}",
    //             value = change.value,
    //         );
    //         assert!(
    //             change.value < max_expected_value,
    //             "change[{i}]({value}) < {max_expected_value}",
    //             value = change.value,
    //         );
    //     }
    //
    //     // 4_899_998_342_164
    //     // 4_999_997_000_000
    //     // 4_999_996_000_000
    // }
    //
    // /// test we are regrouping accumulators **but** only splitting **if**
    // /// the change output is larger than the _accumulator value target_
    // #[test]
    // fn test_mindblower_5() {
    //     let mut selection = selection();
    //     let pending_state = PendingState::temporary();
    //
    //     const USER_ADDRESS: &str =
    //         "addr_test1qqpftzcepsz6c4ecapkr8vzxmyev8yqlny53xp3kxd4p3kuzn0g6ackzyh9r2kj9kgdqx6npjulm3fy6fe9v6unwxxkqxjer8j";
    //
    //     let mut utxos = UTxOStore::new().thaw();
    //     for i in 0..20 {
    //         let tx = format!("accumulator {i}");
    //         utxo_sample!(utxos, tx, 0, "1_000_000",);
    //     }
    //     let utxos = utxos.freeze();
    //
    //     // total ada
    //     let total_ada = utxos.get_balance_of(&TokenId::MAIN).unwrap();
    //     assert_eq!(total_ada, Value::from(20_000_000_000_000));
    //     assert_eq!(
    //         total_ada / *NUM_ACCUMULATORS,
    //         Value::from(1_000_000_000_000)
    //     );
    //
    //     let request = ActionId::new("My Request");
    //     let address = Address::new(USER_ADDRESS);
    //     let value: Value<Normalized> = "2_000_000".parse().unwrap();
    //     let assets = vec![];
    //
    //     selection.add_protocol_magic("unittest.cardano-evm.c1");
    //     selection.add_optional_change_address(Address::new(
    //         "addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj",
    //     ));
    //     selection.add_output(UTxOBuilder::new(address, value, assets, Some(request)).unwrap());
    //
    //     selection.select(utxos, &pending_state).unwrap();
    //
    //     let inputs = selection.inputs();
    //     assert_eq!(inputs.len(), 3);
    //
    //     let outputs = selection.outputs();
    //     assert_eq!(outputs.len(), 1);
    //
    //     let ada_changes: Vec<_> = selection
    //         .changes()
    //         .filter(|txb| txb.assets.is_empty())
    //         .cloned()
    //         .collect();
    //     // we are expecting a split to happen. Indeed we are withdrawing 2millions Ada
    //     // from the UTxO so we are dropping the total ada to 18millions for 20 accumulator.
    //     // the pivot is now `18m / 20m = 0.9m`.
    //     //
    //     // without the split of the accumulator, we would have an a change of
    //     // `999_999.808099` (0.99m). So we need to split it in order to have
    //     // 17 UTxO with 1m and 2 UTxO with 0.5m.
    //     assert_eq!(ada_changes.len(), 2);
    //     let max_expected_value = Value::<Ada>::from(500_000).to_lovelace();
    //     let min_expected_value = Value::<Ada>::from(499_997).to_lovelace();
    //     for change in ada_changes {
    //         assert!(
    //             change.value > min_expected_value,
    //             "{value} > {min_expected_value}",
    //             value = change.value
    //         );
    //         assert!(
    //             change.value < max_expected_value,
    //             "{value} < {max_expected_value}",
    //             value = change.value
    //         );
    //     }
    // }
    //
    // #[test]
    // fn test_1() {
    //     let mut selection = selection();
    //     let pending_state = PendingState::temporary();
    //     let utxos = utxos(1);
    //     let (output_action, output_address, _, _) = sample_output(1);
    //     let output_assets = utxo_asset_sample!("tDRIP", "100");
    //
    //     selection.add_protocol_magic("unittest.cardano-evm.c1");
    //     selection.add_optional_change_address(Address::new(
    //         "addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj",
    //     ));
    //     selection.add_output(
    //         UTxOBuilder::new(
    //             output_address,
    //             Value::<cardano::Ada>::from(3),
    //             output_assets,
    //             Some(output_action),
    //         )
    //             .unwrap(),
    //     );
    //
    //     selection.select(utxos, &pending_state).unwrap();
    //
    //     dbg!(selection.required_fees());
    //     dbg!(selection.inputs());
    //     dbg!(selection.outputs());
    //     dbg!(selection.changes().collect::<Vec<_>>());
    //
    //     assert_eq!(selection.current_balance(), Balance::Balanced);
    //     for (asset, balance) in selection.asset_balance.iter() {
    //         assert_eq!(balance, &Balance::Balanced, "Failed to balance {asset}");
    //     }
    // }
    //
    // #[test]
    // fn test_2() {
    //     let mut selection = selection();
    //     let pending_state = PendingState::temporary();
    //     let utxos = utxos(1);
    //     let (output_action, output_address, _, _) = sample_output(20_000);
    //     let output_assets = utxo_asset_sample!("tDRIP", "100");
    //
    //     selection.add_protocol_magic("unittest.cardano-evm.c1");
    //     selection.add_optional_change_address(Address::new(
    //         "addr_test1wpjf80wvstelml6vw7d46y6j6575klf3s4mxp7ytrcrz5ecl33pgj",
    //     ));
    //     selection.add_output(
    //         UTxOBuilder::new(
    //             output_address,
    //             Value::<cardano::Ada>::from(3),
    //             output_assets,
    //             Some(output_action),
    //         )
    //             .unwrap(),
    //     );
    //
    //     selection.select(utxos, &pending_state).unwrap();
    //
    //     dbg!(selection.required_fees());
    //     dbg!(selection.inputs());
    //     dbg!(selection.outputs());
    //     dbg!(selection.changes().collect::<Vec<_>>());
    //
    //     assert_eq!(selection.current_balance(), Balance::Balanced);
    //     for (asset, balance) in selection.asset_balance.iter() {
    //         assert_eq!(balance, &Balance::Balanced, "Failed to balance {asset}");
    //     }
    // }
}
