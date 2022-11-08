use crate::estimate::TransactionFeeEstimator;
use crate::{InputSelectionResult, UTxOBuilder};
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::error::JsError;
use cardano_multiplatform_lib::ledger::common::value::{from_bignum, BigNum, Coin, Value};
use cardano_multiplatform_lib::TransactionOutput;
use dcspark_core::tx::UTxODetails;
use dcspark_core::{Balance, Regulated, TokenId};
use rand::Rng;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use anyhow::anyhow;

pub fn cip2_largest_first_by<
    OutputUtxo,
    Estimator: TransactionFeeEstimator<InputUtxo = InputBuilderResult, OutputUtxo = OutputUtxo>,
    F,
>(
    estimator: &mut Estimator,
    available_inputs: &[InputBuilderResult],
    available_indices: &mut HashSet<usize>,
    input_total: &mut Value,
    output_total: &mut Value,
    fee: &mut Coin,
    by: F,
) -> anyhow::Result<HashSet<usize>>
where
    F: Fn(&Value) -> Option<BigNum>,
{
    let mut relevant_indices: Vec<usize> = available_indices.iter().cloned().collect();
    let mut chosen_indices = HashSet::<usize>::new();

    relevant_indices.retain(|i| by(&available_inputs[*i].utxo_info.amount()).is_some());
    // ordered in ascending order by predicate {by}
    relevant_indices
        .sort_by_key(|i| by(&available_inputs[*i].utxo_info.amount()).expect("filtered above"));

    // iterate in decreasing order for predicate {by}
    for i in relevant_indices.into_iter().rev() {
        if by(input_total).unwrap_or_else(BigNum::zero)
            >= by(output_total).expect("do not call on asset types that aren't in the output")
        {
            break;
        }
        let input = &available_inputs[i];
        // differing from CIP2, we include the needed fees in the targets instead of just output values
        let input_fee =
            cardano_utils::conversion::value_to_csl_coin(&estimator.fee_for_input(input)?)?;
        estimator.add_input(input.clone())?;

        *input_total = input_total.checked_add(&input.utxo_info.amount()).map_err(|err| anyhow!(err))?;
        *output_total = output_total.checked_add(&Value::new(&input_fee)).map_err(|err| anyhow!(err))?;
        *fee = fee.checked_add(&input_fee).map_err(|err| anyhow!(err))?;

        available_indices.remove(&i);
        chosen_indices.insert(i);
    }

    if by(input_total).unwrap_or_else(BigNum::zero)
        < by(output_total).expect("do not call on asset types that aren't in the output")
    {
        return Err(anyhow!("UTxO Balance Insufficient"));
    }

    Ok(chosen_indices)
}

#[allow(clippy::too_many_arguments)]
pub fn cip2_random_improve_by<
    OutputUtxo,
    Estimator: TransactionFeeEstimator<InputUtxo = InputBuilderResult, OutputUtxo = OutputUtxo>,
    F,
    R: Rng + ?Sized,
>(
    estimator: &mut Estimator,
    available_inputs: &[InputBuilderResult],
    available_indices: &mut BTreeSet<usize>,
    input_total: &mut Value,
    output_total: &mut Value,
    explicit_outputs: &[TransactionOutput],
    fee: &mut Coin,
    by: F,
    rng: &mut R,
) -> anyhow::Result<HashSet<usize>>
    where
        F: Fn(&Value) -> Option<BigNum>,
{
    let mut chosen_indices = HashSet::<usize>::new();

    // Phase 1: Random Selection
    let mut relevant_indices = available_indices
        .iter()
        .filter(|i| by(&available_inputs[**i].utxo_info.amount()).is_some())
        .cloned()
        .collect::<Vec<usize>>();
    let mut associated_indices: BTreeMap<TransactionOutput, Vec<usize>> = BTreeMap::new();
    let mut outputs = explicit_outputs
        .iter()
        .filter(|output| by(&output.amount()).is_some())
        .cloned()
        .collect::<Vec<TransactionOutput>>();
    outputs.sort_by_key(|output| by(&output.amount()).expect("filtered above"));
    for output in outputs.iter().rev() {
        // TODO: how should we adapt this to inputs being associated when running for other assets?
        // if we do these two phases for each asset and don't take into account the other runs for other assets
        // then we over-add (and potentially fail if we don't have plenty of inputs)
        // On the other hand, the improvement phase it difficult to determine if a change is an improvement
        // if we're trying to improve for multiple assets at a time without knowing how important each input is
        // e.g. maybe we have lots of asset A but not much of B
        // For now I will just have this be entirely separate per-asset but we might want to in a later commit
        // consider the improvements separately and have it take some kind of dot product / distance for assets
        // during the improvement phase and have the improvement phase target multiple asset types at once.
        // One issue with that is how to scale in between different assets. We could maybe normalize them by
        // dividing each asset type by the sum of the required asset type in all outputs.
        // Another possibility for adapting this to multiassets is when associating an input x for asset type a
        // we try and subtract all other assets b != a from the outputs we're trying to cover.
        // It might make sense to diverge further and not consider it per-output and to instead just match against
        // the sum of all outputs as one single value.
        let mut added = BigNum::zero();
        let needed = by(&output.amount())
            .ok_or_else(|| anyhow!("Transaction output proper amount is not found"))?;
        while added < needed {
            if relevant_indices.is_empty() {
                return Err(anyhow!("UTxO Balance Insufficient"));
            }
            let random_index = rng.gen_range(0..relevant_indices.len());
            let i = relevant_indices.swap_remove(random_index);
            available_indices.remove(&i);
            let input = &available_inputs[i];
            added = added.checked_add(
                &by(&input.utxo_info.amount())
                    .expect("do not call on asset types that aren't in the output"),
            ).map_err(|err| anyhow!(err))?;
            associated_indices
                .entry(output.clone())
                .or_default()
                .push(i);
        }
    }
    if !relevant_indices.is_empty() {
        // Phase 2: Improvement
        for output in outputs.iter_mut() {
            let associated = associated_indices
                .get_mut(output)
                .ok_or_else(|| anyhow!("Associated index by output key not found"))?;
            for i in associated.iter_mut() {
                let random_index = rng.gen_range(0..relevant_indices.len());
                let j: &mut usize = relevant_indices
                    .get_mut(random_index)
                    .ok_or_else(|| anyhow!("Relevant index by random index not found"))?;
                let should_improve = {
                    let input = &available_inputs[*i];
                    let new_input = &available_inputs[*j];
                    let cur = from_bignum(&input.utxo_info.amount().coin());
                    let new = from_bignum(&new_input.utxo_info.amount().coin());
                    let min = from_bignum(&output.amount().coin());
                    let ideal = 2 * min;
                    let max = 3 * min;
                    let move_closer =
                        (ideal as i128 - new as i128).abs() < (ideal as i128 - cur as i128).abs();
                    let not_exceed_max = new < max;

                    move_closer && not_exceed_max
                };
                if should_improve {
                    available_indices.insert(*i);
                    available_indices.remove(j);
                    std::mem::swap(i, j);
                }
            }
        }
    }

    // after finalizing the improvement we need to actually add these results to the builder
    for output in outputs.iter() {
        for i in associated_indices
            .get(output)
            .ok_or_else(|| anyhow!("Transaction output key not found"))?
            .iter()
        {
            let input = &available_inputs[*i];
            let input_fee =
                cardano_utils::conversion::value_to_csl_coin(&estimator.fee_for_input(input)?)?;
            estimator.add_input(input.clone())?;
            *input_total = input_total.checked_add(&input.utxo_info.amount()).map_err(|err| anyhow!(err))?;
            *output_total = output_total.checked_add(&Value::new(&input_fee)).map_err(|err| anyhow!(err))?;
            *fee = fee.checked_add(&input_fee).map_err(|err| anyhow!(err))?;
            chosen_indices.insert(*i);
        }
    }

    Ok(chosen_indices)
}

pub fn result_from_cml<
    InputUtxo: Clone,
    OutputUtxo: Clone,
>(
    fixed_inputs: Vec<InputUtxo>,
    fixed_outputs: Vec<OutputUtxo>,
    chosen_inputs: Vec<InputUtxo>,
    chosen_outputs: Vec<OutputUtxo>,
    input_total: Value,
    output_total: Value,
    fee: Coin,
) -> anyhow::Result<InputSelectionResult<InputUtxo, OutputUtxo>> {
    let (input_balance, input_asset_balance) =
        cardano_utils::conversion::csl_value_to_tokens(&input_total)?;
    let (output_balance, output_asset_balance) =
        cardano_utils::conversion::csl_value_to_tokens(&output_total)?;
    let fee = cardano_utils::conversion::csl_coin_to_value(&fee)?;

    let mut balance = Balance::zero();
    balance += input_balance.clone() - output_balance.clone();
    let mut asset_balance = HashMap::<TokenId, Balance<Regulated>>::new();
    for (id, asset) in input_asset_balance.iter() {
        let entry = asset_balance
            .entry(id.clone())
            .or_insert_with(Balance::zero);
        *entry += asset.quantity.clone();
    }
    for (id, asset) in output_asset_balance.iter() {
        let entry = asset_balance
            .entry(id.clone())
            .or_insert_with(Balance::zero);
        *entry -= asset.quantity.clone();
    }

    Ok(InputSelectionResult {
        fixed_inputs,
        fixed_outputs,
        chosen_inputs,
        changes: chosen_outputs,
        input_balance,
        output_balance,
        fee,

        input_asset_balance,
        output_asset_balance,

        balance,
        asset_balance,
    })
}
