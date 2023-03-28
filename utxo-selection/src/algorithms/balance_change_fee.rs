use crate::{
    are_assets_balanced, calculate_main_token_balance, InputOutputSetup, InputSelectionAlgorithm,
    InputSelectionResult, TransactionFeeEstimator, UTxOStoreSupport,
};
use anyhow::anyhow;
use dcspark_core::tx::{UTxOBuilder, UTxODetails};
use dcspark_core::{Balance, UTxOStore};

#[derive(Default)]
pub struct FeeChangeBalancer {}

impl UTxOStoreSupport for FeeChangeBalancer {
    fn set_available_utxos(&mut self, _utxos: UTxOStore) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_available_utxos(&mut self) -> anyhow::Result<UTxOStore> {
        Ok(Default::default())
    }
}
impl InputSelectionAlgorithm for FeeChangeBalancer {
    type InputUtxo = UTxODetails;
    type OutputUtxo = UTxOBuilder;

    fn set_available_inputs(
        &mut self,
        _available_inputs: Vec<Self::InputUtxo>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>> {
        if !are_assets_balanced(
            &input_output_setup.input_asset_balance,
            &input_output_setup.output_asset_balance,
        ) {
            return Err(anyhow!(
                "can't balance change when tokens are unbalanced. use other strategy"
            ));
        }

        let mut fee = estimator.min_required_fee()?;
        let current_balance = calculate_main_token_balance(
            &input_output_setup.input_balance,
            &input_output_setup.output_balance,
            &fee,
        );
        match current_balance {
            Balance::Debt(_d) => {
                return Err(anyhow!("there's not enough main token to balance change"));
            }
            Balance::Balanced => {
                // don't do anything
            }
            Balance::Excess(e) => {
                fee += &e;
            }
        }

        Ok(InputSelectionResult {
            input_balance: input_output_setup.input_balance,
            input_asset_balance: input_output_setup.input_asset_balance,
            output_balance: input_output_setup.output_balance,
            output_asset_balance: input_output_setup.output_asset_balance,
            fixed_inputs: input_output_setup.fixed_inputs,
            fixed_outputs: input_output_setup.fixed_outputs,
            chosen_inputs: vec![],
            changes: vec![],
            fee,
        })
    }

    fn available_inputs(&self) -> Vec<Self::InputUtxo> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::{FeeChangeBalancer, LargestFirst};
    use crate::estimators::dummy_estimator::DummyFeeEstimate;
    use crate::{InputOutputSetup, InputSelectionAlgorithm};
    use dcspark_core::tx::UTxOBuilder;
    use dcspark_core::{Address, Regulated, UTxOStore, Value};

    use crate::algorithms::test_utils::create_utxo;

    #[test]
    fn try_select_dummy_fee() {
        let mut store = UTxOStore::new().thaw();
        store
            .insert(create_utxo(
                0,
                0,
                "0".to_string(),
                Value::<Regulated>::from(10),
                vec![],
            ))
            .unwrap();
        let store = store.freeze();

        let mut largest_first = LargestFirst::try_from(store).unwrap();

        let result = largest_first
            .select_inputs(
                &mut DummyFeeEstimate::new(),
                InputOutputSetup {
                    input_balance: Default::default(),
                    input_asset_balance: Default::default(),
                    output_balance: Value::from(1),
                    output_asset_balance: Default::default(),
                    fixed_inputs: vec![],
                    fixed_outputs: vec![UTxOBuilder::new(
                        Address::new("unwrap"),
                        Value::<Regulated>::from(1),
                        vec![],
                    )],
                    change_address: None,
                },
            )
            .unwrap();

        assert_eq!(result.fee, Value::zero());
        assert_eq!(result.output_balance, Value::from(1));
        assert_eq!(result.input_balance, Value::from(10));
        assert_eq!(result.chosen_inputs.len(), 1);

        let mut balance_change = FeeChangeBalancer::default();

        let result = balance_change
            .select_inputs(
                &mut DummyFeeEstimate::new(),
                InputOutputSetup {
                    input_balance: result.input_balance,
                    input_asset_balance: result.input_asset_balance,
                    output_balance: result.output_balance,
                    output_asset_balance: result.output_asset_balance,
                    fixed_inputs: result.chosen_inputs,
                    fixed_outputs: result.fixed_outputs,
                    change_address: Some(Address::new("kek")),
                },
            )
            .unwrap();

        assert_eq!(result.fee, Value::from(9));
        assert_eq!(result.chosen_inputs.len(), 0);
        assert_eq!(result.fixed_inputs.len(), 1);
        assert_eq!(result.fixed_outputs.len(), 1);
        assert_eq!(result.changes.len(), 0);
        assert!(result.is_balanced());
    }
}
