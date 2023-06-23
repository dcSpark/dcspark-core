use crate::error::{FileError, FraosError, MmapError};
use crate::index_on_mmaps::{IndexDescriptor, IndexOnMmaps, MmapChunkAddressMapper};
use crate::shared_mmap::SharedMmap;
use memmap2::{MmapMut, MmapOptions};
use std::cmp::max;

use std::fs::File;
use std::mem::swap;
use std::sync::{RwLock, RwLockWriteGuard};

const MAX_MMAPS_COUNT: usize = 2048;
const MIN_MMAP_BYTES: usize = 4096 * 128;

/// Head and body of file
struct InactiveMmaps {
    index: IndexOnMmaps,
    maps: Vec<SharedMmap>,
}

/// End of file mmap (tail)
struct ActiveMmap {
    len: usize,
    mmap: MmapMut,
    bounds: MmapChunkAddressMapper,
}

struct Storage {
    inactive_mmaps: InactiveMmaps,
    active_map: Option<ActiveMmap>,
}

/// the struct has an active mutable mmap and inactive tail
/// if we have enough space we add records to the active mmap
/// if not we slice the active mmap to the actual end of writes and put it to inactive mmaps
/// then we create a new mmap with 2x size from previous
/// if 2x is not enough we create an mmap with size of the data
///
pub(crate) struct GrowableMmap {
    storage: RwLock<Storage>,
    file: Option<File>,
}

impl GrowableMmap {
    pub fn new(file: Option<File>, existing_length: Option<usize>) -> Result<Self, FraosError> {
        let mut index = IndexOnMmaps::new();
        let mut maps = vec![];

        if let Some(file) = &file {
            let file_length = file
                .metadata()
                .map_err(|err| FraosError::FileError(FileError::Metadata(err)))?
                .len() as usize;

            if let Some(existing_length) = existing_length {
                if existing_length > file_length {
                    return Err(FraosError::DataFileDamaged);
                }
            }

            if file_length > 0 {
                let upper_cap = existing_length.unwrap_or(file_length);
                let mmap = SharedMmap::new(
                    unsafe { MmapOptions::new().offset(0).len(upper_cap).map(file) }
                        .map_err(|err| FraosError::MmapError(MmapError::Mmap(err)))?,
                );

                let mut single_mmap_index = MmapChunkAddressMapper::new(0usize);
                single_mmap_index
                    .append_relative_end(mmap.len())
                    .map_err(FraosError::IndexError)?;
                index
                    .append(single_mmap_index)
                    .map_err(FraosError::IndexError)?;
                maps.push(mmap);
            }
        }

        let growable_mmap = GrowableMmap {
            storage: RwLock::new(Storage {
                inactive_mmaps: InactiveMmaps { index, maps },
                active_map: None,
            }),
            file,
        };

        Ok(growable_mmap)
    }

    pub fn len(&self) -> Result<usize, FraosError> {
        let storage_guard = self.storage.read().map_err(|err| -> FraosError {
            FraosError::StorageLock {
                description: err.to_string(),
            }
        })?;
        match &storage_guard.active_map {
            None => Ok(storage_guard.inactive_mmaps.index.len()),
            Some(mmap) => Ok(mmap.bounds.global_chunk_end()),
        }
    }

    pub fn grow_and_apply<F>(&self, extension: usize, f: F) -> Result<(), FraosError>
    where
        F: Fn(&mut [u8]) -> Result<(), FraosError>,
    {
        if extension == 0 {
            return Err(FraosError::StorageZeroExtension);
        }

        let mut storage_guard = self.storage.write().map_err(|err| -> FraosError {
            FraosError::StorageLock {
                description: err.to_string(),
            }
        })?;

        let start_write_from = match &mut storage_guard.active_map {
            None => {
                let new_mmap_size = self.get_new_mmap_size(extension);
                // inactive size
                let already_mapped = storage_guard.inactive_mmaps.index.len();

                // create mmap and flush
                let new_mmap = self.create_mmap(new_mmap_size, already_mapped)?;

                // create index on active mmap
                let mut single_mmap_index = MmapChunkAddressMapper::new(already_mapped);
                single_mmap_index
                    .append_relative_end(extension)
                    .map_err(FraosError::IndexError)?;

                storage_guard.active_map = Some(ActiveMmap {
                    len: new_mmap_size,
                    mmap: new_mmap,
                    bounds: single_mmap_index,
                });

                0usize
            }
            Some(active_mmap) => {
                let current_mmap_end = active_mmap.bounds.size();

                // if we have enough space use active mmap
                if current_mmap_end + extension < active_mmap.len {
                    active_mmap
                        .bounds
                        .append_relative_end(current_mmap_end + extension)
                        .map_err(FraosError::IndexError)?;
                    current_mmap_end
                } else {
                    let new_mmap_size = self.get_new_mmap_size(extension);
                    // offset is inactive part + current active part
                    let already_mapped = active_mmap.bounds.global_chunk_end();

                    let mut new_mmap = self.create_mmap(new_mmap_size, already_mapped)?;

                    // replace active mmap with new mmap
                    swap(&mut new_mmap, &mut active_mmap.mmap);
                    active_mmap.len = new_mmap_size;

                    let mut new_bounds = MmapChunkAddressMapper::new(already_mapped);
                    new_bounds
                        .append_relative_end(extension)
                        .map_err(FraosError::IndexError)?;
                    swap(&mut new_bounds, &mut active_mmap.bounds);

                    // add old replaced active mmap to inactive mmaps
                    storage_guard
                        .inactive_mmaps
                        .index
                        .append(new_bounds)
                        .map_err(FraosError::IndexError)?;
                    storage_guard.inactive_mmaps.maps.push(
                        SharedMmap::new(
                            new_mmap
                                .make_read_only()
                                .map_err(|err| FraosError::MmapError(MmapError::Protect(err)))?,
                        )
                        .slice(..current_mmap_end),
                    );

                    0usize
                }
            }
        };

        match storage_guard.active_map.as_mut() {
            None => return Err(FraosError::DataFileDamaged),
            Some(active_mmap) => {
                f(&mut active_mmap.mmap.as_mut()[start_write_from..])?;

                active_mmap
                    .mmap
                    .flush()
                    .map_err(|err| FraosError::MmapError(MmapError::Flush(err)))?;
            }
        }

        self.rearrange_mmaps(storage_guard)
    }

    pub fn get_ref_and_apply<F, U>(&self, address: usize, f: F) -> Result<Option<U>, FraosError>
    where
        F: Fn(&[u8]) -> Result<Option<U>, FraosError>,
    {
        let storage_guard = self.storage.read().map_err(|err| FraosError::StorageLock {
            description: err.to_string(),
        })?;

        if address < storage_guard.inactive_mmaps.index.len() {
            let IndexDescriptor {
                mmap_number,
                mmap_offset,
                len,
            } = match storage_guard
                .inactive_mmaps
                .index
                .find(address)
                .map_err(FraosError::IndexError)?
            {
                None => return Ok(None),
                Some(index) => index,
            };

            return f(storage_guard.inactive_mmaps.maps[mmap_number]
                .slice(mmap_offset..mmap_offset + len)
                .as_ref());
        }

        match storage_guard.active_map.as_ref() {
            None => Ok(None),
            Some(active_mmap) => {
                let IndexDescriptor {
                    mmap_number: _mmap_number,
                    mmap_offset,
                    len,
                } = match active_mmap
                    .bounds
                    .find(address)
                    .map_err(FraosError::IndexError)?
                {
                    None => return Ok(None),
                    Some(index) => index,
                };

                f(&active_mmap.mmap.as_ref()[mmap_offset..mmap_offset + len])
            }
        }
    }

    pub fn shrink_to_size(&self) -> Result<(), FraosError> {
        let storage_guard = self
            .storage
            .write()
            .map_err(|err| FraosError::StorageLock {
                description: err.to_string(),
            })?;

        match &self.file {
            None => Ok(()),
            Some(file) => file
                .set_len(
                    storage_guard
                        .active_map
                        .as_ref()
                        .map(|mmap| mmap.bounds.global_chunk_end())
                        .unwrap_or(storage_guard.inactive_mmaps.index.len())
                        as u64,
                )
                .map_err(|err| FraosError::FileError(FileError::Shrink(err))),
        }
    }

    fn rearrange_mmaps(
        &self,
        mut storage_guard: RwLockWriteGuard<Storage>,
    ) -> Result<(), FraosError> {
        if storage_guard.inactive_mmaps.maps.len() <= MAX_MMAPS_COUNT {
            return Ok(());
        }

        let file = match &self.file {
            None => return Ok(()),
            Some(file) => file,
        };

        let mmap = SharedMmap::new(
            unsafe {
                MmapOptions::new()
                    .offset(0)
                    .len(storage_guard.inactive_mmaps.index.len())
                    .map(file)
            }
            .map_err(|err| FraosError::MmapError(MmapError::Mmap(err)))?,
        );

        let mut single_mmap_index = MmapChunkAddressMapper::new(0usize);
        single_mmap_index
            .append_relative_end(mmap.len())
            .map_err(FraosError::IndexError)?;
        let mut index = IndexOnMmaps::new();
        index
            .append(single_mmap_index)
            .map_err(FraosError::IndexError)?;
        storage_guard.inactive_mmaps = InactiveMmaps {
            index,
            maps: vec![mmap],
        };

        Ok(())
    }

    #[allow(unused)]
    pub(crate) fn mmaps_count(&self) -> Result<usize, FraosError> {
        let storage_guard = self.storage.read().map_err(|err| FraosError::StorageLock {
            description: err.to_string(),
        })?;

        Ok(storage_guard.inactive_mmaps.maps.len()
            + storage_guard.active_map.as_ref().map(|_| 1).unwrap_or(0))
    }

    fn get_new_mmap_size(&self, add: usize) -> usize {
        match self.file {
            None => add,
            Some(_) => max(add, MIN_MMAP_BYTES),
        }
    }

    fn create_mmap(&self, new_mmap_size: usize, offset: usize) -> Result<MmapMut, FraosError> {
        if let Some(file) = &self.file {
            file.set_len((offset + new_mmap_size) as u64)
                .map_err(|err| FraosError::FileError(FileError::Extend(err)))?;
            unsafe {
                MmapOptions::new()
                    .len(new_mmap_size)
                    .offset(offset as u64)
                    .map_mut(file)
            }
            .map_err(|err| FraosError::MmapError(MmapError::Mmap(err)))
        } else {
            MmapOptions::new()
                .len(new_mmap_size)
                .map_anon()
                .map_err(|err| FraosError::MmapError(MmapError::Mmap(err)))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::growable_mmap::{GrowableMmap, MAX_MMAPS_COUNT, MIN_MMAP_BYTES};
    use std::fs::{File, OpenOptions};

    fn verify_it_works(file: Option<File>) {
        let mmap = GrowableMmap::new(file, None);
        assert!(mmap.is_ok());
        let mmap = mmap.unwrap();

        let mut initial = None;

        for i in 0..(MAX_MMAPS_COUNT * 2 + 2) as u64 {
            assert!(mmap
                .grow_and_apply(MIN_MMAP_BYTES, |mmap| {
                    let bytes = i.to_be_bytes()[0..8].to_vec();
                    for (i, item) in mmap.iter_mut().enumerate() {
                        *item = bytes.get(i % 8).cloned().unwrap();
                    }
                    Ok(())
                })
                .is_ok());

            if i == 2 {
                let read = mmap.storage.read().unwrap();
                initial = Some(read.inactive_mmaps.maps.get(1).cloned().unwrap());
            }
        }

        let read = mmap.storage.read().unwrap();
        match mmap.file {
            None => assert_eq!(MAX_MMAPS_COUNT * 2 + 1, read.inactive_mmaps.maps.len()),
            Some(_) => assert_eq!(1, read.inactive_mmaps.maps.len()),
        }

        assert_eq!(
            (MAX_MMAPS_COUNT * 2 + 1) * MIN_MMAP_BYTES,
            read.inactive_mmaps.index.len()
        );

        for i in 0..(MAX_MMAPS_COUNT * 2 + 2) as u64 {
            assert_eq!(
                Some(i),
                mmap.get_ref_and_apply(MIN_MMAP_BYTES * i as usize, |mmap| {
                    let mut dst = [0u8; 8];

                    dst.clone_from_slice(&mmap[0..8]);
                    Ok(Some(u64::from_be_bytes(dst)))
                })
                .unwrap()
            );
        }

        assert_eq!(
            Some(initial.unwrap().as_ref().to_vec()),
            mmap.get_ref_and_apply(MIN_MMAP_BYTES, |mmap| {
                Ok(Some(mmap[..MIN_MMAP_BYTES].to_vec()))
            })
            .unwrap()
        );
    }

    #[test]
    fn verify_it_works_mem() {
        verify_it_works(None);
    }

    fn create_file() -> File {
        let path = tempfile::tempdir().unwrap();
        let path = path.as_ref().join("data");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        options.open(path).unwrap()
    }

    #[test]
    fn verify_it_works_file() {
        let file = create_file();
        verify_it_works(Some(file));
    }
}
