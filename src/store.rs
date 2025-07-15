use crate::{
    clock::{Clock, TimeT},
    timer::{Timer, TimerId},
    wheel::{Bucket, Wheel},
};
use std::{
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};

pub struct Store {
    clock: Arc<dyn Clock>,
    tick: TimeT,
    lookup: HashMap<TimerId, Timer>,
    short_wheel: Wheel,
    long_wheel: Wheel,
    overdue: Bucket,
    heap: BinaryHeap<Timer>,
}

impl Store {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        // Get the current time in milliseconds
        // Convert to short wheel resolution
        let short_wheel = Wheel::new_short_wheel();
        let tick = short_wheel.round_timestamp(clock.now());
        Self {
            clock,
            tick,
            lookup: HashMap::new(),
            short_wheel,
            long_wheel: Wheel::new_long_wheel(),
            overdue: Bucket::default(),
            heap: BinaryHeap::new(),
        }
    }

    pub fn insert(&mut self, timer: Timer) {
        if self.lookup.contains_key(&timer.id) {
            // TODO handle error
            panic!("A timer already exists with this ID.")
        }

        // TODO add timer to lookup table

        if timer.pop_time() < self.tick {
            // Timer is overdue.
            self.overdue.insert(timer);
        } else if self.short_wheel.should_insert(&self.tick, &timer) {
            // Timer belongs in short wheel.
            self.short_wheel.insert(timer);
        } else if self.long_wheel.should_insert(&self.tick, &timer) {
            // Timer belongs in long wheel.
            self.long_wheel.insert(timer);
        } else {
            // Timer is too far into the future, put it in the heap.
            self.heap.push(timer);
        }
    }

    pub fn pop(&mut self) -> Bucket {
        // Pop overdue timers regardless of whether we're processing new ticks.
        let mut timers = std::mem::take(&mut self.overdue);

        // Process ticks
        let current_timestamp = self.clock.now();
        let ticks = (current_timestamp - self.tick) / self.short_wheel.resolution;

        for _ in 0..ticks {
            // Pop timers in the current bucket.
            timers.extend(self.short_wheel.pop(self.tick));

            // Advance time and fill wheels.
            self.tick += self.short_wheel.resolution;
            self.fill_wheels();
        }

        timers
    }

    fn fill_wheels(&mut self) {
        // Refill the long wheel at the end of a long wheel period.
        if self.tick % self.long_wheel.period == 0 {
            self.fill_long_wheel();
        }

        // Refill the short wheel at the end of a short wheel period.
        // This is done second to the long wheel as timers may need to
        // propogate from heap -> long wheel -> short wheel.
        if self.tick % self.short_wheel.period == 0 {
            self.fill_short_wheel();
        }
    }

    fn fill_long_wheel(&mut self) {
        while let Some(next_timer) = self.heap.peek() {
            if next_timer.pop_time() < self.tick + self.long_wheel.period {
                if let Some(timer) = self.heap.pop() {
                    self.long_wheel.insert(timer);
                }
            } else {
                break;
            }
        }
    }

    fn fill_short_wheel(&mut self) {
        // Find the correct long wheel bucket based on current timestamp.
        // Take the timers out of this bucket and put them in the correct short wheel bucket.
        let bucket = self.long_wheel.pop(self.tick);
        bucket
            .into_iter()
            .for_each(|timer| self.short_wheel.insert(timer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{clock::tests::FakeClock, wheel};

    const TIMER_GRANULARITY_MS: TimeT = wheel::SHORT_WHEEL_RESOLUTION_MS;

    fn setup() -> (Arc<FakeClock>, Store) {
        let clock = Arc::new(FakeClock::new(0));
        let store = Store::new(clock.clone());
        (clock, store)
    }

    #[test]
    fn test_short_timer() {
        let (clock, mut store) = setup();
        store.insert(Timer::new(clock.now(), 100));

        clock.advance(100 - TIMER_GRANULARITY_MS);
        assert_eq!(0, store.pop().len());
        clock.advance(2 * TIMER_GRANULARITY_MS);
        assert_eq!(1, store.pop().len());
    }

    #[test]
    fn test_short_timer_with_offset() {
        let (clock, mut store) = setup();
        store.insert(Timer::new(clock.now(), 1600));

        clock.advance(1600 - TIMER_GRANULARITY_MS);
        assert_eq!(0, store.pop().len());
        clock.advance(2 * TIMER_GRANULARITY_MS);
        assert_eq!(1, store.pop().len());
    }

    #[test]
    fn test_really_long_timer() {
        let (clock, mut store) = setup();
        store.insert(Timer::new(clock.now(), 3600 * 1000 * 10));

        clock.advance(3600 * 1000 * 10 - TIMER_GRANULARITY_MS);
        assert_eq!(0, store.pop().len());
        clock.advance(2 * TIMER_GRANULARITY_MS);
        assert_eq!(1, store.pop().len());
    }

    #[test]
    fn test_multiple_really_long_timers() {
        let (clock, mut store) = setup();
        store.insert(Timer::new(clock.now(), 3600 * 1000 * 10));
        store.insert(Timer::new(clock.now(), 3600 * 1000 * 10));

        clock.advance(3600 * 1000 * 10 - TIMER_GRANULARITY_MS);
        assert_eq!(0, store.pop().len());
        clock.advance(2 * TIMER_GRANULARITY_MS);
        assert_eq!(2, store.pop().len());
    }

    #[test]
    fn test_overdue_timers() {
        let (clock, mut store) = setup();
        clock.advance(500);
        assert_eq!(0, store.pop().len());

        // Insert a timer set to pop in the past.
        store.insert(Timer::new(0, 100));
        assert_eq!(1, store.pop().len());
    }

    #[test]
    fn test_mixture_of_timer_lengths() {
        // Add timers that pop at the same time but add them in such a way that one is added in the heap,
        // one in the long wheel, and one in the short wheel.
        let (clock, mut store) = setup();

        // Timer 1 pops in 1h:0m:1s:500ms.
        store.insert(Timer::new(clock.now(), (60 * 60 * 1000) + (1 * 1000) + 500));

        // Advance by 1h, no timers have popped.
        clock.advance(60 * 60 * 1000);
        assert_eq!(0, store.pop().len());

        // Timer 2 pops in 1s:500ms.
        store.insert(Timer::new(clock.now(), (1 * 1000) + 500));

        // Advance by 1s, no timers have popped.
        clock.advance(1 * 1000);
        assert_eq!(0, store.pop().len());

        // Timer 3 pops in 500ms.
        store.insert(Timer::new(clock.now(), 500));

        // Advance by 500ms, all timers pop.
        clock.advance(500 + TIMER_GRANULARITY_MS);
        assert_eq!(3, store.pop().len());
    }
}
