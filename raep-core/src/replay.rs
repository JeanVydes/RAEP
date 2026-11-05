#![warn(missing_docs)]

use crate::DeliveryError;
use std::collections::VecDeque;

/// A bounded ordered window of server events.
#[derive(Clone, Debug)]
pub struct ReplayWindow<T> {
    capacity: usize,
    events: VecDeque<(u64, T)>,
}

impl<T> ReplayWindow<T> {
    /// Creates a window with a positive capacity.
    pub fn new(capacity: usize) -> Result<Self, DeliveryError> {
        if capacity == 0 {
            return Err(DeliveryError::InvalidCapacity);
        }
        Ok(Self {
            capacity,
            events: VecDeque::with_capacity(capacity),
        })
    }

    /// Appends an event and evicts the oldest retained event when necessary.
    pub fn append(&mut self, sequence: u64, event: T) -> Result<(), DeliveryError> {
        if let Some((last, _)) = self.events.back() {
            if sequence <= *last {
                return Err(DeliveryError::NonMonotonic {
                    sequence,
                    last: *last,
                });
            }
        }
        if self.events.len() == self.capacity {
            self.events.pop_front();
        }
        self.events.push_back((sequence, event));
        Ok(())
    }

    /// Returns events after the last sequence processed by a client.
    pub fn replay_after(&self, last_seen: u64) -> Result<Vec<(u64, T)>, DeliveryError>
    where
        T: Clone,
    {
        let Some((first, _)) = self.events.front() else {
            return Ok(Vec::new());
        };
        let requested = last_seen.saturating_add(1);
        if requested < *first {
            return Err(DeliveryError::ReplayTruncated {
                requested,
                first: *first,
            });
        }
        Ok(self
            .events
            .iter()
            .filter(|(sequence, _)| *sequence > last_seen)
            .cloned()
            .collect())
    }

    /// Drops events acknowledged by a client.
    pub fn acknowledge(&mut self, sequence: u64) {
        while self
            .events
            .front()
            .is_some_and(|(current, _)| *current <= sequence)
        {
            self.events.pop_front();
        }
    }

    /// Returns the first retained sequence.
    pub fn first_sequence(&self) -> Option<u64> {
        self.events.front().map(|(sequence, _)| *sequence)
    }

    /// Returns the latest retained sequence.
    pub fn last_sequence(&self) -> Option<u64> {
        self.events.back().map(|(sequence, _)| *sequence)
    }
}
