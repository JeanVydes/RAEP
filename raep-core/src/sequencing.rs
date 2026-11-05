#![warn(missing_docs)]

/// Allocates strictly increasing sequence numbers.
#[derive(Clone, Debug)]
pub struct Sequencer {
    next: u64,
}

impl Default for Sequencer {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl Sequencer {
    /// Returns the next sequence and advances the allocator.
    pub fn allocate(&mut self) -> u64 {
        let sequence = self.next;
        self.next = self.next.saturating_add(1);
        sequence
    }

    /// Returns the next sequence without advancing the allocator.
    pub fn peek(&self) -> u64 {
        self.next
    }
}
