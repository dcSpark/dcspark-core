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
        let mut appender =
            Appender::new(path.clone(), None, writable).map(|inner| Self { inner })?;
        let (_, last_len) = match appender.last()? {
            None => return Ok(appender),
            Some(some) => some,
        };

        if last_len == 0 {
            // the storage wasn't shrink to fit and we need to find where the index ends
            let actual_len = appender.find_actual_end()?;
            appender = Appender::new(path, Some(2 * Self::SIZE_OF_USIZE * actual_len), writable)
                .map(|inner| Self { inner })?;
        }

        Ok(appender)
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

    pub fn get_length_at(&self, at: usize) -> Result<usize, FraosError> {
        Ok(self
            .get_offset_and_length(at)?
            .ok_or(FraosError::IndexFileDamaged)?
            .1)
    }

    #[allow(unused)]
    pub fn get_offset_at(&self, at: usize) -> Result<usize, FraosError> {
        Ok(self
            .get_offset_and_length(at)?
            .ok_or(FraosError::IndexFileDamaged)?
            .0)
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

    // The seqno index contains pairs (offset, length), offsets grow monotonically, lengths are always non-zero
    // If the storage is still open or the storage wasn't shrink to fit properly while dropped it might have tailing zeros
    // This way to find out what is the actual storage size and what is indexed we need to find the actual end if seqno is not empty.
    // We utilize binary search to find last non zero value par. This is the actual end
    pub(crate) fn find_actual_end(&self) -> Result<usize, FraosError> {
        let mut start = 0;
        let len = self.len();
        let mut end = self.len();

        // empty index was created or index is empty
        if self.get_length_at(start)? == 0 || end == 0 {
            return Ok(0);
        }

        // all elements are non-zero
        if self.get_length_at(end.saturating_sub(1))? != 0 {
            return Ok(end);
        }

        // if index is empty we checked already
        while start < len.saturating_sub(1) {
            // we checked before that we have at least one zero and it is ok to access start + 1
            if self.get_length_at(start)? != 0 && self.get_length_at(start + 1)? == 0 {
                return Ok(start + 1);
            }
            let mid = (start + end) / 2;
            if self.get_length_at(mid)? == 0 {
                end = mid;
            } else {
                start = mid;
            }
        }

        Err(FraosError::IndexFileDamaged)
    }
}

#[cfg(test)]
mod tests {
    use super::SeqNoIndex;

    use crate::{FileError, FraosError, MmapError};
    use memmap2::{MmapMut, MmapOptions};
    use std::fs::{File, OpenOptions};
    use std::mem::size_of;
    use std::path::PathBuf;

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

    fn get_file(path: PathBuf, writable: bool) -> Result<File, FraosError> {
        let mut options = OpenOptions::new();
        options.read(true);
        if writable {
            options.write(true).create(true);
        };

        options
            .open(&path)
            .map_err(|err| FraosError::FileError(FileError::FileOpen(path.clone(), err)))
    }

    fn allocate_mmap(file: &File, size: usize) -> Result<MmapMut, FraosError> {
        // that fills the file with zeros
        file.set_len(size as u64)
            .map_err(|err| FraosError::FileError(FileError::Extend(err)))?;
        unsafe { MmapOptions::new().len(size).offset(0u64).map_mut(file) }
            .map_err(|err| FraosError::MmapError(MmapError::Mmap(err)))
    }

    #[test]
    fn check_index_recovery_zero_length() {
        for i in 0..20 {
            let tmp = tempfile::NamedTempFile::new().unwrap();

            let file = get_file(tmp.path().to_path_buf(), true).unwrap();

            if i != 0 {
                let mmap = allocate_mmap(&file, size_of::<usize>() * i).unwrap();
                mmap.flush().unwrap();
            }

            let index = SeqNoIndex::new(Some(tmp.path().to_path_buf()), true);
            assert!(index.is_ok(), "can't create seqno index with {} usizes", i);
            let index = index.unwrap();
            assert!(index.is_empty());

            index.append(&[(5000, 5), (5005, 6)]).unwrap();
            drop(index);

            let index = SeqNoIndex::new(Some(tmp.path().to_path_buf()), true);
            assert!(
                index.is_ok(),
                "can't create seqno index with {} usizes after append",
                i,
            );
            let index = index.unwrap();
            assert_eq!(
                index.len(),
                2,
                "seqno index should have len 2 after append at {}",
                i
            );
        }
    }

    #[test]
    fn check_index_recovery_non_zero_length() {
        for (non_zeros, zeros) in [(2, 0), (100, 0), (2, 1), (2, 5), (2, 10), (258, 400)] {
            let tmp = tempfile::NamedTempFile::new().unwrap();

            let file = get_file(tmp.path().to_path_buf(), true).unwrap();

            let mut mmap = allocate_mmap(&file, size_of::<usize>() * (non_zeros + zeros)).unwrap();
            for i in 0..non_zeros {
                mmap.as_mut()[i * size_of::<usize>()..(i + 1) * size_of::<usize>()]
                    .copy_from_slice(&i.to_le_bytes()[..]);
            }
            mmap.flush().unwrap();

            let index = SeqNoIndex::new(Some(tmp.path().to_path_buf()), true);
            assert!(
                index.is_ok(),
                "can't create seqno index with {} non zeros and {} zeros",
                non_zeros,
                zeros
            );
            let index = index.unwrap();
            assert_eq!(
                index.len(),
                non_zeros / 2,
                "seqno index with {} non zeros and {} zeros should have len {}",
                non_zeros,
                zeros,
                non_zeros / 2
            );

            index.append(&[(5000, 5), (5005, 6)]).unwrap();
            drop(index);

            let index = SeqNoIndex::new(Some(tmp.path().to_path_buf()), true);
            assert!(
                index.is_ok(),
                "can't create seqno index with {} non zeros and {} zeros after append",
                non_zeros,
                zeros
            );
            let index = index.unwrap();
            assert_eq!(
                index.len(),
                non_zeros / 2 + 2,
                "seqno index with {} non zeros and {} zeros should have len {} after append",
                non_zeros,
                zeros,
                non_zeros / 2 + 2
            );
        }
    }
}
