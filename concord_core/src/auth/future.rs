use std::future::Future;
use std::pin::Pin;

pub type AuthFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
