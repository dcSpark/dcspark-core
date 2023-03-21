use crate::TransactionFeeEstimator;
use dcspark_core::{Regulated, Value};
use std::marker::PhantomData;

pub(crate) struct DummyFeeEstimate<Input, Output> {
    phantom_data: PhantomData<(Input, Output)>,
}

impl<Input, Output> DummyFeeEstimate<Input, Output> {
    #[allow(unused)]
    pub fn new() -> Self {
        DummyFeeEstimate {
            phantom_data: Default::default(),
        }
    }
}

impl<Input: Clone, Output: Clone> TransactionFeeEstimator for DummyFeeEstimate<Input, Output> {
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

    fn fee_for_output(&self, _output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>> {
        Ok(Value::zero())
    }

    fn add_output(&mut self, _output: Self::OutputUtxo) -> anyhow::Result<()> {
        Ok(())
    }

    fn current_size(&self) -> anyhow::Result<usize> {
        Ok(usize::MIN)
    }

    fn max_size(&self) -> anyhow::Result<usize> {
        Ok(usize::MAX)
    }
}
