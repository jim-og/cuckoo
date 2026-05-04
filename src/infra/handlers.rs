use crate::{
    core::{Clock, Timer, TimerEvent, TimerId},
    utils::{HttpRequest, HttpResponse, RouteHandler, full},
};
use async_trait::async_trait;
use http::{Response, StatusCode};
use serde::Deserialize;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::mpsc::Sender;

pub struct StatusHandler;

#[async_trait]
impl RouteHandler for StatusHandler {
    async fn handle(&self, _req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
        Ok(Response::new(full("OK")))
    }
}

pub struct StatsHandler {
    active_count: Arc<AtomicUsize>,
    event_sender: Sender<TimerEvent>,
    timer_sender: Sender<Timer>,
}

impl StatsHandler {
    pub fn new(
        active_count: Arc<AtomicUsize>,
        event_sender: Sender<TimerEvent>,
        timer_sender: Sender<Timer>,
    ) -> Self {
        Self {
            active_count,
            event_sender,
            timer_sender,
        }
    }
}

#[async_trait]
impl RouteHandler for StatsHandler {
    async fn handle(&self, _req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
        let body = serde_json::json!({
            "active_timer_count": self.active_count.load(Ordering::Relaxed),
            "event_channel_capacity_remaining": self.event_sender.capacity(),
            "fired_channel_capacity_remaining": self.timer_sender.capacity(),
        });
        Ok(Response::new(full(body.to_string())))
    }
}

pub struct TimerHandler {
    event_sender: Sender<TimerEvent>,
    clock: Arc<dyn Clock>,
}

impl TimerHandler {
    pub fn new(event_sender: Sender<TimerEvent>, clock: Arc<dyn Clock>) -> Self {
        Self {
            event_sender,
            clock,
        }
    }
}

#[derive(Deserialize)]
struct RequestPayload {
    interval_ms: u64,
    callback_url: Option<String>,
}

#[async_trait]
impl RouteHandler for TimerHandler {
    async fn handle(&self, req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
        let payload = serde_json::from_slice::<RequestPayload>(&req.body).map_err(|_| {
            let mut resp = Response::new(full("Invalid JSON"));
            *resp.status_mut() = StatusCode::BAD_REQUEST;
            resp
        })?;

        let id = TimerId::new();
        let timer = Timer::new(
            id.to_owned(),
            self.clock.now(),
            payload.interval_ms,
            payload.callback_url,
        );

        let event = TimerEvent::Insert(timer);

        self.event_sender.send(event).await.map_err(|_| {
            let mut resp = Response::new(full("Server overloaded"));
            *resp.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            resp
        })?;

        Ok(Response::new(full(format!("id: {}", id.0))))
    }
}
