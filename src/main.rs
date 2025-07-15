use crate::{
    clock::{Clock, SystemClock},
    store::Store,
    timer::Timer,
};
use core::time;
use std::{sync::Arc, thread::sleep};

mod clock;
mod store;
mod timer;
mod wheel;

fn main() {
    let clock = Arc::new(SystemClock {});
    let mut store = Store::new(clock.clone());
    let timer = Timer::new(clock.now(), 1000);

    store.insert(timer);

    sleep(time::Duration::from_millis(
        1000 + wheel::SHORT_WHEEL_RESOLUTION_MS as u64,
    ));

    let timers = store.pop();
    timers
        .iter()
        .for_each(|timer| println!("timer: {}", timer.id));
}
