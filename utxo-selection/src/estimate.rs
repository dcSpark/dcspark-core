use dcspark_core::{Regulated, Value};
use std::marker::PhantomData;

///
/// This trait is designed to hide the fee calculation under abstraction.
/// The end-user of the library can choose themselves how to estimate the fees.
///
pub trait TransactionFeeEstimator {
    type InputUtxo: Clone;
    type OutputUtxo: Clone;

    fn min_required_fee(&self) -> anyhow::Result<Value<Regulated>>;

    fn fee_for_input(&self, input: &Self::InputUtxo) -> anyhow::Result<Value<Regulated>>;
    fn add_input(&mut self, input: Self::InputUtxo) -> anyhow::Result<()>;
    fn remaining_number_inputs_allowed(&mut self) -> anyhow::Result<usize>;

    fn fee_for_output(&self, output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>>;
    fn add_output(&mut self, output: Self::OutputUtxo) -> anyhow::Result<()>;
}

pub(crate) struct DummyCmlFeeEstimate<Input, Output> {
    phantom_data: PhantomData<(Input, Output)>,
}

impl<Input, Output> DummyCmlFeeEstimate<Input, Output> {
    #[allow(unused)]
    pub fn new() -> Self {
        DummyCmlFeeEstimate {
            phantom_data: Default::default(),
        }
    }
}

impl<Input: Clone, Output: Clone> TransactionFeeEstimator for DummyCmlFeeEstimate<Input, Output> {
    type InputUtxo = Input;
    type OutputUtxo = Output;

    fn min_required_fee(&self) -> anyhow::Result<Value<Regulated>> {
        Ok(Value::zero())
    }

    fn fee_for_input(&self, _input: &Self::InputUtxo) -> anyhow::Result<Value<Regulated>> {
        Ok(Value::zero())
    }

    fn add_input(&mut self, _input: Self::InputUtxo) -> anyhow::Result<()> {
        Ok(())
    }

    fn remaining_number_inputs_allowed(&mut self) -> anyhow::Result<usize> {
        Ok(usize::MAX)
    }

    fn fee_for_output(&self, _output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>> {
        Ok(Value::zero())
    }

    fn add_output(&mut self, _output: Self::OutputUtxo) -> anyhow::Result<()> {
        Ok(())
    }
}
