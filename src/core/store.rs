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

    pub fn next_deadline(&self) -> Option<TimeT> {
        if !self.overdue.is_empty() {
            return Some(self.tick);
        }
        if self.lookup.is_empty() {
            return None;
        }
        Some(self.tick + self.short_wheel.resolution)
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
mod tests;
