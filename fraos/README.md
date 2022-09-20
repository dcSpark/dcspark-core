# `fraos` - a fast reindexable append only storage

Guarantees:
* thread-safe
* changes are dumped to the disk at the moment of insertion

This work was impressed by the original works of data-pile by Eugene Babichenko. Currently maintained by Eugene Gostkin and dcSpark team.

## Design goals

* Efficient append of big chunks of data.
* A user should be able to copy the storage data (for example, over the network)
  while still being able to use the database for both reads and writes.
* The storage should have a minimal dependency footprint.
* Thread-safety

## Usage guide

### Example

```rust
use data_pile::Database;
let db = Database::file("./pile").unwrap();
let value = b"some data";
db.put(&value).unwrap();
```

### Notes

Values are accessible only by their sequential numbers. You will need an
external index if you want any other kind of keys.
