use futures::StreamExt;

/// Workaround for lack of stable [`FnOnce`] implementation.
///
/// Produces a value once on `call`, consuming self.
pub trait BlockingFn {
    type Output;
    fn call(self) -> Self::Output;
}

use std::{
    future::{ready, Ready},
    mem::MaybeUninit,
};

/// Executes in the current thread.
pub fn cur_thread<T, F>(arg: F) -> Ready<T>
where
    F: BlockingFn<Output = T>,
{
    ready(arg.call())
}

/// Executes on a spawned tokio thread.
///
/// If the safety rules will be met, this can be converted to a safe future
/// via a closure (`unsafe {|x| tokio_blocking(x)}`).
///
/// # Safety
/// This uses a non-blocking [`async_scoped`] spawn. If arg has a
/// non-static lifetime, this future CANNOT be forgotten
/// ([`std::mem::forget`]) mid-execution, as that will invalidate `arg`
/// for the spawned thread and possibly cause a panic.
pub async unsafe fn tokio_blocking<'a, T, F>(arg: F) -> T
where
    T: Send + Sync,
    F: BlockingFn<Output = T> + Send,
{
    // Val is held here uninitialized, and written into by the thread.
    let mut val = MaybeUninit::uninit();
    let arg = || {
        val = MaybeUninit::new(arg.call());
    };

    // Spawn the thread and wait for completion.
    let (mut scope, _) = unsafe {
        async_scoped::TokioScope::scope(|s| {
            s.spawn_blocking(arg);
        })
    };
    // Only need to pull () result from one blocking future.
    scope.next().await.unwrap().unwrap();

    drop(scope);

    // Val was initialized by the thread, or the program panicked.
    unsafe { val.assume_init() }
}
