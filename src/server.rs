use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Body, Bytes, Frame, Incoming};
use hyper::server::conn::http1;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use std::net::SocketAddr;
use std::u64;
use tokio::net::TcpListener;
use tower::{Service, ServiceBuilder};

#[derive(Debug, Clone)]
pub struct Logger<S> {
    inner: S,
}
impl<S> Logger<S> {
    pub fn new(inner: S) -> Self {
        Logger { inner }
    }
}
type Req = Request<Incoming>;

impl<S> Service<Req> for Logger<S>
where
    S: Service<Req> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&mut self, req: Req) -> Self::Future {
        println!("processing request: {} {}", req.method(), req.uri().path());
        self.inner.call(req)
    }

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
}

async fn echo(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(full("Try POSTing data to /echo"))),
        (&Method::POST, "/echo") => Ok(Response::new(req.into_body().boxed())),
        (&Method::POST, "/echo/uppercase") => {
            // Convert every byte in every Data frame to uppercase
            let frame_stream = req.into_body().map_frame(|frame| {
                let data = match frame.into_data() {
                    Ok(data) => data.iter().map(|byte| byte.to_ascii_uppercase()).collect(),
                    Err(_) => Bytes::new(),
                };

                Frame::data(data)
            });

            Ok(Response::new(frame_stream.boxed()))
        }
        (&Method::POST, "/echo/reversed") => {
            // Set an upper bound of 64kb for body size
            let upper_bound = req.body().size_hint().upper().unwrap_or(u64::MAX);
            if upper_bound > 1024 * 64 {
                let mut resp = Response::new(full("Body too big"));
                *resp.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
                return Ok(resp);
            }

            // Await the whole body to be collected.
            let body = req.collect().await?.to_bytes();

            // Reverse the body and respond.
            let reversed_body = body.iter().rev().cloned().collect::<Vec<u8>>();

            Ok(Response::new(full(reversed_body)))
        }
        _ => {
            // Return 404 Not Found for other routes.
            let mut not_found = Response::new(empty());
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

/// Helper function to create an Empty body.
fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::new().map_err(|never| match never {}).boxed()
}

/// Helper function to create a Full body.
fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

/// In the event of an error, returns a heap-allocatedw thread-safe error of any type that implements Error.
pub async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Address of localhost on port 3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // Create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;
    println!("Listening on http://{}", addr);

    // Start an event loop to continuously accept incoming connections
    loop {
        let (stream, _remote_addr) = listener.accept().await?;
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            // Bind the incoming connection to our `echo` service
            let echo_svc = tower::service_fn(echo);
            let tower_svc = ServiceBuilder::new()
                .layer_fn(Logger::new)
                .service(echo_svc);
            let svc = TowerToHyperService::new(tower_svc);

            if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                eprint!("Error serving connection: {:?}", err);
            }
        });
    }
}
