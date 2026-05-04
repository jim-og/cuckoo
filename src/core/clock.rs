use crate::core::timer::TimeT;
use chrono::Utc;

pub trait Clock: Send + Sync {
    fn now(&self) -> TimeT;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> TimeT {
        Utc::now().timestamp_millis() as TimeT
    }
}
