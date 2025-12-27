use crate::core::{clock::TimeT, timer::Timer};
use std::collections::{BinaryHeap, HashSet};

pub const SHORT_WHEEL_NUM_BUCKETS: usize = 128;
pub const LONG_WHEEL_NUM_BUCKETS: usize = 4096;
pub const SHORT_WHEEL_RESOLUTION_MS: TimeT = 8;
pub const SHORT_WHEEL_PERIOD_MS: TimeT = SHORT_WHEEL_RESOLUTION_MS * SHORT_WHEEL_NUM_BUCKETS;
pub const LONG_WHEEL_RESOLUTION_MS: TimeT = SHORT_WHEEL_PERIOD_MS;
pub const LONG_WHEEL_PERIOD_MS: TimeT = LONG_WHEEL_RESOLUTION_MS * LONG_WHEEL_NUM_BUCKETS;

pub type Bucket = HashSet<Timer>;

pub struct Wheel {
    buckets: Vec<Bucket>,
    pub resolution: TimeT,
    pub period: TimeT,
}

impl Wheel {
    pub fn new_short_wheel() -> Self {
        Wheel::new(
            SHORT_WHEEL_NUM_BUCKETS,
            SHORT_WHEEL_RESOLUTION_MS,
            SHORT_WHEEL_PERIOD_MS,
        )
    }

    pub fn new_long_wheel() -> Self {
        Wheel::new(
            LONG_WHEEL_NUM_BUCKETS,
            LONG_WHEEL_RESOLUTION_MS,
            LONG_WHEEL_PERIOD_MS,
        )
    }

    fn new(num_buckets: usize, resolution: TimeT, period: TimeT) -> Self {
        let buckets: Vec<Bucket> = (0..num_buckets).map(|_| HashSet::new()).collect();
        Wheel {
            buckets,
            resolution,
            period,
        }
    }

    pub fn insert(&mut self, timer: Timer) {
        let index = self.bucket_index(timer.pop_time());
        // TODO handle error.
        self.buckets
            .get_mut(index)
            .expect("Expected bucket at index")
            .insert(timer);
    }

    pub fn pop(&mut self, timestamp: TimeT) -> Bucket {
        let index = self.bucket_index(timestamp);
        std::mem::take(
            self.buckets
                .get_mut(index)
                .expect("Expected bucket at index"),
        )
        // TODO handle error if index is outside bounds
    }

    pub fn _get(&self, timer: &Timer) -> Option<&Timer> {
        let index = self.bucket_index(timer.pop_time());
        self.buckets
            .get(index)
            .expect("Expected bucket at index")
            .get(timer)
    }

    pub fn remove(&mut self, timer: &Timer) -> bool {
        let index = self.bucket_index(timer.pop_time());
        self.buckets
            .get_mut(index)
            .expect("Expected bucket at index")
            .remove(timer)
    }

    fn bucket_index(&self, timestamp: TimeT) -> usize {
        (timestamp / self.resolution) % self.buckets.len()
    }

    pub fn round_timestamp(&self, timestamp: TimeT) -> TimeT {
        timestamp - (timestamp % self.resolution)
    }

    pub fn should_insert(&self, tick: &TimeT, timer: &Timer) -> bool {
        self.round_timestamp(timer.pop_time()) < self.round_timestamp(tick + self.period)
    }
}

pub struct TimerHeap {
    heap: BinaryHeap<Timer>,
    tombstones: HashSet<Timer>,
}

impl TimerHeap {
    pub fn new() -> Self {
        TimerHeap {
            heap: BinaryHeap::new(),
            tombstones: HashSet::new(),
        }
    }

    pub fn push(&mut self, timer: Timer) {
        self.heap.push(timer)
    }

    pub fn remove(&mut self, timer: &Timer) -> bool {
        // TODO log if the tombstone already existed.
        self.tombstones.insert(timer.clone())
    }

    /// Wrapper around the heap peek which buries tombstoned timers.
    pub fn peek(&mut self) -> Option<&Timer> {
        self.bury();
        self.heap.peek()
    }

    /// Wrapper around the heap pop which buries tombstoned timers.
    pub fn pop(&mut self) -> Option<Timer> {
        self.bury();
        self.heap.pop()
    }

    /// Bury upcoming timers which have a tombstone (RIP).
    fn bury(&mut self) {
        while let Some(next_timer) = self.heap.peek() {
            if self.tombstones.contains(next_timer) {
                if let Some(timer) = self.heap.pop() {
                    self.tombstones.remove(&timer);
                }
            } else {
                break;
            }
        }
    }
}
