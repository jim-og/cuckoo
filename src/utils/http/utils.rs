use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::Response;
use hyper::body::Bytes;
use tokio::sync::mpsc;

pub type RequestSender = mpsc::Sender<HttpRequest>;

pub struct HttpRequest {
    pub method: http::Method,
    pub path: String,
    pub body: Bytes,
}

pub type HttpResponse = Response<BoxBody<Bytes, hyper::Error>>;

/// Helper function to create a Full body.
pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}
