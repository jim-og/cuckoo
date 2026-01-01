use crate::core::clock::TimeT;
use std::cmp::Ordering;
use uuid::Uuid;

/// A Universally Unique Identifier (UUID) for Timers.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct TimerId(pub Uuid);

impl TimerId {
    pub fn new() -> Self {
        TimerId(Uuid::new_v4())
    }

    pub fn uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for TimerId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Timer {
    pub id: TimerId,
    start_time: TimeT,
    interval: TimeT,
}

impl Timer {
    pub fn new(id: TimerId, start_time: TimeT, interval: TimeT) -> Self {
        Self {
            id,
            start_time,
            interval,
        }
    }

    pub fn pop_time(&self) -> TimeT {
        self.start_time + self.interval
    }
}

impl Ord for Timer {
    // TODO check ordering
    fn cmp(&self, other: &Self) -> Ordering {
        (other.pop_time()).cmp(&self.pop_time())
    }
}

impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Test comparison in min-heap
