use std::error;
use std::fmt::{Display, Formatter};

/// Represents raw data bounds: number of mmap, offset in particular it, data length
#[derive(Debug, Eq, PartialEq)]
pub struct IndexDescriptor {
    pub mmap_number: usize,
    pub mmap_offset: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MmapIndexError {
    /// invalid offset within same mmap
    SingleMmapInvalidOffsetOrder { previous_end: usize, end: usize },
    /// zero index can't be added
    AppendZeroOffset,
    /// invalid offset in outer mmaps
    MmapInvalidOffsetOrder { previous_end: usize, end: usize },
    /// generic error for everything else
    InconsistentState { description: String },
}

impl Display for MmapIndexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MmapIndexError::SingleMmapInvalidOffsetOrder { previous_end, end } => {
                write!(
                    f,
                    "invalid offset in the mmap: prev end: {}, proposed end: {}",
                    previous_end, end
                )
            }
            MmapIndexError::AppendZeroOffset => {
                write!(f, "can't append zero length")
            }
            MmapIndexError::MmapInvalidOffsetOrder { previous_end, end } => {
                write!(
                    f,
                    "invalid offset in the global mmaps structure: prev end: {}, proposed end: {}",
                    previous_end, end
                )
            }
            MmapIndexError::InconsistentState { description } => {
                write!(f, "inconsistent state: {}", description)
            }
        }
    }
}

impl error::Error for MmapIndexError {}

/// A particular mmap can have multiple objects inside
pub struct MmapChunkAddressMapper {
    relative_internal_bounds: Vec<usize>,
    global_chunk_start: usize,
}

/// Each mmap must be easily accessible and indexable
/// This structure allows that
pub struct IndexOnMmaps {
    mmaps: Vec<MmapChunkAddressMapper>,
}

impl MmapChunkAddressMapper {
    pub fn new(global_chunk_start: usize) -> Self {
        Self {
            relative_internal_bounds: Vec::new(),
            global_chunk_start,
        }
    }

    pub fn global_chunk_end(&self) -> usize {
        self.global_chunk_start
            + self
                .relative_internal_bounds
                .last()
                .copied()
                .unwrap_or(0usize)
    }

    pub fn global_chunk_start(&self) -> usize {
        self.global_chunk_start
    }

    pub fn size(&self) -> usize {
        self.relative_internal_bounds
            .last()
            .copied()
            .unwrap_or(0usize)
    }

    #[allow(unused)]
    pub fn append_global_end(&mut self, global_end: usize) -> Result<(), MmapIndexError> {
        match global_end {
            global_end if global_end < self.global_chunk_start => {
                Err(MmapIndexError::SingleMmapInvalidOffsetOrder {
                    previous_end: self.global_chunk_end(),
                    end: global_end,
                })
            }
            global_end if global_end == self.global_chunk_start => {
                Err(MmapIndexError::AppendZeroOffset)
            }
            global_end => {
                let relative_end = global_end.checked_sub(self.global_chunk_start).ok_or(
                    MmapIndexError::InconsistentState {
                        description: format!(
                            "global end ({}) must be > global start ({})",
                            global_end, self.global_chunk_start
                        ),
                    },
                )?;
                self.append_relative_end(relative_end)
            }
        }
    }

    pub fn append_relative_end(&mut self, relative_end: usize) -> Result<(), MmapIndexError> {
        let previous_end = self.relative_internal_bounds.last().copied().unwrap_or(0);

        if relative_end == 0 {
            return Err(MmapIndexError::AppendZeroOffset);
        }

        if previous_end >= relative_end {
            return Err(MmapIndexError::SingleMmapInvalidOffsetOrder {
                previous_end: previous_end + self.global_chunk_start,
                end: relative_end + self.global_chunk_start,
            });
        }

        self.relative_internal_bounds.push(relative_end);
        Ok(())
    }

    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    pub fn find(&self, address: usize) -> Result<Option<IndexDescriptor>, MmapIndexError> {
        if self.is_empty()
            || address < self.global_chunk_start
            || self.global_chunk_end() <= address
        {
            // required address is in other chunk or current chunk is empty
            return Ok(None);
        }

        // now chunk is not empty and address is in [start, end)
        let relative_address = address - self.global_chunk_start;
        if relative_address == 0 {
            return Ok(self
                .relative_internal_bounds
                .first()
                .map(|next_relative_offset| IndexDescriptor {
                    mmap_number: 0,
                    mmap_offset: relative_address,
                    len: *next_relative_offset,
                }));
        }

        match self
            .relative_internal_bounds
            .binary_search(&relative_address)
        {
            Ok(relative_address_position) => {
                let mmap_offset = self.relative_internal_bounds[relative_address_position];
                let len = self
                    .relative_internal_bounds
                    .get(relative_address_position + 1)
                    .ok_or(MmapIndexError::InconsistentState {
                        description: format!("expected not empty search while last position is found: relative address: {}, position: {}", relative_address, mmap_offset)
                    })?
                    - mmap_offset;
                Ok(Some(IndexDescriptor {
                    mmap_number: 0,
                    mmap_offset,
                    len,
                }))
            }
            Err(can_be_inserted_at) => {
                let upper_bound = self.relative_internal_bounds[can_be_inserted_at];
                let mmap_offset = address - self.global_chunk_start;
                Ok(Some(IndexDescriptor {
                    mmap_number: 0,
                    mmap_offset,
                    len: upper_bound - mmap_offset,
                }))
            }
        }
    }
}

impl IndexOnMmaps {
    pub fn new() -> Self {
        Self { mmaps: Vec::new() }
    }

    pub fn append(&mut self, next_mmap: MmapChunkAddressMapper) -> Result<(), MmapIndexError> {
        if next_mmap.is_empty() {
            return Ok(());
        }
        let current_global_end = self.len();
        if next_mmap.global_chunk_start() != current_global_end {
            return Err(MmapIndexError::MmapInvalidOffsetOrder {
                previous_end: current_global_end,
                end: next_mmap.global_chunk_start(),
            });
        }

        self.mmaps.push(next_mmap);
        Ok(())
    }

    pub fn find(&self, address: usize) -> Result<Option<IndexDescriptor>, MmapIndexError> {
        let mmap_number = match self
            .mmaps
            .binary_search_by_key(&address, |mmap_index| -> usize {
                if mmap_index.global_chunk_start() <= address
                    && address < mmap_index.global_chunk_end()
                {
                    address
                } else {
                    mmap_index.global_chunk_start
                }
            }) {
            Ok(position) => position,
            Err(_) => return Ok(None),
        };

        let index = self.mmaps[mmap_number].find(address)?;
        Ok(index.map(|index| IndexDescriptor {
            mmap_number,
            mmap_offset: index.mmap_offset,
            len: index.len,
        }))
    }

    pub fn len(&self) -> usize {
        self.mmaps
            .last()
            .map(|mmap_index| mmap_index.global_chunk_end())
            .unwrap_or(0)
    }

    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use crate::index_on_mmaps::{
        IndexDescriptor, IndexOnMmaps, MmapChunkAddressMapper, MmapIndexError,
    };

    #[test]
    fn base_index() {
        let data = [vec![34], vec![42, 67], vec![96, 103, 420]];
        let mut index = IndexOnMmaps::new();

        for item in data.iter() {
            let mut single_mmap_index = MmapChunkAddressMapper::new(index.len());
            for sub_item in item {
                assert!(single_mmap_index
                    .append_relative_end(*sub_item - index.len())
                    .is_ok());
            }
            assert!(index.append(single_mmap_index).is_ok());
        }
        assert_eq!(index.len(), 420);

        assert_eq!(
            Ok(Some(IndexDescriptor {
                len: 34,
                mmap_offset: 0,
                mmap_number: 0,
            })),
            index.find(0)
        );
        assert_eq!(
            Ok(Some(IndexDescriptor {
                len: 8,
                mmap_offset: 0,
                mmap_number: 1,
            })),
            index.find(34)
        );
        assert_eq!(
            Ok(Some(IndexDescriptor {
                len: 25,
                mmap_offset: 8,
                mmap_number: 1,
            })),
            index.find(42)
        );
        assert_eq!(
            Ok(Some(IndexDescriptor {
                len: 29,
                mmap_offset: 0,
                mmap_number: 2,
            })),
            index.find(67)
        );
        assert_eq!(
            Ok(Some(IndexDescriptor {
                len: 7,
                mmap_offset: 29,
                mmap_number: 2,
            })),
            index.find(96)
        );
        assert_eq!(Ok(None), index.find(420));
        assert_eq!(Ok(None), index.find(1000));
    }

    fn single_chunk_index_from(from: usize) {
        let mut chunk = MmapChunkAddressMapper::new(from);
        assert_eq!(chunk.global_chunk_start(), from);
        assert_eq!(chunk.global_chunk_end(), from);
        assert!(chunk.is_empty());

        // can't append 0
        assert_eq!(
            Err(MmapIndexError::AppendZeroOffset),
            chunk.append_relative_end(0)
        );

        // can append new end
        assert!(chunk.append_relative_end(1).is_ok());
        assert_eq!(chunk.global_chunk_start(), from);
        assert_eq!(chunk.global_chunk_end(), from + 1);
        assert!(!chunk.is_empty());

        // can't append same end twice
        assert_eq!(
            Err(MmapIndexError::SingleMmapInvalidOffsetOrder {
                previous_end: from + 1,
                end: from + 1,
            }),
            chunk.append_relative_end(1)
        );
        assert_eq!(chunk.global_chunk_start(), from);
        assert_eq!(chunk.global_chunk_end(), from + 1);
        assert!(!chunk.is_empty());

        assert!(chunk.append_relative_end(100).is_ok());
        assert_eq!(chunk.global_chunk_start(), from);
        assert_eq!(chunk.global_chunk_end(), from + 100);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn single_chunk_index_from_start() {
        single_chunk_index_from(0);
    }

    #[test]
    fn single_chunk_index_from_somewhere() {
        single_chunk_index_from(123441);
    }

    #[test]
    fn multiple_chunks() {
        let mut chunks = IndexOnMmaps::new();
        assert_eq!(chunks.len(), 0);
        assert!(chunks.is_empty());

        // append empty works
        assert!(chunks.append(MmapChunkAddressMapper::new(0)).is_ok());
        assert_eq!(chunks.len(), 0);
        assert!(chunks.is_empty());

        // append ok
        {
            let mut chunk = MmapChunkAddressMapper::new(0);
            assert!(chunk.append_relative_end(2).is_ok());
            assert!(chunk.append_relative_end(4).is_ok());
            assert!(chunk.append_relative_end(8).is_ok());

            assert!(chunks.append(chunk).is_ok());
            assert_eq!(chunks.len(), 8);
            assert!(!chunks.is_empty());
            assert_eq!(Ok(None), chunks.find(8));
            assert_eq!(
                Ok(Some(IndexDescriptor {
                    mmap_number: 0,
                    mmap_offset: 0,
                    len: 2,
                })),
                chunks.find(0)
            );
            assert_eq!(
                Ok(Some(IndexDescriptor {
                    mmap_number: 0,
                    mmap_offset: 1,
                    len: 1,
                })),
                chunks.find(1)
            );
            assert_eq!(
                Ok(Some(IndexDescriptor {
                    mmap_number: 0,
                    mmap_offset: 5,
                    len: 3,
                })),
                chunks.find(5)
            );
        }

        // append empty works
        assert!(chunks.append(MmapChunkAddressMapper::new(0)).is_ok());
        assert_eq!(chunks.len(), 8);
        assert!(!chunks.is_empty());

        // append with gap doesn't work
        {
            let mut chunk = MmapChunkAddressMapper::new(9);
            assert!(chunk.append_relative_end(2).is_ok());

            assert_eq!(
                Err(MmapIndexError::MmapInvalidOffsetOrder {
                    previous_end: 8,
                    end: 9,
                }),
                chunks.append(chunk)
            );
            assert_eq!(chunks.len(), 8);
            assert!(!chunks.is_empty());
        }

        // append after works
        {
            let mut chunk = MmapChunkAddressMapper::new(8);
            assert!(chunk.append_relative_end(2).is_ok());
            assert!(chunk.append_relative_end(4).is_ok());
            assert!(chunk.append_relative_end(8).is_ok());

            assert!(chunks.append(chunk).is_ok());
            assert_eq!(chunks.len(), 16);
            assert!(!chunks.is_empty());
            assert_eq!(Ok(None), chunks.find(16));
            assert_eq!(
                Ok(Some(IndexDescriptor {
                    mmap_number: 1,
                    mmap_offset: 0,
                    len: 2,
                })),
                chunks.find(8)
            );
            assert_eq!(
                Ok(Some(IndexDescriptor {
                    mmap_number: 1,
                    mmap_offset: 1,
                    len: 1,
                })),
                chunks.find(9)
            );
            assert_eq!(
                Ok(Some(IndexDescriptor {
                    mmap_number: 1,
                    mmap_offset: 5,
                    len: 3,
                })),
                chunks.find(13)
            );
        }
    }
}
