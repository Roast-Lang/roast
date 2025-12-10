//! Async utilities.
//!
//! Helpers for async/await programming patterns.
//! Note: This provides abstractions; actual async runtime would be separate.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A simple future that completes immediately with a value.
pub struct Ready<T> {
    value: Option<T>,
}

impl<T> Ready<T> {
    pub fn new(value: T) -> Self {
        Self { value: Some(value) }
    }
}

impl<T> Future for Ready<T> {
    type Output = T;
    
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        Poll::Ready(this.value.take().expect("Ready polled after completion"))
    }
}

/// Create a future that is immediately ready.
pub fn ready<T>(value: T) -> Ready<T> {
    Ready::new(value)
}

/// A pending future that never completes.
pub struct Pending<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> Pending<T> {
    pub fn new() -> Self {
        Self { _marker: std::marker::PhantomData }
    }
}

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Future for Pending<T> {
    type Output = T;
    
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

/// Create a future that never completes.
pub fn pending<T>() -> Pending<T> {
    Pending::new()
}

/// A lazy future that computes its value on first poll.
pub struct Lazy<F, T> {
    func: Option<F>,
    _marker: std::marker::PhantomData<T>,
}

impl<F: FnOnce() -> T, T> Lazy<F, T> {
    pub fn new(func: F) -> Self {
        Self {
            func: Some(func),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<F: FnOnce() -> T, T> Future for Lazy<F, T> {
    type Output = T;
    
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let func = this.func.take().expect("Lazy polled after completion");
        Poll::Ready(func())
    }
}

/// Create a lazy future.
pub fn lazy<F: FnOnce() -> T, T>(func: F) -> Lazy<F, T> {
    Lazy::new(func)
}

/// Shared state for oneshot channel.
struct OneshotState<T> {
    value: Option<T>,
    waker: Option<Waker>,
    closed: bool,
}

/// Sender for oneshot channel.
pub struct OneshotSender<T> {
    state: Arc<Mutex<OneshotState<T>>>,
}

impl<T> OneshotSender<T> {
    /// Send a value.
    pub fn send(self, value: T) -> Result<(), T> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(value);
        }
        state.value = Some(value);
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
        Ok(())
    }
}

/// Receiver for oneshot channel.
pub struct OneshotReceiver<T> {
    state: Arc<Mutex<OneshotState<T>>>,
}

impl<T> Future for OneshotReceiver<T> {
    type Output = Option<T>;
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();
        
        if let Some(value) = state.value.take() {
            return Poll::Ready(Some(value));
        }
        
        if state.closed {
            return Poll::Ready(None);
        }
        
        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl<T> Drop for OneshotReceiver<T> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
    }
}

/// Create a oneshot channel.
pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    let state = Arc::new(Mutex::new(OneshotState {
        value: None,
        waker: None,
        closed: false,
    }));
    
    (
        OneshotSender { state: Arc::clone(&state) },
        OneshotReceiver { state },
    )
}

/// Join multiple futures, waiting for all to complete.
pub async fn join_all<T, I>(futures: I) -> Vec<T>
where
    I: IntoIterator,
    I::Item: Future<Output = T>,
{
    // Note: This is a simplified implementation
    // Real implementation would need proper polling
    let mut results = Vec::new();
    for fut in futures {
        results.push(fut.await);
    }
    results
}

/// Select the first future to complete.
/// Note: This is a simplified version that just awaits the first one.
pub async fn select_first<T, F1, F2>(fut1: F1, fut2: F2) -> T
where
    F1: Future<Output = T>,
    F2: Future<Output = T>,
{
    // Simplified - just await the first
    fut1.await
}

/// Retry a future with exponential backoff.
pub async fn retry<T, E, F, Fut>(
    mut f: F,
    max_retries: usize,
    initial_delay_ms: u64,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut delay = initial_delay_ms;
    
    for attempt in 0..max_retries {
        match f().await {
            Ok(value) => return Ok(value),
            Err(e) if attempt == max_retries - 1 => return Err(e),
            Err(_) => {
                std::thread::sleep(Duration::from_millis(delay));
                delay *= 2;
            }
        }
    }
    
    // This should never be reached due to the loop logic
    f().await
}

/// Timeout a future.
pub struct Timeout<F> {
    future: F,
    deadline: std::time::Instant,
}

impl<F: Future> Timeout<F> {
    pub fn new(future: F, duration: Duration) -> Self {
        Self {
            future,
            deadline: std::time::Instant::now() + duration,
        }
    }
}

/// Timeout error.
#[derive(Debug, Clone, Copy)]
pub struct TimeoutError;

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

impl std::error::Error for TimeoutError {}

