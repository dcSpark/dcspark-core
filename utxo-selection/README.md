# UTxO selection library

The library intends to simplify the way input selection is done for UTxO-based blockchains like Cardano.
The fundamental idea is splitting the flow into 2 parts:
* actual selection algorithm
* fee estimation

Actual selection algorithm is represented by `InputSelectionAlgorithm` trait:
```rust
pub trait InputSelectionAlgorithm {
    type InputUtxo: Clone;
    type OutputUtxo: Clone;

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>>;
}
```

The trait aims to abstract the algorithms of selection in a way new ones can be created easily.
InputUtxo and OutputUtxo types are needed to make the trait more scalable, so that new libraries with new types can be integrated with this one. 
Besides, the algorithms can be combined with each other using the results from previous selections. This way selecting inputs and balancing the change can be done using different input selection algorithms.

To select inputs libraries may rely on some logic depending on fee calculation: how big the tx is already, how much it costs to add one more input or output. To facilitate that we introduce `TransactionFeeEstimator` trait:

```rust
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
```

### Fee estimation

For fee estimation we provide several implementations:
* `CmlFeeEstimator` - fee estimator based on cardano multiplatform library's transaction builder fee estimation mechanics
* `ThermostatFeeEstimator` - fee estimator designed for `Thermostat` input selection algorithm

We also provide a couple utility fee estimators:
* `DummyFeeEstimator` - for tests

If any other estimator is needed the trait is generic enough, so the end-users can implement it themselves.

### Algorithms

As the algorithms we provide 4 classical ones and one new:
* largest first (with multiasset support)
* random improve (with multiasset support)
* thermostat
* fee change balancer (dumps extra ada to fee)
* single output change balancer 

First 2 are classical algorithms (see cip2). The third one we designed ourselves for bridges use case.

### Thermostat algorithm

Thermostat algorithm's goal is to reduce the amount of small value utxos we have and keep good level of parallelism while working with utxos. The stages are the following:
1. We select inputs for assets & balance the excess of it adding change utxos (one per asset).
2. We select inputs for main asset (ada) & balance the excess of it.
3. If there's enough space to include more inputs for regrouping - we do that (adding up to assets number of additional inputs).
4. Then we balance excess of assets again (updating the changes)
5. Finally, we split changes to multiple changes if their relative values are big enough:
   1. we aim to maintain a set of accumulator utxos (set per asset)
   2. we split the accumulators if they have too big values / their number is less than a threshold
   3. this is configured through `ThermostatAlgoConfig`
