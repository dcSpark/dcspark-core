use crate::appender::Appender;
use crate::error::{FraosError, MmapError};
use crate::FraosError::EmptyRecordAppended;
use std::{mem::size_of, path::PathBuf};

/// Index from the sequential number of a record to its location in a flatfile.
pub(crate) struct SeqNoIndex {
    inner: Appender,
}

impl SeqNoIndex {
    const SIZE_OF_USIZE: usize = size_of::<usize>();

    /// Open an index.
    ///
    /// # Arguments
    ///
    /// * `path` - the path to the file. It will be created if not exists.
    pub fn new(path: Option<PathBuf>, writable: bool) -> Result<Self, FraosError> {
        Appender::new(path, None, writable).map(|inner| Self { inner })
    }

    /// Add records to index. This function will block if another write is still
    /// in progress.
    #[allow(clippy::manual_slice_size_calculation)]
    pub fn append(&self, records: &[(usize, usize)]) -> Result<Option<usize>, FraosError> {
        if records.is_empty() {
            return Ok(None);
        }

        if records.iter().any(|(_offset, len)| *len == 0) {
            return Err(EmptyRecordAppended);
        }

        let size_inc: usize = Self::SIZE_OF_USIZE * 2 * records.len();
        let current_seqno = self.len();

        self.inner.append(size_inc, move |mut mmap| {
            if mmap.len() < size_inc {
                return Err(FraosError::MmapError(MmapError::DataLength {
                    actual: mmap.len(),
                    requested: size_inc,
                }));
            }
            for (offset, length) in records {
                mmap[..Self::SIZE_OF_USIZE].copy_from_slice(&offset.to_le_bytes()[..]);
                mmap[Self::SIZE_OF_USIZE..Self::SIZE_OF_USIZE * 2]
                    .copy_from_slice(&length.to_le_bytes()[..]);
                mmap = &mut mmap[Self::SIZE_OF_USIZE * 2..];
            }
            Ok(())
        })?;

        Ok(Some(current_seqno))
    }

    /// Get the location of a record with the given number.
    pub fn get_offset_and_length(
        &self,
        seqno: usize,
    ) -> Result<Option<(usize, usize)>, FraosError> {
        let offset = seqno * Self::SIZE_OF_USIZE * 2;

        match self.inner.get_data(offset, |mmap| {
            let mut offset_buffer = [0u8; Self::SIZE_OF_USIZE];
            let mut length_buffer = [0u8; Self::SIZE_OF_USIZE];
            offset_buffer.copy_from_slice(&mmap[..Self::SIZE_OF_USIZE]);
            length_buffer.copy_from_slice(&mmap[Self::SIZE_OF_USIZE..Self::SIZE_OF_USIZE * 2]);

            Ok(Some((
                usize::from_le_bytes(offset_buffer),
                usize::from_le_bytes(length_buffer),
            )))
        })? {
            None => Ok(None),
            Some(results) => Ok(Some(results)),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.memory_size() / Self::SIZE_OF_USIZE / 2
    }

    #[allow(unused)]
    pub fn memory_size(&self) -> usize {
        self.inner.memory_size()
    }

    pub fn is_correct(&self) -> Result<(), FraosError> {
        if self.inner.memory_size() % (Self::SIZE_OF_USIZE * 2) != 0 {
            return Err(FraosError::IndexFileDamaged);
        }

        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn last(&self) -> Result<Option<(usize, usize)>, FraosError> {
        if self.is_empty() {
            return Ok(None);
        }

        self.get_offset_and_length(self.len().saturating_sub(1))
    }

    pub fn shrink_to_size(&self) -> Result<(), FraosError> {
        self.inner.shrink_to_size()
    }

    #[allow(unused)]
    pub(crate) fn mmaps_count(&self) -> Result<usize, FraosError> {
        self.inner.mmaps_count()
    }
}

#[cfg(test)]
mod tests {
    use super::SeqNoIndex;
    use crate::FraosError;

    #[quickcheck]
    fn test_read_write(records: Vec<(usize, usize)>) {
        if records.is_empty() || records.iter().any(|(l, r)| *l == 0 || *r == 0) {
            return;
        }

        let tmp = tempfile::NamedTempFile::new().unwrap();

        let index = SeqNoIndex::new(Some(tmp.path().to_path_buf()), true);
        assert!(index.is_ok());
        let index = index.unwrap();

        index.append(&records).unwrap();
        assert_eq!(records.len(), index.len());

        for (i, record) in records.iter().enumerate() {
            let drive_record = index.get_offset_and_length(i);
            assert!(drive_record.is_ok());
            let drive_record = drive_record.unwrap().unwrap();
            assert_eq!(*record, drive_record);
        }
    }

    #[quickcheck]
    fn test_seq_number(records: Vec<(usize, usize)>) {
        if records.iter().any(|(_, r)| *r == 0) {
            return;
        }

        let tmp = tempfile::NamedTempFile::new().unwrap();

        let index = SeqNoIndex::new(Some(tmp.path().to_path_buf()), true).unwrap();
        let checks_count = 100usize;
        for i in 0..checks_count {
            let result = index.append(&records).unwrap();
            if !records.is_empty() {
                assert_eq!(result.unwrap(), i * records.len());
            } else {
                assert!(result.is_none());
            }
            let result = index.get_offset_and_length((i + 1) * records.len());
            assert!(result.is_ok());
            assert!(result.unwrap().is_none());
            if !records.is_empty() {
                let record = index.get_offset_and_length(i * records.len() + records.len() - 1);
                assert!(record.is_ok());
                assert!(record.unwrap().is_some());
            }
        }
    }

    #[test]
    fn check_empty_cases() {
        let tmp = tempfile::NamedTempFile::new().unwrap();

        let index = SeqNoIndex::new(Some(tmp.path().to_path_buf()), true).unwrap();

        let records = vec![(0, 5), (5, 0), (5, 10)];
        let result = index.append(&records);
        assert!(result.is_err());
        assert!(matches!(
            result.err().unwrap(),
            FraosError::EmptyRecordAppended
        ));
    }
}
