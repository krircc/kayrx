use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::service::{IntoService, IntoServiceFactory, Service, ServiceFactory};

/// Apply tranform function to a service.
pub fn apply_fn<T, F, R, In, Out, Err, U>(service: U, f: F) -> Apply<T, F, R, In, Out, Err>
where
    T: Service<Error = Err>,
    F: FnMut(In, &mut T) -> R,
    R: Future<Output = Result<Out, Err>>,
    U: IntoService<T>,
{
    Apply::new(service.into_service(), f)
}

/// Service factory that prodices `apply_fn` service.
pub fn apply_fn_factory<T, F, R, In, Out, Err, U>(
    service: U,
    f: F,
) -> ApplyServiceFactory<T, F, R, In, Out, Err>
where
    T: ServiceFactory<Error = Err>,
    F: FnMut(In, &mut T::Service) -> R + Clone,
    R: Future<Output = Result<Out, Err>>,
    U: IntoServiceFactory<T>,
{
    ApplyServiceFactory::new(service.into_factory(), f)
}

/// `Apply` service combinator
pub struct Apply<T, F, R, In, Out, Err>
where
    T: Service<Error = Err>,
{
    service: T,
    f: F,
    r: PhantomData<(In, Out, R)>,
}

impl<T, F, R, In, Out, Err> Apply<T, F, R, In, Out, Err>
where
    T: Service<Error = Err>,
    F: FnMut(In, &mut T) -> R,
    R: Future<Output = Result<Out, Err>>,
{
    /// Create new `Apply` combinator
    fn new(service: T, f: F) -> Self {
        Self {
            service,
            f,
            r: PhantomData,
        }
    }
}

impl<T, F, R, In, Out, Err> Clone for Apply<T, F, R, In, Out, Err>
where
    T: Service<Error = Err> + Clone,
    F: FnMut(In, &mut T) -> R + Clone,
    R: Future<Output = Result<Out, Err>>,
{
    fn clone(&self) -> Self {
        Apply {
            service: self.service.clone(),
            f: self.f.clone(),
            r: PhantomData,
        }
    }
}

impl<T, F, R, In, Out, Err> Service for Apply<T, F, R, In, Out, Err>
where
    T: Service<Error = Err>,
    F: FnMut(In, &mut T) -> R,
    R: Future<Output = Result<Out, Err>>,
{
    type Request = In;
    type Response = Out;
    type Error = Err;
    type Future = R;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(futures_util::ready!(self.service.poll_ready(cx)))
    }

    fn call(&mut self, req: In) -> Self::Future {
        (self.f)(req, &mut self.service)
    }
}

/// `apply()` service factory
pub struct ApplyServiceFactory<T, F, R, In, Out, Err>
where
    T: ServiceFactory<Error = Err>,
    F: FnMut(In, &mut T::Service) -> R + Clone,
    R: Future<Output = Result<Out, Err>>,
{
    service: T,
    f: F,
    r: PhantomData<(R, In, Out)>,
}

impl<T, F, R, In, Out, Err> ApplyServiceFactory<T, F, R, In, Out, Err>
where
    T: ServiceFactory<Error = Err>,
    F: FnMut(In, &mut T::Service) -> R + Clone,
    R: Future<Output = Result<Out, Err>>,
{
    /// Create new `ApplyNewService` new service instance
    fn new(service: T, f: F) -> Self {
        Self {
            f,
            service,
            r: PhantomData,
        }
    }
}

impl<T, F, R, In, Out, Err> Clone for ApplyServiceFactory<T, F, R, In, Out, Err>
where
    T: ServiceFactory<Error = Err> + Clone,
    F: FnMut(In, &mut T::Service) -> R + Clone,
    R: Future<Output = Result<Out, Err>>,
{
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            f: self.f.clone(),
            r: PhantomData,
        }
    }
}

impl<T, F, R, In, Out, Err> ServiceFactory for ApplyServiceFactory<T, F, R, In, Out, Err>
where
    T: ServiceFactory<Error = Err>,
    F: FnMut(In, &mut T::Service) -> R + Clone,
    R: Future<Output = Result<Out, Err>>,
{
    type Request = In;
    type Response = Out;
    type Error = Err;

    type Config = T::Config;
    type Service = Apply<T::Service, F, R, In, Out, Err>;
    type InitError = T::InitError;
    type Future = ApplyServiceFactoryResponse<T, F, R, In, Out, Err>;

    fn new_service(&self, cfg: T::Config) -> Self::Future {
        ApplyServiceFactoryResponse::new(self.service.new_service(cfg), self.f.clone())
    }
}

#[pin_project::pin_project]
pub struct ApplyServiceFactoryResponse<T, F, R, In, Out, Err>
where
    T: ServiceFactory<Error = Err>,
    F: FnMut(In, &mut T::Service) -> R,
    R: Future<Output = Result<Out, Err>>,
{
    #[pin]
    fut: T::Future,
    f: Option<F>,
    r: PhantomData<(In, Out)>,
}

impl<T, F, R, In, Out, Err> ApplyServiceFactoryResponse<T, F, R, In, Out, Err>
where
    T: ServiceFactory<Error = Err>,
    F: FnMut(In, &mut T::Service) -> R,
    R: Future<Output = Result<Out, Err>>,
{
    fn new(fut: T::Future, f: F) -> Self {
        Self {
            f: Some(f),
            fut,
            r: PhantomData,
        }
    }
}

impl<T, F, R, In, Out, Err> Future for ApplyServiceFactoryResponse<T, F, R, In, Out, Err>
where
    T: ServiceFactory<Error = Err>,
    F: FnMut(In, &mut T::Service) -> R,
    R: Future<Output = Result<Out, Err>>,
{
    type Output = Result<Apply<T::Service, F, R, In, Out, Err>, T::InitError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if let Poll::Ready(svc) = this.fut.poll(cx)? {
            Poll::Ready(Ok(Apply::new(svc, this.f.take().unwrap())))
        } else {
            Poll::Pending
        }
    }
}
