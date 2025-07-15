use chrono::Utc;

/// The 64-bit unsigned integer type used to store time in ms.
pub type TimeT = usize;

pub trait Clock {
    fn now(&self) -> TimeT;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> TimeT {
        Utc::now().timestamp_millis() as TimeT
    }
}

#[cfg(test)]
pub mod tests {
    use crate::clock::{Clock, TimeT};
    use parking_lot::Mutex;

    pub struct FakeClock {
        current: Mutex<TimeT>,
    }

    impl FakeClock {
        pub fn new(start: TimeT) -> Self {
            Self {
                current: Mutex::new(start),
            }
        }

        pub fn advance(&self, duration: TimeT) {
            let mut guard = self.current.lock();
            *guard += duration;
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> TimeT {
            *self.current.lock()
        }
    }
}
