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
