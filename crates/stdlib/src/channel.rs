//! Channel communication between threads.
//!
//! Multi-producer, single-consumer and multi-producer, multi-consumer channels.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, Condvar};
use std::time::Duration;
use std::collections::VecDeque;

/// Sender half of a channel.
pub struct Sender<T> {
    inner: mpsc::Sender<T>,
}

impl<T> Sender<T> {
    /// Send a value.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.send(value).map_err(|e| SendError(e.0))
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

/// Receiver half of a channel.
pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
}

impl<T> Receiver<T> {
    /// Receive a value, blocking until one is available.
    pub fn recv(&self) -> Result<T, RecvError> {
        self.inner.recv().map_err(|_| RecvError)
    }
    
    /// Try to receive without blocking.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.inner.try_recv().map_err(|e| match e {
            mpsc::TryRecvError::Empty => TryRecvError::Empty,
            mpsc::TryRecvError::Disconnected => TryRecvError::Disconnected,
        })
    }
    
    /// Receive with timeout.
    pub fn recv_timeout(&self, timeout_ms: u64) -> Result<T, RecvTimeoutError> {
        self.inner.recv_timeout(Duration::from_millis(timeout_ms))
            .map_err(|e| match e {
                mpsc::RecvTimeoutError::Timeout => RecvTimeoutError::Timeout,
                mpsc::RecvTimeoutError::Disconnected => RecvTimeoutError::Disconnected,
            })
    }
    
    /// Iterate over received values.
    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        self.inner.iter()
    }
}

/// Send error.
pub struct SendError<T>(pub T);

impl<T> std::fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendError").finish()
    }
}

/// Receive error.
#[derive(Debug)]
pub struct RecvError;

/// Try receive error.
#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

/// Receive timeout error.
#[derive(Debug)]
pub enum RecvTimeoutError {
    Timeout,
    Disconnected,
}

/// Create a new unbounded channel.
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel();
    (Sender { inner: tx }, Receiver { inner: rx })
}

/// Create a new synchronous (bounded) channel.
pub fn bounded<T>(capacity: usize) -> (SyncSender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::sync_channel(capacity);
    (SyncSender { inner: tx }, Receiver { inner: rx })
}

/// Synchronous sender for bounded channels.
pub struct SyncSender<T> {
    inner: mpsc::SyncSender<T>,
}

impl<T> SyncSender<T> {
    /// Send a value, blocking if the channel is full.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.send(value).map_err(|e| SendError(e.0))
    }
    
    /// Try to send without blocking.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.inner.try_send(value).map_err(|e| match e {
            mpsc::TrySendError::Full(v) => TrySendError::Full(v),
            mpsc::TrySendError::Disconnected(v) => TrySendError::Disconnected(v),
        })
    }
}

impl<T> Clone for SyncSender<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

/// Try send error.
pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}

impl<T> std::fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrySendError::Full(_) => write!(f, "Full"),
            TrySendError::Disconnected(_) => write!(f, "Disconnected"),
        }
    }
}

/// Multi-producer, multi-consumer channel.
pub struct MpmcChannel<T> {
    queue: Mutex<VecDeque<T>>,
    not_empty: Condvar,
    not_full: Condvar,
    capacity: Option<usize>,
}

impl<T> MpmcChannel<T> {
    /// Create an unbounded MPMC channel.
    pub fn unbounded() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::new()),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            capacity: None,
        })
    }
    
    /// Create a bounded MPMC channel.
    pub fn bounded(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::new()),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            capacity: Some(capacity),
        })
    }
    
    /// Send a value.
    pub fn send(&self, value: T) {
        let mut queue = self.queue.lock().unwrap();
        
        // Wait if bounded and full
        if let Some(cap) = self.capacity {
            while queue.len() >= cap {
                queue = self.not_full.wait(queue).unwrap();
            }
        }
        
        queue.push_back(value);
        self.not_empty.notify_one();
    }
    
    /// Receive a value.
    pub fn recv(&self) -> T {
        let mut queue = self.queue.lock().unwrap();
        
        while queue.is_empty() {
            queue = self.not_empty.wait(queue).unwrap();
        }
        
        let value = queue.pop_front().unwrap();
        self.not_full.notify_one();
        value
    }
    
    /// Try to receive without blocking.
    pub fn try_recv(&self) -> Option<T> {
        let mut queue = self.queue.lock().unwrap();
        let value = queue.pop_front();
        if value.is_some() {
            self.not_full.notify_one();
        }
        value
    }
    
    /// Get queue length.
    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.queue.lock().unwrap().is_empty()
    }
}

/// Oneshot channel - send exactly one value.
pub struct Oneshot<T> {
    value: Mutex<Option<T>>,
    condvar: Condvar,
    sent: std::sync::atomic::AtomicBool,
}

impl<T> Oneshot<T> {
    /// Create a new oneshot channel.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            value: Mutex::new(None),
            condvar: Condvar::new(),
            sent: std::sync::atomic::AtomicBool::new(false),
        })
    }
    
    /// Send a value (can only be called once).
    pub fn send(&self, value: T) -> Result<(), T> {
        use std::sync::atomic::Ordering;
        
        if self.sent.swap(true, Ordering::SeqCst) {
            return Err(value);
        }
        
        *self.value.lock().unwrap() = Some(value);
        self.condvar.notify_all();
        Ok(())
    }
    
    /// Receive the value.
    pub fn recv(&self) -> T {
        let mut guard = self.value.lock().unwrap();
        
        while guard.is_none() {
            guard = self.condvar.wait(guard).unwrap();
        }
        
        guard.take().unwrap()
    }
    
    /// Try to receive.
    pub fn try_recv(&self) -> Option<T> {
        self.value.lock().unwrap().take()
    }
    
    /// Check if value has been sent.
    pub fn is_sent(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.sent.load(Ordering::SeqCst)
    }
}

impl<T> Default for Oneshot<T> {
    fn default() -> Self {
        Self {
            value: Mutex::new(None),
            condvar: Condvar::new(),
            sent: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    
    #[test]
    fn test_unbounded_channel() {
        let (tx, rx) = unbounded();
        
        tx.send(42).unwrap();
        tx.send(43).unwrap();
        
        assert_eq!(rx.recv().unwrap(), 42);
        assert_eq!(rx.recv().unwrap(), 43);
    }
    
    #[test]
    fn test_bounded_channel() {
        let (tx, rx) = bounded(2);
        
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        
        assert!(tx.try_send(3).is_err()); // Full
        
        assert_eq!(rx.recv().unwrap(), 1);
        tx.send(3).unwrap(); // Now has space
    }
    
    #[test]
    fn test_mpmc() {
        let ch = MpmcChannel::unbounded();
        let ch2 = Arc::clone(&ch);
        
        thread::spawn(move || {
            ch2.send(42);
        });
        
        assert_eq!(ch.recv(), 42);
    }
}

