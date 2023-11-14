# Indexed Log Map Store

Indexed Log Map Storage - persistent insert-ordered append-only thread-safe KV-store. It's similar to what `BTreeMap` does, but disk-based. 

## Use cases

The storage is handy in case any insert-ordered data need to be preserved between service restarts. E.g.:
* Blockchain data indexing & backup: if you want to fetch some specific data from the blocks and store related events for future (re)-usage. This way you can e.g. deploy new services with the events backup without the need to read all the blockchain to re-fetch these events, which can save a lot of sync time
* Blockchain backend events fetch & read: the storage works similarly to kafka topic. If you need to make sure no event is lost and the events are handled in order of appearance on chain - the storage is for you. 
  * You can remember at which position you finished the reads using e.g. `SaveProgressSource` from [blockchain-source-library](../blockchain-source)

## Example

```rust
// key / value / path are already assumed as initialized
let config = IndexedLogMapConfig {
    storage_path: Some(path),
    use_key_indexing: true,
    readonly: false,
};
let storage = IndexedLogMap::<KeyType, ValueType>::new(config)?;
let storage = Arc::new(storage);

storage.append(key, value).unwrap();
assert_eq!(value, storage.get(&key)?.unwrap());
```
## How it works

The storage is based on `sled::db` and `fraos`. 
`fraos` - thread-safe fast reindexable append only storage. 
See more about this storage and memory structure [here](../fraos/README.md).

### Field schema
```rust
pub struct IndexedLogMap<Key: Serialize + DeserializeOwned, Value: Serialize + DeserializeOwned> {
    storage: fraos::Database, // raw storage
    key_to_seqno: Option<sled::Db>, // key mapping
    phantom_data: PhantomData<(Key, Value)>,
}
```

Field roles:
* `storage` - the raw data + sequential index on top of that
* `key_to_seqno` - mapping from key to sequential index

### Lookups technique
When you call `storage.get(&key)`:
* storage lookups the `key` -> `sequential index` mapping in `key_to_seqno`
* then the inner `storage` performs lookup in the sequential index file for `sequential index` to get `(offseet, length)` pair
* then the inner `storage` performs lookup in the raw data file for `offset` and tries to read `length` bytes
* in case of successful read the data is deserialized to the proper type and returned

When you call `storage.iter_from(&key)`:
* storage lookups the `key` -> `sequential index` mapping in `key_to_seqno`
* this `sequential index` is needed to identify from which position to read data in the underlying storage
* then for each record:
  * the inner storage sequentially iterates the sequential index file from `sequential index` to get `(offseet, length)` pairs
  * then the inner `storage` performs lookup in the raw data file for `offset` and tries to read `length` bytes
  * in case of successful read the data is deserialized to the proper type and returned

### Main functionality
Methods:
```rust
impl<Key: Serialize + DeserializeOwned, Value: Serialize + DeserializeOwned>
    IndexedLogMap<Key, Value>
{
    pub fn new(config: IndexedLogMapConfig) -> Result<Self> { ... }
    
    pub fn append(&self, key: Key, value: Value) -> Result<()> { ... }
    pub fn get(&self, key: &Key) -> Result<Option<Value>> { ... }
    
    pub fn iter_from(&self, key: &Key) -> Result<Option<impl Iterator<Item = Result<Value>>>> { ... }
    pub fn iter(&self) -> Option<impl Iterator<Item = Result<Value>>> { ... }

    pub fn last(&self) -> Result<Option<Value>> { ... }
    
    pub fn contains(&self, key: &Key) -> Result<bool> { ... }
    pub fn is_empty(&self) -> Result<bool> { ... }

}
```

* `new(config: IndexedLogMapConfig)` - creates the storage based on the config
* `append(&self, key: Key, value: Value)` - append the `key`-`value` pair into the storage (so you can `index_from` it after)
  * if you append already existing `key` the old `key` value is still kept in the storage history. So if you do:
    * `storage.get(&key)` - new value will be returned
    * `storage.iter()` - both values will be seen in the order of insertion
* `get(&self, key: &Key)` - get the value by `key` or `None`
* `iter_from(&self, key: &Key)` - iterate the storage in order of insertion from position `key`
* `iter(&self)` - iterate the storage in order of insertion from the beginning
* `last(&self)` - return last element in the storage (if exists) or `None`
* `contains(&self, key: &Key)` - check if the storage contains `key`
* `is_empty(&self)` - check if the storage is empty

