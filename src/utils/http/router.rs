use crate::utils::{
    HttpRequest,
    http::{HttpResponse, full},
};
use async_trait::async_trait;
use http::{Method, StatusCode};
use hyper::Response;
use std::{collections::HashMap, hash::Hash};

#[async_trait]
pub trait RouteHandler: Send + Sync + 'static {
    async fn handle(&self, req: HttpRequest) -> Result<HttpResponse, HttpResponse>;
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct RouteKey {
    pub method: Method,
    pub path: String,
}

impl From<&HttpRequest> for RouteKey {
    fn from(req: &HttpRequest) -> Self {
        RouteKey {
            method: req.method.clone(),
            path: req.path.clone(),
        }
    }
}

pub struct Router {
    routes: HashMap<RouteKey, Box<dyn RouteHandler>>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn add<H>(mut self, method: Method, path: impl Into<String>, handler: H) -> Self
    where
        H: RouteHandler,
    {
        self.routes.insert(
            RouteKey {
                method,
                path: path.into(),
            },
            Box::new(handler),
        );
        self
    }

    pub async fn route(&self, req: HttpRequest) -> HttpResponse {
        let key = (&req).into();
        match self.routes.get(&key) {
            Some(handler) => match handler.handle(req).await {
                Ok(resp) | Err(resp) => resp,
            },
            None => {
                let mut resp = Response::new(full("Not found"));
                *resp.status_mut() = StatusCode::NOT_FOUND;
                resp
            }
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::http::full;
    use bytes::Bytes;
    use http_body_util::BodyExt;
    use hyper::body::Bytes as HyperBytes;

    struct OkHandler;

    #[async_trait]
    impl RouteHandler for OkHandler {
        async fn handle(&self, _req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
            Ok(Response::new(full("ok")))
        }
    }

    struct ErrHandler;

    #[async_trait]
    impl RouteHandler for ErrHandler {
        async fn handle(&self, _req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
            let mut resp = Response::new(full("nope"));
            *resp.status_mut() = StatusCode::IM_A_TEAPOT;
            Err(resp)
        }
    }

    fn request(method: Method, path: &str) -> HttpRequest {
        HttpRequest {
            method,
            path: path.to_string(),
            body: HyperBytes::new(),
        }
    }

    async fn body_bytes(resp: HttpResponse) -> Bytes {
        resp.into_body().collect().await.unwrap().to_bytes()
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let router = Router::new();
        let resp = router.route(request(Method::GET, "/missing")).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(body_bytes(resp).await, Bytes::from("Not found"));
    }

    #[tokio::test]
    async fn default_router_matches_new() {
        let router = Router::default();
        let resp = router.route(request(Method::GET, "/")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn method_mismatch_returns_404() {
        let router = Router::new().add(Method::GET, "/x", OkHandler);

        let resp = router.route(request(Method::POST, "/x")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn matching_route_invokes_handler_ok() {
        let router = Router::new().add(Method::GET, "/x", OkHandler);

        let resp = router.route(request(Method::GET, "/x")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_bytes(resp).await, Bytes::from("ok"));
    }

    #[tokio::test]
    async fn handler_err_response_is_returned() {
        let router = Router::new().add(Method::GET, "/x", ErrHandler);

        let resp = router.route(request(Method::GET, "/x")).await;
        assert_eq!(resp.status(), StatusCode::IM_A_TEAPOT);
        assert_eq!(body_bytes(resp).await, Bytes::from("nope"));
    }
}
