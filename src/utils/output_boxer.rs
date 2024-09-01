use std::{
    future::Future,
    pin::Pin,
    task::{ready, Poll},
};

use pin_project::pin_project;

#[derive(Debug, Clone)]
#[pin_project]
pub struct OutputBoxer<T>(#[pin] pub T);

impl<T: Future<Output = Result<U, E>>, U, E> Future for OutputBoxer<T> {
    type Output = Result<Pin<Box<U>>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        match ready!(self.project().0.poll(cx)) {
            Ok(output) => Poll::Ready(Ok(Box::pin(output))),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}
