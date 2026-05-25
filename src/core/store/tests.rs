use super::*;
use crate::core::wheel;
use test_case::test_case;

const TIMER_GRANULARITY_MS: TimeT = wheel::SHORT_WHEEL_RESOLUTION_MS;

struct FakeClock(TimeT);

impl FakeClock {
    fn new(start: TimeT) -> Self {
        Self(start)
    }

    fn now(&self) -> TimeT {
        self.0
    }

    fn advance(&mut self, duration: TimeT) {
        self.0 += duration;
    }
}

fn setup() -> (FakeClock, Store) {
    let clock = FakeClock::new(0);
    let store = Store::new(clock.now());
    (clock, store)
}

#[test_case(100; "short")]
#[test_case(1600; "long")]
#[test_case(3600 * 1000 * 10; "really_long")]
fn timer_pops_after_interval(interval: TimeT) {
    let (mut clock, mut store) = setup();
    store.insert(Timer::new(TimerId::new(), clock.now(), interval, None));

    clock.advance(interval - TIMER_GRANULARITY_MS);
    assert_eq!(0, store.pop(clock.now()).len());

    clock.advance(2 * TIMER_GRANULARITY_MS);
    assert_eq!(1, store.pop(clock.now()).len());
}

#[test_case(100; "short")]
#[test_case(1600; "long")]
#[test_case(3600 * 1000 * 10; "really_long")]
fn multiple_timers_pop(interval: TimeT) {
    let (mut clock, mut store) = setup();
    store.insert(Timer::new(TimerId::new(), clock.now(), interval, None));
    store.insert(Timer::new(TimerId::new(), clock.now(), interval, None));

    clock.advance(interval - TIMER_GRANULARITY_MS);
    assert_eq!(0, store.pop(clock.now()).len());

    clock.advance(2 * TIMER_GRANULARITY_MS);
    assert_eq!(2, store.pop(clock.now()).len());
}

#[test_case(100; "short")]
#[test_case(1600; "long")]
#[test_case(3600 * 1000 * 10; "really_long")]
fn timer_removal_after_interval(interval: TimeT) {
    let (mut clock, mut store) = setup();
    let id = TimerId::new();
    store.insert(Timer::new(id.clone(), clock.now(), interval, None));

    store.remove(&id);

    clock.advance(interval + TIMER_GRANULARITY_MS);
    assert_eq!(0, store.pop(clock.now()).len());
}

#[test]
fn overdue_timer_pop() {
    let (mut clock, mut store) = setup();

    clock.advance(500);
    assert_eq!(0, store.pop(clock.now()).len());

    // Insert a timer set to pop in the past.
    store.insert(Timer::new(TimerId::new(), 0, 100, None));
    assert_eq!(1, store.pop(clock.now()).len());
}

#[test]
fn overdue_timer_removal() {
    let (mut clock, mut store) = setup();

    clock.advance(500);
    assert_eq!(0, store.pop(clock.now()).len());

    // Insert a timer set to pop in the past.
    let id = TimerId::new();
    store.insert(Timer::new(id.clone(), 0, 100, None));

    store.remove(&id);
    assert_eq!(0, store.pop(clock.now()).len());
}

#[test]
fn mixture_of_timer_lengths_pop() {
    // Add timers that pop at the same time but span heap, long wheel, and short wheel.
    let (mut clock, mut store) = setup();

    // Timer 1 pops in 1h + 1s + 500ms.
    store.insert(Timer::new(
        TimerId::new(),
        clock.now(),
        (60 * 60 * 1000) + 1000 + 500,
        None,
    ));

    // Advance by 1h, no timers have popped.
    clock.advance(60 * 60 * 1000);
    assert_eq!(0, store.pop(clock.now()).len());

    // Timer 2 pops in 1s + 500ms.
    store.insert(Timer::new(TimerId::new(), clock.now(), 1000 + 500, None));

    // Advance by 1s, no timers have popped.
    clock.advance(1000);
    assert_eq!(0, store.pop(clock.now()).len());

    // Timer 3 pops in 500ms.
    store.insert(Timer::new(TimerId::new(), clock.now(), 500, None));

    // Advance by 500ms + one tick, all timers pop.
    clock.advance(500 + TIMER_GRANULARITY_MS);
    assert_eq!(3, store.pop(clock.now()).len());
}

#[test]
fn next_deadline_empty_store() {
    let (_clock, store) = setup();
    assert_eq!(None, store.next_deadline());
}

#[test]
fn next_deadline_with_future_timer() {
    let (clock, mut store) = setup();
    store.insert(Timer::new(TimerId::new(), clock.now(), 1000, None));
    assert_eq!(Some(TIMER_GRANULARITY_MS), store.next_deadline());
}

#[test]
fn next_deadline_with_overdue_timer() {
    let (mut clock, mut store) = setup();
    clock.advance(500);
    // Advance the store's internal tick so the next insert lands in overdue.
    store.pop(clock.now());
    store.insert(Timer::new(TimerId::new(), 0, 100, None));
    assert_eq!(Some(store.tick), store.next_deadline());
}

#[test]
fn next_deadline_after_pop_drains_wheels() {
    let (mut clock, mut store) = setup();
    store.insert(Timer::new(TimerId::new(), clock.now(), 100, None));

    clock.advance(100 + TIMER_GRANULARITY_MS);
    assert_eq!(1, store.pop(clock.now()).len());

    assert_eq!(None, store.next_deadline());
}

#[test]
fn heap_timers_pop() {
    // Test that the timer which is next to pop is at the top of the heap.
    let (mut clock, mut store) = setup();

    let id_1 = TimerId::new();
    let id_2 = TimerId::new();
    let id_3 = TimerId::new();

    // timer_2 pops first, timer_3 next, timer_1 last.
    let timer_2_interval = 3600 * 1000 + TIMER_GRANULARITY_MS * 4;

    store.insert(Timer::new(
        id_1,
        clock.now(),
        3600 * 1000 * 10 + TIMER_GRANULARITY_MS * 2,
        None,
    ));
    store.insert(Timer::new(
        id_2.clone(),
        clock.now(),
        timer_2_interval,
        None,
    ));
    store.insert(Timer::new(
        id_3,
        clock.now(),
        3600 * 1000 * 5 + TIMER_GRANULARITY_MS * 6,
        None,
    ));

    clock.advance(timer_2_interval + TIMER_GRANULARITY_MS);
    let timers = store.pop(clock.now());
    assert_eq!(1, timers.len());

    if let Some(timer) = timers.iter().next() {
        assert_eq!(timer.id, id_2);
    } else {
        panic!("Expected there to be a popped timer.");
    }
}

#[test]
fn remove_unknown_id_returns_false() {
    let (_clock, mut store) = setup();
    assert!(!store.remove(&TimerId::new()));
}

#[test]
fn remove_already_removed_id_returns_false() {
    let (clock, mut store) = setup();
    let id = TimerId::new();
    store.insert(Timer::new(id.clone(), clock.now(), 100, None));

    assert!(store.remove(&id));
    assert!(!store.remove(&id));
}
