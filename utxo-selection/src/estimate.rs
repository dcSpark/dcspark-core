use dcspark_core::{Regulated, Value};
use std::marker::PhantomData;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::TransactionOutput;
use dcspark_core::tx::{UTxOBuilder, UTxODetails};

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

    fn fee_for_output(&self, output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>>;
    fn add_output(&mut self, output: Self::OutputUtxo) -> anyhow::Result<()>;

    fn current_size(&self) -> anyhow::Result<usize>;
    fn max_size(&self) -> anyhow::Result<usize>;
}

pub struct ConvertedFeeEstimate<InputFrom, InputTo, OutputFrom, OutputTo, Estimator: TransactionFeeEstimator<InputUtxo=InputTo, OutputUtxo=OutputTo>> {
    inner: Estimator,
    phantom: PhantomData<(InputFrom, OutputFrom)>,
}

impl<InputFrom, InputTo, OutputFrom, OutputTo, Estimator: TransactionFeeEstimator<InputUtxo=InputTo, OutputUtxo=OutputTo>> ConvertedFeeEstimate<InputFrom, InputTo, OutputFrom, OutputTo, Estimator> {
    pub fn new(inner: Estimator) -> Self {
        Self {
            inner,
            phantom: Default::default(),
        }
    }
}

impl<Estimator: TransactionFeeEstimator<InputUtxo=InputBuilderResult, OutputUtxo=TransactionOutput>>
    TransactionFeeEstimator for ConvertedFeeEstimate<UTxODetails, InputBuilderResult, UTxOBuilder, TransactionOutput, Estimator> {
    type InputUtxo = UTxODetails;
    type OutputUtxo = UTxOBuilder;

    fn min_required_fee(&self) -> anyhow::Result<Value<Regulated>> {
        self.inner.min_required_fee()
    }

    fn fee_for_input(&self, input: &Self::InputUtxo) -> anyhow::Result<Value<Regulated>> {
        let input = cardano_utils::conversion::input_to_input_builder_result(input.clone())?;
        self.inner.fee_for_input(&input)
    }

    fn add_input(&mut self, input: Self::InputUtxo) -> anyhow::Result<()> {
        let input = cardano_utils::conversion::input_to_input_builder_result(input)?;
        self.inner.add_input(input)
    }

    fn fee_for_output(&self, output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>> {
        let output = cardano_utils::conversion::output_to_output_builder(output.clone())?;
        self.inner.fee_for_output(&output)
    }

    fn add_output(&mut self, output: Self::OutputUtxo) -> anyhow::Result<()> {
        let output = cardano_utils::conversion::output_to_output_builder(output)?;
        self.inner.add_output(output)
    }

    fn current_size(&self) -> anyhow::Result<usize> {
        self.inner.current_size()
    }

    fn max_size(&self) -> anyhow::Result<usize> {
        self.inner.max_size()
    }
}

impl<Estimator: TransactionFeeEstimator<InputUtxo=UTxODetails, OutputUtxo=UTxOBuilder>>
    TransactionFeeEstimator for ConvertedFeeEstimate<InputBuilderResult, UTxODetails, TransactionOutput, UTxOBuilder, Estimator> {
    type InputUtxo = InputBuilderResult;
    type OutputUtxo = TransactionOutput;

    fn min_required_fee(&self) -> anyhow::Result<Value<Regulated>> {
        self.inner.min_required_fee()
    }

    fn fee_for_input(&self, input: &Self::InputUtxo) -> anyhow::Result<Value<Regulated>> {
        let input = cardano_utils::conversion::input_builder_result_to_input(input.clone())?;

        self.inner.fee_for_input(&input)
    }

    fn add_input(&mut self, input: Self::InputUtxo) -> anyhow::Result<()> {
        let input = cardano_utils::conversion::input_builder_result_to_input(input)?;
        self.inner.add_input(input)
    }

    fn fee_for_output(&self, output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>> {
        let output = cardano_utils::conversion::output_to_utxo_builder(output.clone())?;
        self.inner.fee_for_output(&output)
    }

    fn add_output(&mut self, output: Self::OutputUtxo) -> anyhow::Result<()> {
        let output = cardano_utils::conversion::output_to_utxo_builder(output)?;
        self.inner.add_output(output)
    }

    fn current_size(&self) -> anyhow::Result<usize> {
        self.inner.current_size()
    }

    fn max_size(&self) -> anyhow::Result<usize> {
        self.inner.max_size()
    }
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
