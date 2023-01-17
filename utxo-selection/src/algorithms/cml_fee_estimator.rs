use anyhow::anyhow;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::builders::output_builder::SingleOutputBuilderResult;
use cardano_multiplatform_lib::builders::tx_builder::TransactionBuilder;
use cardano_multiplatform_lib::TransactionOutput;
use dcspark_core::{Regulated, Value};

use crate::TransactionFeeEstimator;

pub struct CmlFeeEstimator {
    builder: TransactionBuilder,
}

const DEFAULT_TX_SIZE: usize = 16384;

impl CmlFeeEstimator {
    pub fn new(mut tx_builder: TransactionBuilder) -> anyhow::Result<Self> {
        tx_builder.set_fee(
            &tx_builder
                .min_fee(true)
                .map_err(|err| anyhow!("can't set fee: {}", err))?,
        );
        Ok(Self {
            builder: tx_builder,
        })
    }
}

impl TransactionFeeEstimator for CmlFeeEstimator {
    type InputUtxo = InputBuilderResult;
    type OutputUtxo = TransactionOutput;

    fn min_required_fee(&self) -> anyhow::Result<Value<Regulated>> {
        let fee = self
            .builder
            .min_fee(true)
            .map_err(|err| anyhow!("can't calculate fees: {}", err))?;

        Ok(Value::<Regulated>::from(u64::from(fee)))
    }

    fn fee_for_input(&self, input: &Self::InputUtxo) -> anyhow::Result<Value<Regulated>> {
        let mut builder = self.builder.clone();
        builder
            .add_input(input)
            .map_err(|err| anyhow!("Can't add input: {}", err))?;
        let fee = builder
            .min_fee(true)
            .map_err(|err| anyhow!("can't calculate fees: {}", err))?;
        Ok(Value::<Regulated>::from(u64::from(fee)))
    }

    fn add_input(&mut self, input: Self::InputUtxo) -> anyhow::Result<()> {
        self.builder
            .add_input(&input)
            .map_err(|err| anyhow!("Can't add input {}", err))
    }

    fn fee_for_output(&self, output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>> {
        let mut builder = self.builder.clone();
        let output = output_to_builder_result(output);
        println!("{:?}", output.output().amount().coin());
        builder
            .add_output(&output)
            .map_err(|err| anyhow!("Can't add output: {}", err))?;
        let fee = builder
            .min_fee(true)
            .map_err(|err| anyhow!("can't calculate fees: {}", err))?;
        println!("{:?}", fee);

        Ok(Value::<Regulated>::from(u64::from(fee)))
    }

    fn add_output(&mut self, output: Self::OutputUtxo) -> anyhow::Result<()> {
        println!("{:?}", output.amount().coin());

        let output = output_to_builder_result(&output);
        self.builder
            .add_output(&output)
            .map_err(|err| anyhow!("Can't add output {}", err))
    }

    fn current_size(&self) -> anyhow::Result<usize> {
        self.builder
            .full_size()
            .map_err(|err| anyhow!("can't calculate size: {}", err))
    }

    fn max_size(&self) -> anyhow::Result<usize> {
        Ok(DEFAULT_TX_SIZE)
    }
}

fn output_to_builder_result(output: &TransactionOutput) -> SingleOutputBuilderResult {
    SingleOutputBuilderResult::new(output)
}

#[cfg(test)]
mod tests {
    use cardano_multiplatform_lib::builders::tx_builder::{
        TransactionBuilderConfig, TransactionBuilderConfigBuilder,
    };
    use cardano_multiplatform_lib::ledger::alonzo::fees::LinearFee;
    use cardano_multiplatform_lib::ledger::common::value::BigNum;
    use cardano_multiplatform_lib::plutus::ExUnitPrices;
    use cardano_multiplatform_lib::UnitInterval;
    use std::sync::Arc;

    use crate::algorithms::{CmlFeeEstimator, Thermostat, ThermostatAlgoConfig};
    use crate::{ConvertedFeeEstimate, InputOutputSetup, InputSelectionAlgorithm};
    use dcspark_core::tx::{TransactionId, UTxOBuilder, UTxODetails, UtxoPointer};
    use dcspark_core::{Address, Value};

    fn builder_config() -> TransactionBuilderConfig {
        let coefficient = BigNum::from_str("44").unwrap();
        let constant = BigNum::from_str("155381").unwrap();
        let linear_fee = LinearFee::new(&coefficient, &constant);

        let coins_per_utxo_byte = BigNum::from_str("4310").unwrap();
        let pool_deposit = BigNum::from_str("500000000").unwrap();
        let key_deposit = BigNum::from_str("2000000").unwrap();
        let max_value_size = 5000;
        let max_tx_size = 16384;

        #[allow(deprecated)]
        TransactionBuilderConfigBuilder::new()
            .fee_algo(&linear_fee)
            .coins_per_utxo_byte(&coins_per_utxo_byte)
            .pool_deposit(&pool_deposit)
            .key_deposit(&key_deposit)
            .max_value_size(max_value_size)
            .max_tx_size(max_tx_size as u32)
            .ex_unit_prices(&ExUnitPrices::new(
                &UnitInterval::new(&BigNum::zero(), &BigNum::zero()),
                &UnitInterval::new(&BigNum::zero(), &BigNum::zero()),
            ))
            .collateral_percentage(0)
            .max_collateral_inputs(0)
            .build()
            .unwrap()
    }

    #[test]
    fn test_compatibility() {
        let mut estimator = ConvertedFeeEstimate::new(
            CmlFeeEstimator::new(
                cardano_multiplatform_lib::builders::tx_builder::TransactionBuilder::new(
                    &builder_config(),
                ),
            )
            .unwrap(),
        );
        let mut thermostat = Thermostat::new(ThermostatAlgoConfig::default());

        thermostat.set_available_inputs(vec![UTxODetails {
            pointer: UtxoPointer { transaction_id: TransactionId::new("ac8f9af3d7760348030515e007c84584537ad056ada73c8a0b86ada14b22d4e0"), output_index: Default::default() },
            address: Address::new("addr1q9meks43s2gg5w8s67n4wjfy476t6scg6h34x497le6j886pgt7rsny5d0ncq0ncm8mdm4xag8ej46fsf4fuxsnuhyxq4r0mlu"),
            value: Value::from(10000000),
            assets: vec![],
            metadata: Arc::new(Default::default())
        }]).unwrap();

        let result = thermostat.select_inputs(&mut estimator, InputOutputSetup {
            input_balance: Default::default(),
            input_asset_balance: Default::default(),
            output_balance: Value::from(1000000),
            output_asset_balance: Default::default(),
            fixed_inputs: vec![],
            fixed_outputs: vec![UTxOBuilder::new(Address::new("addr1q99d9num2ngfkamdpgttty6wk42p4tvvvmm29hqex7y9avexqm79yn72ukr3enfwwdtpeju0rha978puyx7g90jspvxqskjafk"), Value::from(1000000), vec![])],
            change_address: Some(Address::new("addr1q9meks43s2gg5w8s67n4wjfy476t6scg6h34x497le6j886pgt7rsny5d0ncq0ncm8mdm4xag8ej46fsf4fuxsnuhyxq4r0mlu")),
        }).unwrap();
        assert!((&result.balance - &result.fee).balanced());
    }
}
