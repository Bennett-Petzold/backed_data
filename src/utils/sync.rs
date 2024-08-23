use std::{
    future::Future,
    pin::{pin, Pin},
    sync::{Mutex, OnceLock},
    task::{Context, Poll, Waker},
};

use super::Once;

#[derive(Debug, Default)]
struct InitializationQueue {
    initializing: bool,
    wakers: Vec<Waker>,
}

/// Partial alternative implementation of [`tokio::sync::OnceCell`].
#[derive(Debug, Default)]
pub struct AsyncOnceLock<T> {
    inner: OnceLock<T>,
    init_queue: Mutex<InitializationQueue>,
}

impl<T: Clone> Clone for AsyncOnceLock<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            init_queue: Mutex::new(InitializationQueue::default()),
        }
    }
}

#[derive(Debug)]
pub struct WaitForInit<'a, T, F> {
    queue_pos: usize,
    f: Option<F>,
    once_lock: &'a AsyncOnceLock<T>,
}

#[derive(Debug)]
pub struct InitializingFn<'a, T, E, Fut>
where
    Fut: Future<Output = Result<T, E>> + Unpin,
{
    fut: Fut,
    once_lock: &'a AsyncOnceLock<T>,
}

#[derive(Debug)]
pub struct CreatingFn<'a, T, F: FnOnce() -> Fut, Fut> {
    f: Option<F>,
    once_lock: &'a AsyncOnceLock<T>,
}

/// Internal implementing [`Future`] for [`AsyncOnceLock::get_or_try_init`].
#[derive(Debug)]
pub enum TryInitFuture<'a, T, E, F: FnOnce() -> Fut, Fut: Future<Output = Result<T, E>> + Unpin> {
    Creating(CreatingFn<'a, T, F, Fut>),
    Initialized(&'a T),
    Initializing(InitializingFn<'a, T, E, Fut>),
    Waiting(WaitForInit<'a, T, F>),
}

impl<'a, T, E, F: FnOnce() -> Fut, Fut: Future<Output = Result<T, E>> + Unpin>
    TryInitFuture<'a, T, E, F, Fut>
{
    pub fn new(once_lock: &'a AsyncOnceLock<T>, f: F) -> Self
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        Self::Creating(CreatingFn {
            f: Some(f),
            once_lock,
        })
    }

    fn create(once_lock: &'a AsyncOnceLock<T>, f: F, cx: &mut Context<'_>) -> Self {
        if let Some(val) = once_lock.get() {
            Self::Initialized(val)
        } else {
            let mut init_queue = once_lock.init_queue.lock().unwrap();

            if init_queue.initializing {
                // Register a waker at back of queue
                let queue_pos = init_queue.wakers.len();
                init_queue.wakers.push(cx.waker().clone());

                Self::Waiting(WaitForInit {
                    queue_pos,
                    f: Some(f),
                    once_lock,
                })
            } else {
                // Check that there wasn't an initialization between previous
                // once_lock check and init_queue mutex claim.
                if let Some(val) = once_lock.get() {
                    Self::Initialized(val)
                } else {
                    // Claim initialization
                    init_queue.initializing = true;
                    Self::Initializing(InitializingFn {
                        fut: (f)(),
                        once_lock,
                    })
                }
            }
        }
    }
}

impl<T, E, F: FnOnce() -> Fut, Fut: Future<Output = Result<T, E>> + Unpin> Unpin
    for TryInitFuture<'_, T, E, F, Fut>
{
}

impl<'a, T, E, F, Fut> Future for TryInitFuture<'a, T, E, F, Fut>
where
    T: 'a,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>> + Unpin,
{
    type Output = Result<&'a T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this {
            // Run future initialization and immediately poll the next state
            Self::Creating(CreatingFn {
                ref mut f,
                once_lock,
            }) => {
                let f = f
                    .take()
                    .unwrap_or_else(|| panic!("This is always Some, as it executes once."));
                *this = Self::create(once_lock, f, cx);

                if !matches!(this, Self::Waiting(_)) {
                    Pin::new(this).poll(cx)
                } else {
                    Poll::Pending
                }
            }

            // Lock into always returning the result state
            Self::Initialized(val) => Poll::Ready(Ok(val)),

            // Only one thread polls the initializing function
            Self::Initializing(InitializingFn { fut, once_lock }) => match pin!(fut).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(res) => {
                    let mut init_queue = once_lock.init_queue.lock().unwrap();

                    // Enable all other futures pending on initialization
                    {
                        // Release hold regardless of failure or success
                        init_queue.initializing = false;

                        // Empty out the waker queue and ping each one
                        let wakers = std::mem::take(&mut init_queue.wakers);
                        wakers.into_iter().for_each(|waker| waker.wake());
                    }

                    match res {
                        Ok(val) => {
                            *this = Self::Initialized(once_lock.get_or_init(|| val));
                            Pin::new(this).poll(cx)
                        }
                        // This will cause spurious wakes if re-polled after Ready(Err).
                        // That's a bad usage pattern though, so the performance loss
                        // is a nonconcern.
                        Err(e) => Poll::Ready(Err(e)),
                    }
                }
            },

            Self::Waiting(WaitForInit {
                queue_pos,
                f,
                once_lock,
            }) => {
                let mut init_queue = once_lock.init_queue.lock().unwrap();

                if init_queue.initializing {
                    // The initializing thread isn't finished,
                    // so just update the callback
                    init_queue.wakers[*queue_pos].clone_from(cx.waker());
                    Poll::Pending
                } else {
                    // Runs again if initialization failed
                    *this = if let Some(val) = once_lock.get() {
                        Self::Initialized(val)
                    } else {
                        let f = f
                            .take()
                            .unwrap_or_else(|| panic!("This is always Some, as it executes once."));

                        // Claim initialization
                        init_queue.initializing = true;
                        Self::Initializing(InitializingFn {
                            fut: (f)(),
                            once_lock,
                        })
                    };

                    // The mutex needs to be released for the next poll
                    drop(init_queue);

                    Pin::new(this).poll(cx)
                }
            }
        }
    }
}

impl<T> AsyncOnceLock<T> {
    pub fn new_with(value: Option<T>) -> Self {
        let this = Self::new();

        if let Some(value) = value {
            this.inner.set(value).unwrap_or_else(|_| {
                panic!("Entirely internal object, should be impossible to share")
            });
        }

        this
    }

    pub fn get_or_try_init<E, F, Fut>(&self, f: F) -> TryInitFuture<T, E, F, Fut>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>> + Unpin,
    {
        TryInitFuture::new(self, f)
    }
}

impl<T> Once for AsyncOnceLock<T> {
    type Inner = T;

    fn new() -> Self {
        Self {
            inner: OnceLock::new(),
            init_queue: Mutex::new(InitializationQueue::default()),
        }
    }

    fn get(&self) -> Option<&Self::Inner> {
        self.inner.get()
    }

    fn set(&self, value: Self::Inner) -> Result<(), Self::Inner> {
        self.inner.set(value)
    }

    fn take(&mut self) -> Option<Self::Inner> {
        self.inner.take()
    }

    fn get_mut(&mut self) -> Option<&mut Self::Inner> {
        self.inner.get_mut()
    }

    fn into_inner(self) -> Option<Self::Inner> {
        self.inner.into_inner()
    }

    fn get_or_init<F>(&self, f: F) -> &Self::Inner
    where
        F: FnOnce() -> Self::Inner,
    {
        let _ = self.set((f)());
        self.get().unwrap()
    }
}
