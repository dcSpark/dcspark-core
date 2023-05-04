use cardano_multiplatform_lib::builders::witness_builder::{
    NativeScriptWitnessInfo, PartialPlutusWitness,
};
use cardano_multiplatform_lib::plutus::PlutusData;
use cardano_multiplatform_lib::{NativeScript, RequiredSigners};

#[derive(Debug, Clone)]
pub enum CardanoPaymentCredentials {
    PlutusScript {
        partial_witness: PartialPlutusWitness,
        required_signers: RequiredSigners,
        datum: PlutusData,
    },
    PaymentKey,
    NativeScript {
        native_script: NativeScript,
        witness_info: NativeScriptWitnessInfo,
    },
}
