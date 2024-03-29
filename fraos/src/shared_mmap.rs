use memmap2::Mmap;
use std::{
    ops::{Bound::*, RangeBounds},
    slice,
    sync::Arc,
};

/// A structure that implements a view into memory mapping.
#[derive(Debug, Clone)]
pub struct SharedMmap {
    mmap: Arc<Mmap>,
    len: usize,
    slice: *const u8,
}

impl SharedMmap {
    pub(crate) fn new(mmap: Mmap) -> SharedMmap {
        let len = mmap.len();
        let slice = mmap.as_ptr();
        SharedMmap {
            mmap: Arc::new(mmap),
            len,
            slice,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get a sub-view. It will point to the same memory mapping as the parent
    /// mapping.
    pub fn slice(&self, bounds: impl RangeBounds<usize>) -> SharedMmap {
        if self.len == 0 {
            return SharedMmap {
                len: 0,
                ..self.clone()
            };
        }
        let start = match bounds.start_bound() {
            Included(start) => *start,
            Excluded(start) => start + 1,
            Unbounded => 0,
        };

        if start >= self.len {
            return SharedMmap {
                len: 0,
                ..self.clone()
            };
        }

        let end = match bounds.end_bound() {
            Included(end) => *end,
            Excluded(end) if *end == 0 => {
                return SharedMmap {
                    len: 0,
                    ..self.clone()
                };
            }
            Excluded(end) => end - 1,
            Unbounded => self.len - 1,
        };
        let end = std::cmp::min(end, self.len - 1);

        let len = if start <= end { end - start + 1 } else { 0 };
        let slice = unsafe { self.slice.add(start) };

        SharedMmap {
            mmap: self.mmap.clone(),
            len,
            slice,
        }
    }

    fn get_ref(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.slice, self.len) }
    }
}

// Those are safe to implement because the underlying `*const u8` is never
// modified.
unsafe impl Send for SharedMmap {}
unsafe impl Sync for SharedMmap {}

impl AsRef<[u8]> for SharedMmap {
    fn as_ref(&self) -> &[u8] {
        self.get_ref()
    }
}

#[cfg(test)]
mod tests {
    use crate::error::MmapError;
    use crate::shared_mmap::SharedMmap;
    use memmap2::MmapOptions;

    #[test]
    fn simple() {
        let mmapped_area = MmapOptions::new()
            .len(u8::MAX as usize + 1)
            .map_anon()
            .map_err(MmapError::Mmap);
        assert!(mmapped_area.is_ok());
        let mut mmapped_area = mmapped_area.unwrap();

        // set numbers from 0 to u8::MAX
        for (index, byte) in mmapped_area.iter_mut().enumerate() {
            *byte = index as u8;
        }

        // verify numbers are correct
        for i in u8::MIN as usize..u8::MAX as usize + 1 {
            assert_eq!(Some(i as u8), mmapped_area.get(i).cloned());
        }

        // create a shared mmap
        let read_only = mmapped_area.make_read_only().map_err(MmapError::Protect);
        assert!(read_only.is_ok());
        let read_only = read_only.unwrap();
        let shared_mmap = SharedMmap::new(read_only);

        let slice = shared_mmap
            .slice(0..u8::MAX as usize + 1)
            .get_ref()
            .to_vec();
        assert_eq!(slice[0], 0);
        assert_eq!(slice.last().cloned(), Some(u8::MAX));
        assert_eq!(slice.len(), u8::MAX as usize + 1);

        let slice = shared_mmap
            .slice(0..u8::MAX as usize + 100)
            .get_ref()
            .to_vec();
        assert_eq!(slice[0], 0);
        assert_eq!(slice.last().cloned(), Some(u8::MAX));
        assert_eq!(slice.len(), u8::MAX as usize + 1);

        let slice = shared_mmap
            .slice(1..u8::MAX as usize + 100)
            .get_ref()
            .to_vec();
        assert_eq!(slice[0], 1);
        assert_eq!(slice.last().cloned(), Some(u8::MAX));
        assert_eq!(slice.len(), u8::MAX as usize);

        let slice = shared_mmap.slice(1..u8::MAX as usize).get_ref().to_vec();
        assert_eq!(slice[0], 1);
        assert_eq!(slice.last().cloned(), Some(u8::MAX - 1));
        assert_eq!(slice.len(), u8::MAX as usize - 1);
    }
}
