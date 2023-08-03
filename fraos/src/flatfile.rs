use crate::appender::Appender;
use crate::error::{FraosError, MmapError};
use std::{io::Write, path::PathBuf};

/// Flatfiles are the main database files that hold all keys and data.
///
/// Records are stored without any additional spaces. The file does not hold any
/// additional data.
///
/// A flatfile is opened with `mmap` and we rely on OS's mechanisms for caching
/// pages, etc.
pub(crate) struct FlatFile {
    inner: Appender,
}

/// Low-level interface to flatfiles.
impl FlatFile {
    /// Open a flatfile.
    ///
    /// # Arguments
    ///
    /// * `path` - the path to the file. It will be created if not exists.
    pub fn new(
        path: Option<PathBuf>,
        existing_length: usize,
        writable: bool,
    ) -> Result<Self, FraosError> {
        Appender::new(path, Some(existing_length), writable).map(|inner| FlatFile { inner })
    }

    /// Write an array of records to the drive. This function will block if
    /// another write is still in progress.
    pub fn append(&self, records: &[&[u8]]) -> Result<(), FraosError> {
        if records.is_empty() {
            return Ok(());
        }

        if records.iter().any(|record| record.is_empty()) {
            return Err(FraosError::EmptyRecordAppended);
        }

        let size_inc: usize = records.iter().map(|record| record.len()).sum();

        self.inner.append(size_inc, move |mut mmap| {
            for record in records {
                mmap.write_all(record)
                    .map_err(|err| FraosError::MmapError(MmapError::MmapWrite(err)))?;
            }

            Ok(())
        })
    }

    /// Get the value at the given `offset`. If the `offset` is outside of the
    /// file boundaries, `None` is returned. Upon a successul read a key-value
    /// record is returned. Note that this function do not check if the given
    /// `offset` is the start of an actual record, so you should be careful when
    /// using it.
    pub fn get_record_at_offset(
        &self,
        offset: usize,
        length: usize,
    ) -> Result<Option<Vec<u8>>, FraosError> {
        self.inner.get_data(offset, move |mmap| {
            if mmap.len() < length {
                return Err(FraosError::MmapError(MmapError::DataLength {
                    actual: mmap.len(),
                    requested: length,
                }));
            }

            Ok(Some(mmap[..length].to_vec()))
        })
    }

    pub fn memory_size(&self) -> usize {
        self.inner.memory_size()
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
    use super::FlatFile;

    #[quickcheck]
    fn test_read_write(records: Vec<Vec<u8>>) {
        if records.is_empty() {
            return;
        }

        let tmp = tempfile::NamedTempFile::new().unwrap();

        let raw_records: Vec<_> = records
            .iter()
            .filter(|x| !x.is_empty())
            .map(|x| x.as_ref())
            .collect();

        let flatfile = FlatFile::new(Some(tmp.path().to_path_buf()), 0, true).unwrap();
        flatfile.append(&raw_records).unwrap();

        let mut offset = 0;
        for record in raw_records.iter() {
            let drive_record = flatfile
                .get_record_at_offset(offset, record.len())
                .unwrap()
                .unwrap();
            assert_eq!(*record, drive_record.as_slice());
            offset += drive_record.len();
        }
    }
}
