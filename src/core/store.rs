use crate::core::{
    timer::{TimeT, Timer, TimerId},
    wheel::{Bucket, TimerHeap, Wheel},
};
use std::collections::HashMap;

pub struct Store {
    tick: TimeT,
    lookup: HashMap<TimerId, Timer>,
    short_wheel: Wheel,
    long_wheel: Wheel,
    overdue: Bucket,
    heap: TimerHeap,
}

impl Store {
    pub fn new(now: TimeT) -> Self {
        let short_wheel = Wheel::new_short_wheel();
        let tick = short_wheel.round_timestamp(now);
        Self {
            tick,
            lookup: HashMap::new(),
            short_wheel,
            long_wheel: Wheel::new_long_wheel(),
            overdue: Bucket::default(),
            heap: TimerHeap::new(),
        }
    }

    pub fn insert(&mut self, timer: Timer) {
        // Add timer ID and pop time to lookup.
        if self
            .lookup
            .insert(timer.id.clone(), timer.clone())
            .is_some()
        {
            // TODO handle error
            panic!("A timer already exists with this ID.");
        }

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

    pub fn pop(&mut self, now: TimeT) -> Bucket {
        // Pop overdue timers regardless of whether we're processing new ticks.
        let mut timers = std::mem::take(&mut self.overdue);

        // Process ticks
        let ticks = (now - self.tick) / self.short_wheel.resolution;

        for _ in 0..ticks {
            // Pop timers in the current bucket.
            let popped_timers = self.short_wheel.pop(self.tick);

            // Remove from lookup.
            popped_timers.iter().for_each(|timer| {
                if self.lookup.remove(&timer.id).is_none() {
                    panic!("Timer does not exist in lookup.")
                }
            });

            timers.extend(popped_timers);

            // Advance time and fill wheels.
            self.tick += self.short_wheel.resolution;
            self.fill_wheels();
        }

        timers
    }

    pub fn remove(&mut self, id: &TimerId) -> bool {
        // Remove from lookup.
        if let Some(timer) = self.lookup.remove(id) {
            // Remove from bucket.
            if timer.pop_time() < self.tick {
                // Timer is overdue.
                self.overdue.remove(&timer)
            } else if self.short_wheel.should_insert(&self.tick, &timer) {
                // Timer lives in short wheel.
                self.short_wheel.remove(&timer)
            } else if self.long_wheel.should_insert(&self.tick, &timer) {
                // Timer lives in long wheel.
                self.long_wheel.remove(&timer)
            } else {
                // Timer lives in heap.
                self.heap.remove(&timer)
            }
        } else {
            eprintln!("Timer does not exist in lookup.");
            false
        }
    }

    fn fill_wheels(&mut self) {
        // Refill the long wheel at the end of a long wheel period.
        if self.tick.is_multiple_of(self.long_wheel.period) {
            self.fill_long_wheel();
        }

        // Refill the short wheel at the end of a short wheel period.
        // This is done second to the long wheel as timers may need to
        // propogate from heap -> long wheel -> short wheel.
        if self.tick.is_multiple_of(self.short_wheel.period) {
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
    use crate::core::wheel;
    use test_case::test_case;

    const G: TimeT = wheel::SHORT_WHEEL_RESOLUTION_MS;

    #[test_case(100; "short")]
    #[test_case(1600; "long")]
    #[test_case(3600 * 1000 * 10; "really_long")]
    fn timer_pops_after_interval(interval: TimeT) {
        let mut store = Store::new(0);
        store.insert(Timer::new(TimerId::new(), 0, interval));

        assert_eq!(0, store.pop(interval - G).len());
        assert_eq!(1, store.pop(interval + G).len());
    }

    #[test_case(100; "short")]
    #[test_case(1600; "long")]
    #[test_case(3600 * 1000 * 10; "really_long")]
    fn multiple_timers_pop(interval: TimeT) {
        let mut store = Store::new(0);
        store.insert(Timer::new(TimerId::new(), 0, interval));
        store.insert(Timer::new(TimerId::new(), 0, interval));

        assert_eq!(0, store.pop(interval - G).len());
        assert_eq!(2, store.pop(interval + G).len());
    }

    #[test_case(100; "short")]
    #[test_case(1600; "long")]
    #[test_case(3600 * 1000 * 10; "really_long")]
    fn timer_removal_after_interval(interval: TimeT) {
        let mut store = Store::new(0);
        let id = TimerId::new();
        store.insert(Timer::new(id.clone(), 0, interval));

        store.remove(&id);

        assert_eq!(0, store.pop(interval + G).len());
    }

    #[test]
    fn overdue_timer_pop() {
        let mut store = Store::new(0);
        assert_eq!(0, store.pop(500).len());

        // Insert a timer set to pop in the past.
        store.insert(Timer::new(TimerId::new(), 0, 100));
        assert_eq!(1, store.pop(500).len());
    }

    #[test]
    fn overdue_timer_removal() {
        let mut store = Store::new(0);
        assert_eq!(0, store.pop(500).len());

        // Insert a timer set to pop in the past.
        let id = TimerId::new();
        store.insert(Timer::new(id.clone(), 0, 100));

        store.remove(&id);
        assert_eq!(0, store.pop(500).len());
    }

    #[test]
    fn mixture_of_timer_lengths_pop() {
        // Add timers that pop at the same time but span heap, long wheel, and short wheel.
        let mut store = Store::new(0);

        // Timer 1 pops at 1h + 1s + 500ms.
        store.insert(Timer::new(TimerId::new(), 0, (60 * 60 * 1000) + 1000 + 500));

        // Advance by 1h, no timers have popped.
        assert_eq!(0, store.pop(60 * 60 * 1000).len());

        // Timer 2 pops in 1s + 500ms from now (= same absolute time as timer 1).
        store.insert(Timer::new(TimerId::new(), 60 * 60 * 1000, 1000 + 500));

        // Advance by 1s, no timers have popped.
        assert_eq!(0, store.pop(60 * 60 * 1000 + 1000).len());

        // Timer 3 pops in 500ms from now (= same absolute time).
        store.insert(Timer::new(TimerId::new(), 60 * 60 * 1000 + 1000, 500));

        // Advance by 500ms + one tick, all timers pop.
        assert_eq!(3, store.pop(60 * 60 * 1000 + 1000 + 500 + G).len());
    }

    #[test]
    fn heap_timers_pop() {
        // Test that the timer which is next to pop is at the top of the heap.
        let mut store = Store::new(0);

        let id_1 = TimerId::new();
        let id_2 = TimerId::new();
        let id_3 = TimerId::new();

        // timer_2 pops first, timer_3 next, timer_1 last.
        let timer_2_interval = 3600 * 1000 + G * 4;

        store.insert(Timer::new(id_1, 0, 3600 * 1000 * 10 + G * 2));
        store.insert(Timer::new(id_2.clone(), 0, timer_2_interval));
        store.insert(Timer::new(id_3, 0, 3600 * 1000 * 5 + G * 6));

        let timers = store.pop(timer_2_interval + G);
        assert_eq!(1, timers.len());

        if let Some(timer) = timers.iter().next() {
            assert_eq!(timer.id, id_2);
        } else {
            panic!("Expected there to be a popped timer.");
        }
    }
}
