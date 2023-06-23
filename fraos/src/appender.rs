//! Appenders are mmap'ed files intended for append-only use.

use crate::error::{FileError, FraosError, MmapError};
use crate::growable_mmap::GrowableMmap;
use std::{
    cell::UnsafeCell,
    fs::OpenOptions,
    marker::Sync,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

pub(crate) struct Appender {
    // This is used to trick the compiler so that we have parallel reads and
    // writes.
    mmap: UnsafeCell<GrowableMmap>,
    // Atomic is used to ensure that we can have lock-free and memory-safe
    // reads. Since this value is updated only after the write has finished it
    // is safe to use it as the upper boundary for reads.
    actual_size: AtomicUsize,
}

impl Appender {
    /// Open a flatfile.
    ///
    /// # Arguments
    ///
    /// * `path` - the path to the file. It will be created if not exists.
    /// * `writable` - flag that indicates whether the storage is read-only
    pub fn new(
        path: Option<PathBuf>,
        existing_length: Option<usize>,
        writable: bool,
    ) -> Result<Self, FraosError> {
        let file = if let Some(path) = path {
            let mut options = OpenOptions::new();
            options.read(true);
            if writable {
                options.write(true).create(true);
            };
            let file = options
                .open(&path)
                .map_err(|err| FraosError::FileError(FileError::FileOpen(path.clone(), err)))?;

            Some(file)
        } else {
            None
        };

        let mmap = UnsafeCell::new(GrowableMmap::new(file, existing_length)?);
        let actual_size = AtomicUsize::from(
            unsafe { mmap.get().as_ref() }
                .ok_or(FraosError::MmapError(MmapError::Access))?
                .len()?,
        );

        Ok(Self { mmap, actual_size })
    }

    /// Append data to the file. The mutable pointer to the new data location is
    /// given to `f` which should write the data. This function will block if
    /// another write is in progress.
    pub fn append<F>(&self, size_inc: usize, f: F) -> Result<(), FraosError>
    where
        F: Fn(&mut [u8]) -> Result<(), FraosError>,
    {
        if size_inc == 0 {
            return Ok(());
        }

        let mmap =
            unsafe { self.mmap.get().as_mut() }.ok_or(FraosError::MmapError(MmapError::Access))?;
        let actual_size = self.actual_size.load(Ordering::SeqCst);

        let new_file_size = actual_size + size_inc;

        mmap.grow_and_apply(size_inc, f)?;
        self.actual_size.store(new_file_size, Ordering::SeqCst);

        Ok(())
    }

    /// The whole data buffer is given to `f` which should return the data back
    /// or return None if something went wrong.
    pub fn get_data<F, U>(&self, offset: usize, f: F) -> Result<Option<U>, FraosError>
    where
        F: Fn(&[u8]) -> Result<Option<U>, FraosError>,
    {
        let mmap =
            unsafe { self.mmap.get().as_ref() }.ok_or(FraosError::MmapError(MmapError::Access))?;
        mmap.get_ref_and_apply(offset, f)
    }

    pub fn memory_size(&self) -> usize {
        self.actual_size.load(Ordering::SeqCst)
    }

    pub fn shrink_to_size(&self) -> Result<(), FraosError> {
        let mmap =
            unsafe { self.mmap.get().as_ref() }.ok_or(FraosError::MmapError(MmapError::Access))?;

        mmap.shrink_to_size()
    }

    #[allow(unused)]
    pub(crate) fn mmaps_count(&self) -> Result<usize, FraosError> {
        let mmap =
            unsafe { self.mmap.get().as_ref() }.ok_or(FraosError::MmapError(MmapError::Access))?;

        mmap.mmaps_count()
    }
}

unsafe impl Sync for Appender {}
