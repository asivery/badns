use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};

#[derive(Debug)]
struct ExpiringValue<V> {
    value: V,
    expiration: SystemTime,
}

impl<V> ExpiringValue<V> {
    fn new(value: V, ttl: Duration) -> Self {
        Self {
            value,
            expiration: SystemTime::now() + ttl,
        }
    }
}

#[derive(Debug)]
pub struct TTLDict<K: Eq + std::hash::Hash, V> {
    backing: HashMap<K, ExpiringValue<V>>,
    drop_queue: VecDeque<K>,
}

impl<K: Eq + std::hash::Hash + Clone, V> TTLDict<K, V> {
    pub fn new() -> Self {
        Self {
            backing: HashMap::new(),
            drop_queue: VecDeque::new(),
        }
    }

    fn tidy_up(&mut self) {
        let now = SystemTime::now();
        let to_delete: Vec<_> = self
            .backing
            .iter()
            .filter(|(_, expiring)| expiring.expiration < now)
            .map(|(key, _)| key.clone())
            .collect();

        for key in to_delete {
            self.backing.remove(&key);
        }
    }

    pub fn get<'a>(&'a mut self, key: &K) -> Option<&'a V> {
        self.tidy_up();
        self.backing.get(key).map(|expiring| &expiring.value)
    }

    pub fn set(&mut self, key: K, value: V, ttl: Duration) {
        let expiring = ExpiringValue::new(value, ttl);
        self.backing.insert(key.clone(), expiring);
        self.drop_queue.retain(|k| k != &key);
        self.drop_queue.push_back(key);
    }
}
