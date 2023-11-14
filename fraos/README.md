# `fraos` - a fast reindexable append only storage

Guarantees:
* thread-safe
* changes are dumped to the disk at the moment of insertion or the insertion is failed
* the records are stored / iterated in the order of insertion

This work was impressed by the original works of data-pile by Eugene Babichenko. Currently maintained by Eugene Gostkin and dcSpark team.

## Design goals

* Efficient append of big chunks of data.
* A user should be able to copy the storage data (for example, over the network)
  while still being able to use the database for both reads and writes.
* The storage should have a minimal dependency footprint.
* Thread-safety

## Example

```rust
let storage = Database::file(path).unwrap()?;
let value = b"some data";
db.put(&value).unwrap();
```

## How it works

### Field schema
```rust
pub struct Database {
    flatfile: Arc<FlatFile>,
    seqno_index: Arc<SeqNoIndex>,
    write_lock: Arc<Mutex<()>>,
}
```

Field roles:
* `flatfile` - the **raw data file**, where the bytes are stored sequentially
* `seqno_index` - sequentially stored pairs `(offset, length)` that point to records stored in **raw data file**
  * can be accessed by the `sequential index` (the right offset is `2 * size_of::<usize>() * n`)
* `write_lock` - handles concurrency

### Memory allocation

Both `flatfile` and `seqno_index` use `Appender` concept inside:
```rust
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
    ) -> Result<Self, FraosError> { ... }

    /// Append data to the file. The mutable pointer to the new data location is
    /// given to `f` which should write the data. This function will block if
    /// another write is in progress.
    pub fn append<F>(&self, size_inc: usize, f: F) -> Result<(), FraosError>
    where
        F: Fn(&mut [u8]) -> Result<(), FraosError>,
    { ... }

    /// The whole data buffer is given to `f` which should return the data back
    /// or return None if something went wrong.
    pub fn get_data<F, U>(&self, offset: usize, f: F) -> Result<Option<U>, FraosError>
    where
        F: Fn(&[u8]) -> Result<Option<U>, FraosError>,
    { ... }

    pub fn memory_size(&self) -> usize { ... }

    pub fn shrink_to_size(&self) -> Result<(), FraosError> { ... }
}

```

```rust
pub(crate) struct GrowableMmap {
    storage: RwLock<Storage>,
    file: Option<File>,
}

struct Storage {
    inactive_mmaps: InactiveMmaps,
    active_map: Option<ActiveMmap>,
}
```

`GrowableMmap` has an active mutable mmap tail (`active_map`) and inactive prefix (`inactive_mmaps`). 
* If we have enough space we add records to the active mmap
* If we don't have enough space:
  * we slice the active mmap to the actual end of writes
  * put it to inactive mmaps 
  * create a new mmap either of size of the data or `MIN_MMAP_BYTES`
* If `inactive_mmaps` has more than `MAX_MMAPS_COUNT` mmaps we remap the existing data and create a single mmap for that data
  * This is needed, since on UNIX-like systems there's a limit on how much mmaps a process can have at a time. If the limit is exceeded the storage will stop working

When the data is being appended:
* We try check if `GrowableMmap` of `flatfile` has an active section. 
  * If free space in active section is enough, then the data is written into the free section and dumped to disk
  * If free space is not enough the current active mmap is cut, added to list of inactive mmaps and new chunk is allocated. The data is written to the allocated section and dumped to disk. If the record is too big the active mmap size is equal to the record size
* Same applies to `GrowableMmap` of `seqno_index`:
  * We append the pair of `offset` and `length` to the active section, so the index know where to search the data in `flatfile`

Reload note:

* If the storage is reloaded without proper `drop` it might be the case when the end of storage is filled with zeros. So the actual amount of stored data is less than amount of allocated memory. This way:
  * for `flatfile` this is no problem: we never go there if we don't have a link from `seqno_index`
  * for `seqno_index` this is a problem: we need to identify where is the actual end of the data:
    * you can see that `(offset, length)` pairs have a special structure: `offset` is monotonically increasing sequence, `length` is always non-zero
    * when we reload the `seqno_index` and see that it is not empty, but has zeros in the end we use binary search to find the actual storage end and reload the storage knowing the size already
