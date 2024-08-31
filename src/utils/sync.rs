use std::{
    future::Future,
    pin::{pin, Pin},
    sync::{Mutex, OnceLock},
    task::{ready, Context, Poll, Waker},
};

use pin_project::pin_project;

use super::Once;

#[derive(Debug, Default)]
struct InitializationQueue {
    initializing: bool,
    wakers: Vec<Waker>,
}

/// Partial alternative implementation of [`tokio::sync::OnceCell`].
#[derive(Debug)]
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

impl<T> Default for AsyncOnceLock<T> {
    fn default() -> Self {
        Self {
            inner: OnceLock::new(),
            init_queue: Mutex::new(InitializationQueue::default()),
        }
    }
}

#[derive(Debug)]
pub struct WaitForInit {
    queue_pos: usize,
}

#[derive(Debug)]
enum TryInitFutureState<'a, T> {
    Creating,
    Initialized(&'a T),
    Initializing,
    Waiting(WaitForInit),
}

/// Internal implementing [`Future`] for [`AsyncOnceLock::get_or_try_init`].
#[derive(Debug)]
#[pin_project]
pub struct TryInitFuture<'a, T, E, Fut: Future<Output = Result<T, E>>> {
    once_lock: &'a AsyncOnceLock<T>,
    #[pin]
    fut: Fut,
    state: TryInitFutureState<'a, T>,
}

impl<'a, T, E, Fut: Future<Output = Result<T, E>>> TryInitFuture<'a, T, E, Fut> {
    pub fn new(once_lock: &'a AsyncOnceLock<T>, fut: Fut) -> Self
    where
        Fut: Future<Output = Result<T, E>>,
    {
        Self {
            once_lock,
            fut,
            state: TryInitFutureState::Creating,
        }
    }

    fn create(once_lock: &'a AsyncOnceLock<T>, cx: &mut Context<'_>) -> TryInitFutureState<'a, T> {
        if let Some(val) = once_lock.get() {
            TryInitFutureState::Initialized(val)
        } else {
            let mut init_queue = once_lock.init_queue.lock().unwrap();

            if init_queue.initializing {
                // Register a waker at back of queue
                let queue_pos = init_queue.wakers.len();
                init_queue.wakers.push(cx.waker().clone());

                TryInitFutureState::Waiting(WaitForInit { queue_pos })
            } else {
                // Check that there wasn't an initialization between previous
                // once_lock check and init_queue mutex claim.
                if let Some(val) = once_lock.get() {
                    TryInitFutureState::Initialized(val)
                } else {
                    // Claim initialization
                    init_queue.initializing = true;
                    TryInitFutureState::Initializing
                }
            }
        }
    }
}

impl<'a, T, E, Fut> Future for TryInitFuture<'a, T, E, Fut>
where
    T: 'a,
    Fut: Future<Output = Result<T, E>>,
{
    type Output = Result<&'a T, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().project();
        match this.state {
            // Run future initialization and immediately poll the next state
            TryInitFutureState::Creating => {
                *this.state = Self::create(this.once_lock, cx);

                if !matches!(this.state, TryInitFutureState::Waiting(_)) {
                    drop(this);
                    self.poll(cx)
                } else {
                    Poll::Pending
                }
            }

            // Lock into always returning the result state
            TryInitFutureState::Initialized(val) => Poll::Ready(Ok(val)),

            // Only one thread polls the initializing function
            TryInitFutureState::Initializing => {
                let res = ready!(this.fut.poll(cx));
                let mut init_queue = this.once_lock.init_queue.lock().unwrap();

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
                        *this.state =
                            TryInitFutureState::Initialized(this.once_lock.get_or_init(|| val));
                        self.poll(cx)
                    }
                    // This will cause spurious wakes if re-polled after Ready(Err).
                    // That's a bad usage pattern though, so the performance loss
                    // is a nonconcern.
                    Err(e) => Poll::Ready(Err(e)),
                }
            }

            TryInitFutureState::Waiting(WaitForInit { queue_pos }) => {
                let mut init_queue = this.once_lock.init_queue.lock().unwrap();

                if init_queue.initializing {
                    // The initializing thread isn't finished,
                    // so just update the callback
                    init_queue.wakers[*queue_pos].clone_from(cx.waker());
                    Poll::Pending
                } else {
                    // Runs again if initialization failed
                    *this.state = if let Some(val) = this.once_lock.get() {
                        TryInitFutureState::Initialized(val)
                    } else {
                        // Claim initialization
                        init_queue.initializing = true;
                        TryInitFutureState::Initializing
                    };

                    // The mutex needs to be released for the next poll
                    drop(init_queue);

                    self.poll(cx)
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

    pub fn get_or_try_init<E, Fut>(&self, f: Fut) -> TryInitFuture<'_, T, E, Fut>
    where
        Fut: Future<Output = Result<T, E>>,
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
