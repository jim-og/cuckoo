use crate::{
    clock::{Clock, SystemClock},
    store::Store,
    timer::{Timer, TimerId},
};
use core::time;
use std::{sync::Arc, thread::sleep};

mod clock;
mod server;
mod store;
mod timer;
mod wheel;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let clock = Arc::new(SystemClock {});
    let mut store = Store::new(clock.clone());

    let timer_1 = Timer::new(TimerId::new(), clock.now(), 1000);
    store.insert(timer_1);

    let timer_2_id = TimerId::new();
    let timer_2 = Timer::new(timer_2_id.clone(), clock.now(), 2000);
    store.insert(timer_2);

    sleep(time::Duration::from_millis(
        1000 + wheel::SHORT_WHEEL_RESOLUTION_MS as u64,
    ));

    store
        .pop()
        .iter()
        .for_each(|timer| println!("timer: {}", timer.id.uuid()));

    store.remove(&timer_2_id);

    server::run_server().await
}
