use chrono::Utc;

/// The 64-bit unsigned integer type used to store time in ms.
pub type TimeT = usize;

pub trait Clock {
    fn now(&self) -> TimeT;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> TimeT {
        Utc::now().timestamp_millis() as usize
    }
}

pub struct FakeClock {
    current: TimeT,
}

impl FakeClock {
    pub fn new(start: TimeT) -> Self {
        Self { current: start }
    }

    pub fn advance(&mut self, duration: TimeT) {
        self.current += duration;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> TimeT {
        self.current
    }
}
