use crate::multisig_plan::MultisigPlan;
use cardano_multiplatform_lib::builders::tx_builder::{
    TransactionBuilderConfig, TransactionBuilderConfigBuilder,
};
use cardano_multiplatform_lib::ledger::alonzo::fees::LinearFee;
use cardano_multiplatform_lib::ledger::common::value::{BigNum, Coin};
use cardano_multiplatform_lib::TransactionWitnessSet;
use serde::Deserialize;

// set an overhead so we keep some room
// we accept the extra transaction cost because
// cardano does not have deterministic encoding
// of transactions and so no deterministic fee
const ASSUMED_OVERHEAD: usize = 50;

const ASSUMED_SIZE_EMPTY_TX: usize = 17 + ASSUMED_OVERHEAD;
const ASSUMED_SIZE_OF_ONE_INPUT: usize
= 2  // size of a CBOR array with length > 23
    + 2 // CBOR Array + size of 32
    + 32 // size of the transaction hash
    + 2  // size of an index up to 255 entries
;
const ASSUMED_SIZE_OF_ONE_OUTPUT: usize
= 5  // size of a CBOR array with length > 24
    + 1 // array of a tuple
    + 2 // bytes of 57 entries
    + 57 // network_id + payment key hash + credential hash
    + 1 // array of a tuple
    + 9 // unsigned integer with 8 bytes encoding
    + 2 // map of up to 255 entries
    + 2 // bytes of 28 bytes
    + 28
    + 1 // map of an entry
    + 2 // bytes of 32 bytes
    + 32
    + 9 // unsigned integer with 8 bytes encoding
;
const ASSUMED_SIZE_OF_ONE_WITNESS: usize
= 5 // array
    + 2 // tuple
    + 1 // tag
    + 2 + 32 // the public key revealed
    + 2 + 64 // the signature
;

const DEFAULT_MAX_TX_SIZE: usize = 16384;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum NetworkInfo {
    Testnet,
    Mainnet,
    Custom {
        protocol_magic: u32,
        network_id: u8,
        linear_fee_coefficient: String,
        linear_fee_constant: String,
        coins_per_utxo_word: String,
        pool_deposit: String,
        key_deposit: String,
        max_value_size: u32,
        max_tx_size: u32,
    },
}

impl NetworkInfo {
    pub fn network_info(&self) -> cardano_multiplatform_lib::address::NetworkInfo {
        match self {
            Self::Mainnet => cardano_multiplatform_lib::address::NetworkInfo::mainnet(),
            Self::Testnet => cardano_multiplatform_lib::address::NetworkInfo::testnet(),
            Self::Custom {
                network_id,
                protocol_magic,
                ..
            } => cardano_multiplatform_lib::address::NetworkInfo::new(*network_id, *protocol_magic),
        }
    }

    #[inline]
    pub fn max_tx_size(&self) -> usize {
        match self {
            Self::Mainnet => DEFAULT_MAX_TX_SIZE,
            Self::Testnet => DEFAULT_MAX_TX_SIZE,
            Self::Custom { max_tx_size, .. } => *max_tx_size as usize,
        }
    }

    /// get the assumed cost of an empty transaction
    ///
    /// This will be used as a base for our operation
    /// of custom input selections
    pub fn assumed_empty_transaction(&self) -> Coin {
        let fee = self.linear_fee();
        let assumed_size_input = BigNum::from_str(&ASSUMED_SIZE_EMPTY_TX.to_string()).unwrap();
        fee.coefficient()
            .checked_mul(&assumed_size_input)
            .unwrap()
            .checked_add(&fee.constant())
            .unwrap()
    }

    /// get the assumed cost of one input
    ///
    /// This will be used as a base for our operation
    /// of custom input selections
    pub fn assumed_cost_one_input(&self) -> Coin {
        let fee = self.linear_fee();
        let assumed_size = BigNum::from_str(&ASSUMED_SIZE_OF_ONE_INPUT.to_string()).unwrap();
        fee.coefficient().checked_mul(&assumed_size).unwrap()
    }

    /// get the assumed cost of one output, output with a native asset
    ///
    /// This will be used as a base for our operation
    /// of custom input selections
    pub fn assumed_cost_one_output(&self) -> Coin {
        let fee = self.linear_fee();
        let assumed_size = BigNum::from_str(&ASSUMED_SIZE_OF_ONE_OUTPUT.to_string()).unwrap();
        fee.coefficient().checked_mul(&assumed_size).unwrap()
    }

    /// get the assumed cost of one witness
    ///
    /// This will be used as a base for our operation
    /// of custom input selections
    pub fn assumed_cost_one_witness(&self) -> Coin {
        let fee = self.linear_fee();
        let assumed_size = BigNum::from_str(&ASSUMED_SIZE_OF_ONE_WITNESS.to_string()).unwrap();
        fee.coefficient().checked_mul(&assumed_size).unwrap()
    }

    #[inline]
    pub const fn estimated_size_empty(&self) -> usize {
        ASSUMED_SIZE_EMPTY_TX
    }

    #[inline]
    pub const fn estimated_size_input(&self) -> usize {
        ASSUMED_SIZE_OF_ONE_INPUT
    }

    #[inline]
    pub const fn estimated_size_output(&self) -> usize {
        ASSUMED_SIZE_OF_ONE_OUTPUT
    }

    pub fn estimate_size_overhead(&self, plan: &MultisigPlan) -> usize {
        let mut size = ASSUMED_OVERHEAD;

        // get the size of the multisig native script
        size += {
            let plan = plan.to_script();
            let mut set = TransactionWitnessSet::new();
            set.set_native_scripts(&plan);
            set.to_bytes().len()
        };

        // add the size of the witnesses
        size += plan.quorum as usize * ASSUMED_SIZE_OF_ONE_WITNESS;

        size
    }

    /// get the assumed cost of one native script for bridge (based on quorum size)
    ///
    /// This will be used as a base for our operation
    /// of custom input selections
    pub fn assumed_cost_native_script(&self, plan: &MultisigPlan) -> Coin {
        let plan = plan.to_script();
        let mut set = TransactionWitnessSet::new();
        set.set_native_scripts(&plan);
        let bytes = BigNum::from_str(&set.to_bytes().len().to_string()).unwrap();
        let fee = self.linear_fee();
        fee.coefficient().checked_mul(&bytes).unwrap()
    }

    pub fn assumed_cost_metadata_protocol_magic(&self, protocol_magic: impl AsRef<str>) -> Coin {
        let protocol_magic_size = protocol_magic.as_ref().as_bytes().len();

        let len = protocol_magic_size
            + 5 // add some CBOR overhead,
            // technically it is 4 that is needed but we add 1 extra
            // just to be sure
            // then we assume the ASSUMED_OVERHEAD will handle the remaining
            // cardano CBOR silliness so we don't have to think too hard
            // when doing transactions
            ;

        let bytes = BigNum::from_str(&len.to_string()).unwrap();
        let fee = self.linear_fee();
        fee.coefficient().checked_mul(&bytes).unwrap()
    }

    pub fn linear_fee(&self) -> LinearFee {
        match self {
            Self::Mainnet | Self::Testnet => {
                let coefficient = BigNum::from_str("44").unwrap();
                let constant = BigNum::from_str("155381").unwrap();
                LinearFee::new(&coefficient, &constant)
            }
            Self::Custom {
                linear_fee_coefficient,
                linear_fee_constant,
                ..
            } => {
                let coefficient = BigNum::from_str(linear_fee_coefficient).unwrap();
                let constant = BigNum::from_str(linear_fee_constant).unwrap();
                LinearFee::new(&coefficient, &constant)
            }
        }
    }

    pub fn transaction_builder(&self) -> TransactionBuilderConfig {
        let linear_fee = self.linear_fee();
        match self {
            Self::Mainnet | Self::Testnet => {
                let coins_per_utxo_word = BigNum::from_str("34482").unwrap();
                let pool_deposit = BigNum::from_str("500000000").unwrap();
                let key_deposit = BigNum::from_str("2000000").unwrap();
                let max_value_size = 5000;
                let max_tx_size = DEFAULT_MAX_TX_SIZE;

                #[allow(deprecated)]
                TransactionBuilderConfigBuilder::new()
                    .fee_algo(&linear_fee)
                    .coins_per_utxo_word(&coins_per_utxo_word)
                    .pool_deposit(&pool_deposit)
                    .key_deposit(&key_deposit)
                    .max_value_size(max_value_size)
                    .max_tx_size(max_tx_size as u32)
                    .build()
                    .unwrap()
            }
            Self::Custom {
                coins_per_utxo_word,
                pool_deposit,
                key_deposit,
                max_value_size,
                max_tx_size,
                ..
            } => {
                let coins_per_utxo_word = BigNum::from_str(coins_per_utxo_word).unwrap();
                let pool_deposit = BigNum::from_str(pool_deposit).unwrap();
                let key_deposit = BigNum::from_str(key_deposit).unwrap();
                let max_value_size = *max_value_size;
                let max_tx_size = *max_tx_size;

                #[allow(deprecated)]
                TransactionBuilderConfigBuilder::new()
                    .fee_algo(&linear_fee)
                    .coins_per_utxo_word(&coins_per_utxo_word)
                    .pool_deposit(&pool_deposit)
                    .key_deposit(&key_deposit)
                    .max_value_size(max_value_size)
                    .max_tx_size(max_tx_size)
                    .build()
                    .unwrap()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_multiplatform_lib::address::{BaseAddress, StakeCredential};
    use cardano_multiplatform_lib::crypto::{
        Ed25519KeyHash, Ed25519Signature, PublicKey, TransactionHash, Vkey, Vkeywitness,
        Vkeywitnesses,
    };
    use cardano_multiplatform_lib::ledger::alonzo::fees::min_no_script_fee;
    use cardano_multiplatform_lib::ledger::common::value::Value;
    use cardano_multiplatform_lib::metadata::{
        AuxiliaryData, GeneralTransactionMetadata, TransactionMetadatum,
    };
    use cardano_multiplatform_lib::{
        AssetName, MultiAsset, PolicyID, Transaction, TransactionBody, TransactionInput,
        TransactionInputs, TransactionOutput, TransactionOutputs,
    };

    fn empty_transaction_cost(linear_fee: &LinearFee) -> Coin {
        let inputs = TransactionInputs::new();
        let outputs = TransactionOutputs::new();
        let fee = Coin::from_str("800000").unwrap();
        let witness_set = TransactionWitnessSet::new();
        let body = TransactionBody::new(&inputs, &outputs, &fee, None);
        let tx = Transaction::new(&body, &witness_set, None);

        min_no_script_fee(&tx, linear_fee).unwrap()
    }

    fn protocol_magic_metadata_fee(linear_fee: &LinearFee, magic: &str) -> Coin {
        let auxiliary_data = {
            let mut auxiliary_data = AuxiliaryData::new();
            let mut metadata = GeneralTransactionMetadata::new();
            metadata.insert(
                &BigNum::from_str("87").unwrap(),
                &TransactionMetadatum::new_text(magic.to_string())
                    .map_err(|error| {
                        anyhow::anyhow!("Failed to encode the magic metadata: {}", error)
                    })
                    .unwrap(),
            );
            auxiliary_data.set_metadata(&metadata);
            auxiliary_data
        };
        let size = BigNum::from_str(&auxiliary_data.to_bytes().len().to_string()).unwrap();
        linear_fee.coefficient().checked_mul(&size).unwrap()
    }

    fn one_input_transaction_fee(linear_fee: &LinearFee) -> Coin {
        let inputs = {
            let mut inputs = TransactionInputs::new();

            let transaction_id = TransactionHash::from_bytes(vec![0; 32]).unwrap();
            let input = TransactionInput::new(&transaction_id, &BigNum::from(0));
            inputs.add(&input);

            inputs
        };
        let outputs = TransactionOutputs::new();
        let fee = Coin::from_str("800000").unwrap();
        let witness_set = TransactionWitnessSet::new();
        let body = TransactionBody::new(&inputs, &outputs, &fee, None);
        let tx = Transaction::new(&body, &witness_set, None);

        min_no_script_fee(&tx, linear_fee).unwrap()
    }

    fn two_inputs_transaction_fee(linear_fee: &LinearFee) -> Coin {
        let inputs = {
            let mut inputs = TransactionInputs::new();

            let transaction_id = TransactionHash::from_bytes(vec![0; 32]).unwrap();
            let input = TransactionInput::new(&transaction_id, &BigNum::from(0));
            inputs.add(&input);
            let transaction_id = TransactionHash::from_bytes(vec![1; 32]).unwrap();
            let input = TransactionInput::new(&transaction_id, &BigNum::from(200));
            inputs.add(&input);

            inputs
        };
        let outputs = TransactionOutputs::new();
        let fee = Coin::from_str("800000").unwrap();
        let witness_set = TransactionWitnessSet::new();
        let body = TransactionBody::new(&inputs, &outputs, &fee, None);
        let tx = Transaction::new(&body, &witness_set, None);

        min_no_script_fee(&tx, linear_fee).unwrap()
    }

    fn one_output_transaction_fee(network_id: u8, linear_fee: &LinearFee) -> Coin {
        let inputs = TransactionInputs::new();
        let outputs = {
            let mut outputs = TransactionOutputs::new();

            let payment =
                StakeCredential::from_keyhash(&Ed25519KeyHash::from_bytes(vec![0; 28]).unwrap());
            let stake =
                StakeCredential::from_keyhash(&Ed25519KeyHash::from_bytes(vec![1; 28]).unwrap());
            let address = BaseAddress::new(network_id, &payment, &stake).to_address();
            let amount = {
                let mut value = Value::new(&Coin::from_str("45000000000000000").unwrap());

                let mut multiasset = MultiAsset::new();
                let policy_id = PolicyID::from_bytes(vec![2; 28]).unwrap();
                let asset_name = AssetName::new(vec![3; 28]).unwrap();
                let quantity = Coin::from_str("45000000000000000").unwrap();
                multiasset.set_asset(&policy_id, &asset_name, &quantity);
                value.set_multiasset(&multiasset);

                value
            };
            let output = TransactionOutput::new(&address, &amount);
            outputs.add(&output);

            outputs
        };
        let fee = Coin::from_str("800000").unwrap();
        let witness_set = TransactionWitnessSet::new();
        let body = TransactionBody::new(&inputs, &outputs, &fee, None);
        let tx = Transaction::new(&body, &witness_set, None);

        min_no_script_fee(&tx, linear_fee).unwrap()
    }

    fn one_witness_transaction_fee(linear_fee: &LinearFee) -> Coin {
        let inputs = TransactionInputs::new();
        let outputs = TransactionOutputs::new();
        let fee = Coin::from_str("800000").unwrap();
        let witness_set = {
            let mut set = TransactionWitnessSet::new();
            let mut vkeys = Vkeywitnesses::new();
            let pk = PublicKey::from_bytes(&[0; 32]).unwrap();
            let vkey = Vkey::new(&pk);
            let signature = Ed25519Signature::from_bytes(vec![0; 64]).unwrap();
            let elem = Vkeywitness::new(&vkey, &signature);
            vkeys.add(&elem);
            set.set_vkeys(&vkeys);
            set
        };
        let body = TransactionBody::new(&inputs, &outputs, &fee, None);
        let tx = Transaction::new(&body, &witness_set, None);

        min_no_script_fee(&tx, linear_fee).unwrap()
    }

    fn test_assumed_cost(network_info: NetworkInfo) {
        let expected = network_info.assumed_empty_transaction();
        let min_empty_fee = empty_transaction_cost(&network_info.linear_fee());

        assert!(
            expected >= min_empty_fee,
            "expected fee {expected:?} is greater than the minimum {min_empty_fee:?}"
        );

        let min_one_fee = one_input_transaction_fee(&network_info.linear_fee());
        let one_fee = min_one_fee.clamped_sub(&min_empty_fee);
        let cost_one_input = network_info.assumed_cost_one_input();

        assert!(
            cost_one_input >= one_fee,
            "expected fee {cost_one_input:?} is greater than the minimum {one_fee:?}"
        );

        let min_two_fee = two_inputs_transaction_fee(&network_info.linear_fee());
        let two_fee = min_two_fee.clamped_sub(&min_empty_fee);
        let cost_two_inputs = network_info
            .assumed_cost_one_input()
            .checked_mul(&Coin::from_str("2").unwrap())
            .unwrap();

        assert!(
            cost_two_inputs >= two_fee,
            "expected fee {cost_two_inputs:?} is greater than the minimum {two_fee:?}"
        );

        let min_one_fee = one_output_transaction_fee(
            network_info.network_info().network_id(),
            &network_info.linear_fee(),
        );
        let one_fee = min_one_fee.clamped_sub(&min_empty_fee);
        let cost_one_output = network_info.assumed_cost_one_output();

        assert!(
            cost_one_output >= one_fee,
            "expected fee {cost_one_output:?} is greater than the minimum {one_fee:?}"
        );

        let min_one_fee = one_witness_transaction_fee(&network_info.linear_fee());
        let one_fee = min_one_fee.clamped_sub(&min_empty_fee);
        let cost_one_witness = network_info.assumed_cost_one_witness();

        assert!(
            cost_one_witness >= one_fee,
            "expected fee {cost_one_witness:?} is greater than the minimum {one_fee:?}"
        );

        let min_magic_fee = protocol_magic_metadata_fee(&network_info.linear_fee(), "magic");
        let cost_magic = network_info.assumed_cost_metadata_protocol_magic("magic");

        assert!(
            cost_magic > min_magic_fee,
            "Expected magic fee {cost_magic:?} to be greater than the minimum {min_magic_fee:?}"
        )
    }

    #[test]
    fn mainnet_assumed_costs() {
        test_assumed_cost(NetworkInfo::Mainnet);
    }

    #[test]
    fn testnet_assumed_costs() {
        test_assumed_cost(NetworkInfo::Testnet);
    }

    #[test]
    fn mainnet_protocol_magic_metadata_costs() {
        let network_info = NetworkInfo::Mainnet;

        assert_eq!(
            protocol_magic_metadata_fee(&network_info.linear_fee(), ""),
            BigNum::from_str("176").unwrap(),
        );
        assert_eq!(
            protocol_magic_metadata_fee(&network_info.linear_fee(), "mainnet.cardano-evm.c1"),
            BigNum::from_str("1144").unwrap(),
        );
    }

    #[test]
    fn testnet_protocol_magic_metadata_costs() {
        let network_info = NetworkInfo::Testnet;

        assert_eq!(
            protocol_magic_metadata_fee(&network_info.linear_fee(), ""),
            BigNum::from_str("176").unwrap(),
        );
        assert_eq!(
            protocol_magic_metadata_fee(&network_info.linear_fee(), "localnet.cardano-evm.c1"),
            BigNum::from_str("1188").unwrap(),
        );
        assert_eq!(
            protocol_magic_metadata_fee(&network_info.linear_fee(), "internal.cardano-evm.c1"),
            BigNum::from_str("1188").unwrap(),
        );
        assert_eq!(
            protocol_magic_metadata_fee(&network_info.linear_fee(), "devnet.cardano-evm.c1"),
            BigNum::from_str("1100").unwrap(),
        );
    }
    // ************************************************************************
    // the following tests are to check that the initial assumptions are not  *
    // changing overtime. So if we change the fee on mainnet/testnet we will  *
    // need to change these values too in order for the tests to pass.        *
    // ************************************************************************

    #[test]
    fn mainnet_empty_transaction_cost() {
        let fee = empty_transaction_cost(&NetworkInfo::Mainnet.linear_fee());
        const EXPECTED: &str = "156041";
        assert_eq!(EXPECTED, fee.to_str())
    }

    #[test]
    fn mainnet_one_input_transaction_cost() {
        let fee = one_input_transaction_fee(&NetworkInfo::Mainnet.linear_fee());
        const EXPECTED: &str = "157625";
        assert_eq!(EXPECTED, fee.to_str());
    }

    #[test]
    fn mainnet_two_inputs_transaction_cost() {
        let fee = two_inputs_transaction_fee(&NetworkInfo::Mainnet.linear_fee());
        const EXPECTED: &str = "159253";
        assert_eq!(EXPECTED, fee.to_str())
    }

    #[test]
    fn mainnet_one_output_transaction_cost() {
        let network_info = NetworkInfo::Mainnet;
        let fee = one_output_transaction_fee(
            network_info.network_info().network_id(),
            &network_info.linear_fee(),
        );
        const EXPECTED: &str = "162245";
        assert_eq!(EXPECTED, fee.to_str());
    }

    #[test]
    fn mainnet_one_witness_transaction_cost() {
        let network_info = NetworkInfo::Mainnet;
        let fee = one_witness_transaction_fee(&network_info.linear_fee());
        const EXPECTED: &str = "160573";
        assert_eq!(EXPECTED, fee.to_str());
    }

    #[test]
    fn testnet_empty_transaction_cost() {
        let fee = empty_transaction_cost(&NetworkInfo::Testnet.linear_fee());
        const EXPECTED: &str = "156041";
        assert_eq!(EXPECTED, fee.to_str())
    }

    #[test]
    fn testnet_one_input_transaction_cost() {
        let fee = one_input_transaction_fee(&NetworkInfo::Testnet.linear_fee());
        const EXPECTED: &str = "157625";
        assert_eq!(EXPECTED, fee.to_str());
    }

    #[test]
    fn testnet_two_inputs_transaction_cost() {
        let fee = two_inputs_transaction_fee(&NetworkInfo::Testnet.linear_fee());
        const EXPECTED: &str = "159253";
        assert_eq!(EXPECTED, fee.to_str())
    }

    #[test]
    fn testnet_one_output_transaction_cost() {
        let network_info = NetworkInfo::Testnet;
        let fee = one_output_transaction_fee(
            network_info.network_info().network_id(),
            &network_info.linear_fee(),
        );
        const EXPECTED: &str = "162245";
        assert_eq!(EXPECTED, fee.to_str());
    }

    #[test]
    fn testnet_one_witness_transaction_cost() {
        let network_info = NetworkInfo::Testnet;
        let fee = one_witness_transaction_fee(&network_info.linear_fee());
        const EXPECTED: &str = "160573";
        assert_eq!(EXPECTED, fee.to_str());
    }
}
