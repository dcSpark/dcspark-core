use crate::config::IndexedLogMapConfig;
use anyhow::{anyhow, bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::marker::PhantomData;

/// Indexed log map inherits the properties of fraos::Database (thread safety) and allows
/// access by key
pub struct IndexedLogMap<Key: Serialize + DeserializeOwned, Value: Serialize + DeserializeOwned> {
    storage: fraos::Database,
    // this is an option since we might want to take a look at
    key_to_seqno: Option<sled::Db>,
    phantom_data: PhantomData<(Key, Value)>,
}

unsafe impl<Key: Serialize + DeserializeOwned, Value: Serialize + DeserializeOwned> Send
    for IndexedLogMap<Key, Value>
{
}
unsafe impl<Key: Serialize + DeserializeOwned, Value: Serialize + DeserializeOwned> Sync
    for IndexedLogMap<Key, Value>
{
}

impl<Key: Serialize + DeserializeOwned, Value: Serialize + DeserializeOwned>
    IndexedLogMap<Key, Value>
{
    pub fn new(config: IndexedLogMapConfig) -> Result<Self> {
        let storage = match &config.storage_path {
            None => fraos::Database::memory(),
            Some(path) => match &config.readonly {
                true => fraos::Database::file_readonly(path.clone()),
                false => fraos::Database::file(path.clone()),
            },
        }
        .context("can't create / open database")?;

        let key_to_seqno = if config.use_key_indexing {
            let key_to_seqno = match &config.storage_path {
                None => sled::Config::default()
                    .temporary(true)
                    .open()
                    .context("Open temporary index db error"),
                Some(path) => sled::Config::new()
                    .path(path.join("key_index"))
                    .mode(sled::Mode::HighThroughput)
                    .open()
                    .context("Failed to open the key index's persistent database"),
            }?;
            Some(key_to_seqno)
        } else {
            None
        };

        Ok(Self {
            storage,
            key_to_seqno,
            phantom_data: Default::default(),
        })
    }

    pub fn append(&self, key: Key, value: Value) -> Result<()> {
        if self.key_to_seqno.is_none() {
            return Err(anyhow!("Can't append when key index is not available"));
        }
        let mut serialized_key = Vec::new();
        ciborium::ser::into_writer(&key, &mut serialized_key)
            .context("Failed to encode cbor data")?;
        let mut serialized_value = Vec::new();
        ciborium::ser::into_writer(&value, &mut serialized_value)
            .context("Failed to encode cbor data")?;
        let records = vec![serialized_value.as_slice()];
        let index = self
            .storage
            .append_get_seqno(records.as_slice())
            .context("can't append to storage")?;
        match index {
            None => Err(anyhow!("Value is not inserted")),
            Some(position) => {
                self.key_to_seqno
                    .as_ref()
                    .unwrap()
                    .insert(
                        serialized_key.as_slice(),
                        deps::serde_json::to_vec(&position)
                            .context("we should always be able to encode")?,
                    )
                    .context("can't insert key mapping into index")?;
                Ok(())
            }
        }
    }

    pub fn iter_from(&self, key: &Key) -> Result<Option<impl Iterator<Item = Result<Value>>>> {
        if self.key_to_seqno.is_none() {
            return Err(anyhow!(
                "Can't iter from key when key index is not available"
            ));
        }
        let mut serialized_key = Vec::new();
        ciborium::ser::into_writer(key, &mut serialized_key)
            .context("Failed to encode cbor data")?;
        let key_position = match self.key_to_seqno.as_ref().unwrap().get(serialized_key)? {
            None => return Ok(None),
            Some(pos) => pos,
        };

        let index: usize = deps::serde_json::from_slice(&key_position)
            .expect("we should always be able to decode");

        let iter = match self.storage.iter_from_seqno(index) {
            None => return Ok(None),
            Some(iter) => iter,
        };

        Ok(Some(iter.map(|mmap| -> Result<Value> {
            ciborium::de::from_reader(mmap?.as_slice()).context("can't deserialize cbor rep")
        })))
    }

    pub fn get(&self, key: &Key) -> Result<Option<Value>> {
        let iter = self.iter_from(key)?;
        let element = match iter {
            None => None,
            Some(mut pos) => pos.next(),
        };
        match element {
            None => Ok(None),
            Some(result) => match result {
                Ok(value) => Ok(Some(value)),
                Err(err) => Err(err),
            },
        }
    }

    pub fn contains(&self, key: &Key) -> Result<bool> {
        match self.get(key) {
            Ok(value) => Ok(value.is_some()),
            Err(err) => Err(err),
        }
    }

    pub fn is_empty(&self) -> Result<bool> {
        match &self.key_to_seqno {
            None => bail!("Can't check key when key index is not available"),
            Some(db) => Ok(db.is_empty()),
        }
    }

    pub fn iter(&self) -> Option<impl Iterator<Item = Result<Value>>> {
        let iter = match self.storage.iter_from_seqno(0) {
            None => return None,
            Some(iter) => iter,
        };

        Some(iter.map(|mmap| -> Result<Value> {
            ciborium::de::from_reader(mmap?.as_slice()).context("can't deserialize cbor rep")
        }))
    }

    pub fn last(&self) -> Result<Option<Result<Value>>> {
        Ok(self
            .storage
            .last()
            .map_err(|err| anyhow!("can't get last element of the storage: {:?}", err))?
            .map(|mmap| -> Result<Value> {
                ciborium::de::from_reader(mmap.as_slice()).context("can't deserialize cbor rep")
            }))
    }
}

pub fn get_next<Key: Serialize + DeserializeOwned, Value: Serialize + DeserializeOwned>(
    storage: &IndexedLogMap<Key, Value>,
    key: &Option<Key>,
) -> Result<Option<Value>> {
    match key {
        None => {
            let iter = storage.iter();
            match iter {
                None => Ok(None),
                Some(mut iter) => match iter.next() {
                    None => Ok(None),
                    Some(next) => Ok(Some(next?)),
                },
            }
        }
        Some(from) => {
            let current_pos_iter = storage.iter_from(from)?;
            let iter = current_pos_iter.and_then(|mut iter| iter.next().map(|_| iter));
            match iter {
                None => Ok(None),
                Some(mut iter) => match iter.next() {
                    None => Ok(None),
                    Some(next) => Ok(Some(next?)),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::IndexedLogMapConfig;
    use crate::storage::IndexedLogMap;
    use rand::{distributions::Alphanumeric, Rng};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;
    use std::thread::JoinHandle;
    use std::time::Duration;

    #[test]
    fn test_serde() {
        let path = create_temp_dir();
        let config = IndexedLogMapConfig {
            storage_path: Some(path),
            use_key_indexing: true,
            readonly: false,
        };

        let storage = IndexedLogMap::<usize, usize>::new(config);
        let storage = Arc::new(storage.unwrap());

        let n_items = 10000usize;
        for i in 0..n_items {
            storage.append(i, i).unwrap();
        }

        let iter_from_vec = [
            0usize,
            n_items / 4,
            n_items / 2,
            n_items / 4 * 3,
            n_items - 1,
            1usize,
        ];

        for i in iter_from_vec.iter() {
            assert_eq!(storage.get(i).unwrap().unwrap(), *i);
            assert!(storage.contains(i).unwrap());
            assert!(!storage.contains(&(i + n_items)).unwrap());
        }

        let threads: Vec<_> = iter_from_vec
            .iter()
            .map(|iter_from| {
                let iter_from = *iter_from;
                let storage = storage.clone();
                thread::spawn(move || {
                    let iterator = storage.iter_from(&iter_from);
                    assert!(iterator.is_ok(), "can't obtain iterator");
                    let iterator = iterator.unwrap();
                    assert!(iterator.is_some(), "no elements to iterate from");
                    let iterator = iterator.unwrap();
                    let mut prev = match iter_from {
                        0usize => None,
                        _ => Some(iter_from - 1),
                    };
                    let mut seen = 0usize;
                    for i in iterator {
                        assert!(i.is_ok(), "deserialize error");
                        let unwrapped = i.unwrap();
                        if let Some(prev) = prev {
                            assert_eq!(prev + 1, unwrapped);
                        }
                        prev = Some(unwrapped);
                        seen += 1;
                    }
                    assert_eq!(seen, n_items - iter_from);
                })
            })
            .collect();

        for thread in threads {
            assert!(thread.join().is_ok());
        }
    }

    #[test]
    fn test_concurrency() {
        let path = create_temp_dir();
        let config = IndexedLogMapConfig {
            storage_path: Some(path),
            use_key_indexing: true,
            readonly: false,
        };

        let storage = IndexedLogMap::<usize, usize>::new(config);
        let storage = Arc::new(storage.unwrap());

        let n_items_per_thread = 5000usize;
        let threads_count = 8;
        // we have a linear order from 0 to n_items_per_thread for each thread
        let appending_fn = |thread_number| {
            let storage = storage.clone();
            thread::spawn(move || {
                for i in 0..n_items_per_thread {
                    let item = n_items_per_thread * thread_number + i;
                    storage.append(item, item).unwrap();
                }
            })
        };

        // we just check that we can read while writing
        let reading_fn = |_thread_number| {
            let storage = storage.clone();
            thread::spawn(move || {
                let iterator = storage.iter();
                assert!(iterator.is_some(), "no elements to iterate from");
                let iterator = iterator.unwrap();
                iterator.count()
            })
        };

        let mut appending_threads: Vec<JoinHandle<()>> = vec![];
        let mut reading_threads: Vec<JoinHandle<usize>> = vec![];
        let iterations = 3;
        for iteration in 0..iterations {
            appending_threads.extend(
                (threads_count * iteration..threads_count * (iteration + 1))
                    .map(appending_fn)
                    .collect::<Vec<_>>(),
            );
            thread::sleep(Duration::new(1, 0));
            reading_threads.extend((0..2 * threads_count).map(reading_fn).collect::<Vec<_>>());
        }

        for thread in appending_threads {
            assert!(thread.join().is_ok());
        }

        // check that all writes are successful
        let events_count = storage.iter().unwrap().count();
        assert_eq!(
            threads_count * iterations * n_items_per_thread,
            events_count
        );

        // check that all reads are ok
        for thread in reading_threads {
            let joined = thread.join();
            assert!(joined.is_ok());
            assert!(joined.unwrap() <= events_count);
        }

        // check the inserted order of events (per thread)
        let mut last_seen: Vec<Option<usize>> = vec![None; iterations * threads_count];
        for element in storage.iter().unwrap() {
            assert!(element.is_ok());
            let value = element.unwrap();
            let thread_index = value / n_items_per_thread;
            let local_index = value % n_items_per_thread;
            if let Some(last_seen) = last_seen[thread_index] {
                assert_eq!(last_seen + 1, local_index);
            }
            last_seen[thread_index] = Some(local_index);
        }

        // check that we've seen all the events
        for last_seen in last_seen.into_iter() {
            assert!(last_seen.is_some());
            assert_eq!(last_seen.unwrap(), n_items_per_thread - 1);
        }
    }

    fn create_temp_dir() -> PathBuf {
        let suffix: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();

        tempfile::tempdir()
            .unwrap()
            .into_path()
            .join("milkomeda_IndexedLogMap_concurrency_test".to_owned() + suffix.as_str())
    }
}
