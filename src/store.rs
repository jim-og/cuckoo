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

// test min-heap structure
