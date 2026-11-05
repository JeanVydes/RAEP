#![warn(missing_docs)]

use crate::DeliveryError;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// A bounded cache of outcomes for idempotent client commands.
#[derive(Clone, Debug)]
pub struct IdempotencyCache<K, V> {
    capacity: usize,
    values: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K, V> IdempotencyCache<K, V>
where
    K: Clone + Eq + Hash,
{
    /// Creates a cache with a positive capacity.
    pub fn new(capacity: usize) -> Result<Self, DeliveryError> {
        if capacity == 0 {
            return Err(DeliveryError::InvalidCapacity);
        }
        Ok(Self {
            capacity,
            values: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
        })
    }

    /// Returns a previously recorded outcome.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.values.get(key)
    }

    /// Records an outcome, replacing an existing key without growing beyond
    /// the configured capacity.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let existed = self.values.contains_key(&key);
        let was_full = self.values.len() == self.capacity;
        let previous = self.values.insert(key.clone(), value);
        if existed {
            self.order.retain(|current| current != &key);
            self.order.push_back(key);
            return previous;
        }
        if was_full {
            if let Some(oldest) = self.order.pop_front() {
                self.values.remove(&oldest);
            }
        }
        self.order.push_back(key);
        None
    }

    /// Returns the number of cached outcomes.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether no outcomes are cached.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
