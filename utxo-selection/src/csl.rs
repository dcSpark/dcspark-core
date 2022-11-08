use crate::UTxOBuilder;
use cardano_multiplatform_lib::TransactionOutput;

pub struct CslTransactionOutput {
    pub inner: TransactionOutput,
}

impl From<TransactionOutput> for CslTransactionOutput {
    fn from(_: TransactionOutput) -> Self {
        todo!()
    }
}
