use crate::error::FraosError;
use crate::{flatfile::FlatFile, seqno::SeqNoIndex};
use std::sync::Arc;

/// This structure allows to iterate over records in the order they were added
/// to this database.
pub struct SeqNoIter {
    data: Arc<FlatFile>,
    index: Arc<SeqNoIndex>,
    seqno: usize,
}

impl SeqNoIter {
    pub(crate) fn new(data: Arc<FlatFile>, index: Arc<SeqNoIndex>, seqno: usize) -> Self {
        Self { data, index, seqno }
    }

    pub fn next_impl(&mut self) -> Result<Option<Vec<u8>>, FraosError> {
        let (offset, length) = match self.index.get_offset_and_length(self.seqno)? {
            None => return Ok(None),
            Some((offset, length)) => (offset, length),
        };

        let item = self.data.get_record_at_offset(offset, length)?;
        self.seqno += 1;
        Ok(item)
    }
}

impl Iterator for SeqNoIter {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_impl().unwrap()
    }
}
