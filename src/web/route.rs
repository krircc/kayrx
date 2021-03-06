use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use crate::http::{Method, error::Error, Response as HttpResponse};
use crate::service::{Service, ServiceFactory};
use futures_util::future::{ready, FutureExt, LocalBoxFuture};

use crate::web::extract::FromRequest;
use crate::web::guard::{self, Guard};
use crate::web::handler::{Extract, Factory, Handler};
use crate::web::responder::Responder;
use crate::web::service::{ServiceRequest, ServiceResponse};

type BoxedRouteService<Req, Res> = Box<
    dyn Service<
        Request = Req,
        Response = Res,
        Error = Error,
        Future = LocalBoxFuture<'static, Result<Res, Error>>,
    >,
>;

type BoxedRouteNewService<Req, Res> = Box<
    dyn ServiceFactory<
        Config = (),
        Request = Req,
        Response = Res,
        Error = Error,
        InitError = (),
        Service = BoxedRouteService<Req, Res>,
        Future = LocalBoxFuture<'static, Result<BoxedRouteService<Req, Res>, ()>>,
    >,
>;

/// Resource route definition
///
/// Route uses builder-like pattern for configuration.
/// If handler is not explicitly set, default *404 Not Found* handler is used.
pub struct Route {
    service: BoxedRouteNewService<ServiceRequest, ServiceResponse>,
    guards: Rc<Vec<Box<dyn Guard>>>,
}

impl Route {
    /// Create new route which matches any request.
    pub fn new() -> Route {
        Route {
            service: Box::new(RouteNewService::new(Extract::new(Handler::new(|| {
                ready(HttpResponse::NotFound())
            })))),
            guards: Rc::new(Vec::new()),
        }
    }

    pub(crate) fn take_guards(&mut self) -> Vec<Box<dyn Guard>> {
        std::mem::replace(Rc::get_mut(&mut self.guards).unwrap(), Vec::new())
    }
}

impl ServiceFactory for Route {
    type Config = ();
    type Request = ServiceRequest;
    type Response = ServiceResponse;
    type Error = Error;
    type InitError = ();
    type Service = RouteService;
    type Future = CreateRouteService;

    fn new_service(&self, _: ()) -> Self::Future {
        CreateRouteService {
            fut: self.service.new_service(()),
            guards: self.guards.clone(),
        }
    }
}

type RouteFuture = LocalBoxFuture<
    'static,
    Result<BoxedRouteService<ServiceRequest, ServiceResponse>, ()>,
>;

#[pin_project::pin_project]
pub struct CreateRouteService {
    #[pin]
    fut: RouteFuture,
    guards: Rc<Vec<Box<dyn Guard>>>,
}

impl Future for CreateRouteService {
    type Output = Result<RouteService, ()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.fut.poll(cx)? {
            Poll::Ready(service) => Poll::Ready(Ok(RouteService {
                service,
                guards: this.guards.clone(),
            })),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct RouteService {
    service: BoxedRouteService<ServiceRequest, ServiceResponse>,
    guards: Rc<Vec<Box<dyn Guard>>>,
}

impl RouteService {
    pub fn check(&self, req: &mut ServiceRequest) -> bool {
        for f in self.guards.iter() {
            if !f.check(req.head()) {
                return false;
            }
        }
        true
    }
}

impl Service for RouteService {
    type Request = ServiceRequest;
    type Response = ServiceResponse;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        self.service.call(req).boxed_local()
    }
}

impl Route {
    /// Add method guard to the route.
    ///
    /// ```rust
    /// # use kayrx::web::{self, App, HttpRequest, HttpResponse};
    /// # use kayrx::http;
    /// # fn main() {
    /// App::new().service(web::resource("/path").route(
    ///     web::get()
    ///         .method(http::Method::CONNECT)
    ///         .guard(guard::Header("content-type", "text/plain"))
    ///         .to(|req: HttpRequest| HttpResponse::Ok()))
    /// );
    /// # }
    /// ```
    pub fn method(mut self, method: Method) -> Self {
        Rc::get_mut(&mut self.guards)
            .unwrap()
            .push(Box::new(guard::Method(method)));
        self
    }

    /// Add guard to the route.
    ///
    /// ```rust
    /// # use kayrx::web::{self, App, HttpRequest, HttpResponse};
    /// # fn main() {
    /// App::new().service(web::resource("/path").route(
    ///     web::route()
    ///         .guard(guard::Get())
    ///         .guard(guard::Header("content-type", "text/plain"))
    ///         .to(|req: HttpRequest| HttpResponse::Ok()))
    /// );
    /// # }
    /// ```
    pub fn guard<F: Guard + 'static>(mut self, f: F) -> Self {
        Rc::get_mut(&mut self.guards).unwrap().push(Box::new(f));
        self
    }

    /// Set handler function, use request extractors for parameters.
    ///
    /// ```rust
    /// use kayrx::web::{self, types, App};
    /// use kayrx::http;
    /// use serde_derive::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Info {
    ///     username: String,
    /// }
    ///
    /// /// extract path info using serde
    /// async fn index(info: types::Path<Info>) -> String {
    ///     format!("Welcome {}!", info.username)
    /// }
    ///
    /// fn main() {
    ///     let app = App::new().service(
    ///         web::resource("/{username}/index.html") // <- define path parameters
    ///             .route(web::get().to(index))        // <- register handler
    ///     );
    /// }
    /// ```
    ///
    /// It is possible to use multiple extractors for one handler function.
    ///
    /// ```rust
    /// # use std::collections::HashMap;
    /// # use serde_derive::Deserialize;
    /// use kayrx::web::{self, types, App};
    ///
    /// #[derive(Deserialize)]
    /// struct Info {
    ///     username: String,
    /// }
    ///
    /// /// extract path info using serde
    /// async fn index(path: types::Path<Info>, query: types::Query<HashMap<String, String>>, body: types::Json<Info>) -> String {
    ///     format!("Welcome {}!", path.username)
    /// }
    ///
    /// fn main() {
    ///     let app = App::new().service(
    ///         web::resource("/{username}/index.html") // <- define path parameters
    ///             .route(web::get().to(index))
    ///     );
    /// }
    /// ```
    pub fn to<F, T, R, U>(mut self, handler: F) -> Self
    where
        F: Factory<T, R, U>,
        T: FromRequest + 'static,
        R: Future<Output = U> + 'static,
        U: Responder + 'static,
    {
        self.service =
            Box::new(RouteNewService::new(Extract::new(Handler::new(handler))));
        self
    }
}

struct RouteNewService<T>
where
    T: ServiceFactory<Request = ServiceRequest, Error = (Error, ServiceRequest)>,
{
    service: T,
}

impl<T> RouteNewService<T>
where
    T: ServiceFactory<
        Config = (),
        Request = ServiceRequest,
        Response = ServiceResponse,
        Error = (Error, ServiceRequest),
    >,
    T::Future: 'static,
    T::Service: 'static,
    <T::Service as Service>::Future: 'static,
{
    pub fn new(service: T) -> Self {
        RouteNewService { service }
    }
}

impl<T> ServiceFactory for RouteNewService<T>
where
    T: ServiceFactory<
        Config = (),
        Request = ServiceRequest,
        Response = ServiceResponse,
        Error = (Error, ServiceRequest),
    >,
    T::Future: 'static,
    T::Service: 'static,
    <T::Service as Service>::Future: 'static,
{
    type Config = ();
    type Request = ServiceRequest;
    type Response = ServiceResponse;
    type Error = Error;
    type InitError = ();
    type Service = BoxedRouteService<ServiceRequest, Self::Response>;
    type Future = LocalBoxFuture<'static, Result<Self::Service, Self::InitError>>;

    fn new_service(&self, _: ()) -> Self::Future {
        self.service
            .new_service(())
            .map(|result| match result {
                Ok(service) => {
                    let service: BoxedRouteService<_, _> =
                        Box::new(RouteServiceWrapper { service });
                    Ok(service)
                }
                Err(_) => Err(()),
            })
            .boxed_local()
    }
}

struct RouteServiceWrapper<T: Service> {
    service: T,
}

impl<T> Service for RouteServiceWrapper<T>
where
    T::Future: 'static,
    T: Service<
        Request = ServiceRequest,
        Response = ServiceResponse,
        Error = (Error, ServiceRequest),
    >,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx).map_err(|(e, _)| e)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        // let mut fut = self.service.call(req);
        self.service
            .call(req)
            .map(|res| match res {
                Ok(res) => Ok(res),
                Err((err, req)) => Ok(req.error_response(err)),
            })
            .boxed_local()

        // match fut.poll() {
        //     Poll::Ready(Ok(res)) => Either::Left(ok(res)),
        //     Poll::Ready(Err((e, req))) => Either::Left(ok(req.error_response(e))),
        //     Poll::Pending => Either::Right(Box::new(fut.then(|res| match res {
        //         Ok(res) => Ok(res),
        //         Err((err, req)) => Ok(req.error_response(err)),
        //     }))),
        // }
    }
}
