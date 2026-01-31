use crate::utils::{
    HttpRequest,
    http::{HttpResponse, full},
};
use http::{Method, StatusCode};
use hyper::Response;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

pub trait RouteHandler: Send + Sync + 'static {
    fn handle(&self, req: HttpRequest) -> Result<HttpResponse, HttpResponse>;
}

#[derive(Clone, Eq)]
pub struct RouteKey {
    pub method: Method,
    pub path: String,
}

impl PartialEq for RouteKey {
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method && self.path == other.path
    }
}

impl Hash for RouteKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.path.hash(state);
    }
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

    pub fn route(&self, req: HttpRequest) -> HttpResponse {
        let key = (&req).into();
        match self.routes.get(&key) {
            Some(handler) => match handler.handle(req) {
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
