use crate::core::timer::{TimeT, Timer};
use std::collections::{BinaryHeap, HashSet};

pub const SHORT_WHEEL_NUM_BUCKETS: usize = 128;
pub const LONG_WHEEL_NUM_BUCKETS: usize = 4096;
pub const SHORT_WHEEL_RESOLUTION_MS: TimeT = 8;
pub const SHORT_WHEEL_PERIOD_MS: TimeT =
    SHORT_WHEEL_RESOLUTION_MS * SHORT_WHEEL_NUM_BUCKETS as TimeT;
pub const LONG_WHEEL_RESOLUTION_MS: TimeT = SHORT_WHEEL_PERIOD_MS;
pub const LONG_WHEEL_PERIOD_MS: TimeT = LONG_WHEEL_RESOLUTION_MS * LONG_WHEEL_NUM_BUCKETS as TimeT;

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

    pub fn remove(&mut self, timer: &Timer) -> bool {
        let index = self.bucket_index(timer.pop_time());
        self.buckets
            .get_mut(index)
            .expect("Expected bucket at index")
            .remove(timer)
    }

    fn bucket_index(&self, timestamp: TimeT) -> usize {
        ((timestamp / self.resolution) % self.buckets.len() as TimeT) as usize
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

impl Default for TimerHeap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::timer::TimerId;

    fn timer_at(pop_time: TimeT) -> Timer {
        Timer::new(TimerId::new(), 0, pop_time, None)
    }

    #[test]
    fn short_wheel_constants() {
        let wheel = Wheel::new_short_wheel();
        assert_eq!(wheel.resolution, SHORT_WHEEL_RESOLUTION_MS);
        assert_eq!(wheel.period, SHORT_WHEEL_PERIOD_MS);
        assert_eq!(wheel.buckets.len(), SHORT_WHEEL_NUM_BUCKETS);
    }

    #[test]
    fn long_wheel_constants() {
        let wheel = Wheel::new_long_wheel();
        assert_eq!(wheel.resolution, LONG_WHEEL_RESOLUTION_MS);
        assert_eq!(wheel.period, LONG_WHEEL_PERIOD_MS);
        assert_eq!(wheel.buckets.len(), LONG_WHEEL_NUM_BUCKETS);
    }

    #[test]
    fn bucket_index_wraps_modulo_buckets() {
        let wheel = Wheel::new_short_wheel();
        let buckets = SHORT_WHEEL_NUM_BUCKETS as TimeT;
        let res = SHORT_WHEEL_RESOLUTION_MS;

        assert_eq!(wheel.bucket_index(0), 0);
        assert_eq!(wheel.bucket_index(res), 1);
        assert_eq!(wheel.bucket_index(res * buckets), 0);
        assert_eq!(wheel.bucket_index(res * (buckets + 3)), 3);
    }

    #[test]
    fn round_timestamp_rounds_down_to_resolution() {
        let wheel = Wheel::new_short_wheel();
        let res = SHORT_WHEEL_RESOLUTION_MS;

        assert_eq!(wheel.round_timestamp(0), 0);
        assert_eq!(wheel.round_timestamp(res - 1), 0);
        assert_eq!(wheel.round_timestamp(res), res);
        assert_eq!(wheel.round_timestamp(res + 1), res);
        assert_eq!(wheel.round_timestamp(2 * res + 5), 2 * res);
    }

    #[test]
    fn should_insert_within_period() {
        let wheel = Wheel::new_short_wheel();
        let tick: TimeT = 0;

        // Just inside the period.
        assert!(wheel.should_insert(&tick, &timer_at(wheel.period - 1)));
        // Just outside the period (rounded equality fails the strict <).
        assert!(!wheel.should_insert(&tick, &timer_at(wheel.period)));
        assert!(!wheel.should_insert(&tick, &timer_at(wheel.period + wheel.resolution)));
    }

    #[test]
    fn insert_then_pop_returns_timer() {
        let mut wheel = Wheel::new_short_wheel();
        let timer = timer_at(wheel.resolution);

        wheel.insert(timer.clone());
        let popped = wheel.pop(wheel.resolution);

        assert_eq!(popped.len(), 1);
        assert!(popped.contains(&timer));
    }

    #[test]
    fn pop_empty_bucket_returns_empty() {
        let mut wheel = Wheel::new_short_wheel();
        let popped = wheel.pop(0);
        assert!(popped.is_empty());
    }

    #[test]
    fn remove_returns_true_then_false() {
        let mut wheel = Wheel::new_short_wheel();
        let timer = timer_at(wheel.resolution);

        wheel.insert(timer.clone());
        assert!(wheel.remove(&timer));
        assert!(!wheel.remove(&timer));
    }

    #[test]
    fn timer_heap_pops_in_min_pop_time_order() {
        let mut heap = TimerHeap::new();
        heap.push(timer_at(300));
        heap.push(timer_at(100));
        heap.push(timer_at(200));

        assert_eq!(heap.pop().unwrap().pop_time(), 100);
        assert_eq!(heap.pop().unwrap().pop_time(), 200);
        assert_eq!(heap.pop().unwrap().pop_time(), 300);
        assert!(heap.pop().is_none());
    }

    #[test]
    fn timer_heap_remove_buries_tombstoned_timer() {
        let mut heap = TimerHeap::new();
        let early = timer_at(100);
        let later = timer_at(200);
        heap.push(early.clone());
        heap.push(later.clone());

        assert!(heap.remove(&early));
        // Re-tombstoning the same timer is a no-op (HashSet::insert returns false).
        assert!(!heap.remove(&early));

        assert!(heap.peek().is_some_and(|t| *t == later));
        assert!(heap.pop().is_some_and(|t| t == later));
        assert!(heap.pop().is_none());
    }

    #[test]
    fn timer_heap_default_matches_new() {
        let mut heap = TimerHeap::default();
        assert!(heap.peek().is_none());
        assert!(heap.pop().is_none());
    }
}
