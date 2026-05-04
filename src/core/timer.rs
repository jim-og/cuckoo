use std::cmp::Ordering;
use uuid::Uuid;

pub type TimeT = u64;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn pop_time_is_start_plus_interval() {
        let timer = Timer::new(TimerId::new(), 100, 250);
        assert_eq!(timer.pop_time(), 350);
    }

    #[test]
    fn timer_id_new_returns_distinct_ids() {
        let a = TimerId::new();
        let b = TimerId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn timer_id_default_returns_distinct_ids() {
        let a = TimerId::default();
        let b = TimerId::default();
        assert_ne!(a, b);
    }

    #[test]
    fn timer_id_uuid_returns_inner() {
        let id = TimerId::new();
        assert_eq!(id.uuid(), id.0);
    }

    #[test]
    fn timer_id_round_trips_through_hashset() {
        let id = TimerId::new();
        let mut set = HashSet::new();
        set.insert(id.clone());
        assert!(set.contains(&id));
    }

    #[test]
    fn ord_reverses_pop_time_for_min_heap() {
        let earlier = Timer::new(TimerId::new(), 0, 100);
        let later = Timer::new(TimerId::new(), 0, 200);

        // The smaller pop_time should compare as Greater so a max-heap (BinaryHeap)
        // pops it first — i.e. the heap behaves as a min-heap on pop_time.
        assert_eq!(earlier.cmp(&later), Ordering::Greater);
        assert_eq!(later.cmp(&earlier), Ordering::Less);
        assert_eq!(earlier.partial_cmp(&later), Some(Ordering::Greater));
    }
}
