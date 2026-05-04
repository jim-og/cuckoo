use crate::{
    core::{TimeT, Timer, TimerEvent, TimerId},
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
    event_sender: Sender<TimerEvent>,
}

impl TimerHandler {
    pub fn new(event_sender: Sender<TimerEvent>) -> Self {
        Self { event_sender }
    }
}

#[derive(Deserialize)]
struct RequestPayload {
    interval_ms: u64,
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

        let event = TimerEvent::Insert(timer);

        self.event_sender.send(event).await.map_err(|_| {
            let mut resp = Response::new(full("Server overloaded"));
            *resp.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            resp
        })?;

        Ok(Response::new(full(format!("id: {}", id.0))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::Method;
    use http_body_util::BodyExt;
    use hyper::body::Bytes as HyperBytes;
    use tokio::sync::mpsc;

    fn request(method: Method, body: &[u8]) -> HttpRequest {
        HttpRequest {
            method,
            path: "/timer".to_string(),
            body: HyperBytes::copy_from_slice(body),
        }
    }

    async fn body_bytes(resp: HttpResponse) -> Bytes {
        resp.into_body().collect().await.unwrap().to_bytes()
    }

    #[tokio::test]
    async fn status_handler_returns_ok() {
        let handler = StatusHandler;
        let req = HttpRequest {
            method: Method::GET,
            path: "/".to_string(),
            body: HyperBytes::new(),
        };

        let resp = handler.handle(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_bytes(resp).await, Bytes::from("OK"));
    }

    #[tokio::test]
    async fn timer_handler_invalid_json_returns_bad_request() {
        let (sender, _receiver) = mpsc::channel::<TimerEvent>(1);
        let handler = TimerHandler::new(sender);

        let resp = handler
            .handle(request(Method::POST, b"not json"))
            .await
            .unwrap_err();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_bytes(resp).await, Bytes::from("Invalid JSON"));
    }

    #[tokio::test]
    async fn timer_handler_dropped_receiver_returns_service_unavailable() {
        let (sender, receiver) = mpsc::channel::<TimerEvent>(1);
        drop(receiver);
        let handler = TimerHandler::new(sender);

        let resp = handler
            .handle(request(Method::POST, br#"{"interval_ms": 100}"#))
            .await
            .unwrap_err();

        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body_bytes(resp).await, Bytes::from("Server overloaded"));
    }

    #[tokio::test]
    async fn timer_handler_happy_path_sends_event_and_returns_id() {
        let (sender, mut receiver) = mpsc::channel::<TimerEvent>(1);
        let handler = TimerHandler::new(sender);

        let resp = handler
            .handle(request(Method::POST, br#"{"interval_ms": 100}"#))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let event = receiver.recv().await.unwrap();
        let TimerEvent::Insert(timer) = event;

        let body = body_bytes(resp).await;
        let expected = format!("id: {}", timer.id.0);
        assert_eq!(body, Bytes::from(expected));
    }
}
