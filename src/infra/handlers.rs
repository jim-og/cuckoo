use crate::{
    core::{TimeT, Timer, TimerId, TimerServiceEvent},
    utils::{HttpRequest, HttpResponse, RouteHandler, full},
};
use async_trait::async_trait;
use chrono::Utc;
use http::{Response, StatusCode};
use serde::Deserialize;
use tokio::sync::mpsc::Sender;

pub struct StatusHandler;

#[async_trait]
impl RouteHandler for StatusHandler {
    async fn handle(&self, _req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
        Ok(Response::new(full("OK")))
    }
}

pub struct TimerHandler {
    event_sender: Sender<TimerServiceEvent>,
}

impl TimerHandler {
    pub fn new(event_sender: Sender<TimerServiceEvent>) -> Self {
        Self { event_sender }
    }
}

#[derive(Deserialize)]
struct RequestPayload {
    interval_ms: usize,
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
            Utc::now().timestamp_millis() as TimeT,
            payload.interval_ms,
        );

        let event = TimerServiceEvent::Insert(timer);

        self.event_sender.send(event).await.map_err(|_| {
            let mut resp = Response::new(full("Server overloaded"));
            *resp.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            resp
        })?;

        Ok(Response::new(full(format!("id: {}", id.0))))
    }
}
