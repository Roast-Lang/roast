//! Thread management.
//!
//! Threading utilities and thread pool.

use std::thread::{self, JoinHandle as StdJoinHandle, ThreadId};
use std::time::Duration;
use std::sync::{Arc, Mutex, mpsc};

/// Thread handle.
pub struct JoinHandle<T> {
    inner: StdJoinHandle<T>,
}

impl<T> JoinHandle<T> {
    /// Wait for the thread to finish.
    pub fn join(self) -> Result<T, Box<dyn std::any::Any + Send>> {
        self.inner.join()
    }
    
    /// Check if the thread has finished.
    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }
    
    /// Get the thread's ID.
    pub fn thread_id(&self) -> ThreadId {
        self.inner.thread().id()
    }
}

/// Spawn a new thread.
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    JoinHandle { inner: thread::spawn(f) }
}

/// Spawn a named thread.
pub fn spawn_named<F, T>(name: &str, f: F) -> std::io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let handle = thread::Builder::new()
        .name(name.to_string())
        .spawn(f)?;
    Ok(JoinHandle { inner: handle })
}

/// Sleep for a duration.
pub fn sleep(duration_ms: u64) {
    thread::sleep(Duration::from_millis(duration_ms));
}

/// Sleep for seconds.
pub fn sleep_secs(secs: u64) {
    thread::sleep(Duration::from_secs(secs));
}

/// Yield the current thread.
pub fn yield_now() {
    thread::yield_now();
}

/// Get the current thread's ID.
pub fn current_id() -> ThreadId {
    thread::current().id()
}

/// Get the current thread's name.
pub fn current_name() -> Option<String> {
    thread::current().name().map(String::from)
}

/// Get the number of available CPU cores.
pub fn available_parallelism() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Thread builder for configuring threads.
pub struct Builder {
    name: Option<String>,
    stack_size: Option<usize>,
}

impl Builder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            name: None,
            stack_size: None,
        }
    }
    
    /// Set thread name.
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }
    
    /// Set stack size.
    pub fn stack_size(mut self, size: usize) -> Self {
        self.stack_size = Some(size);
        self
    }
    
    /// Spawn the thread.
    pub fn spawn<F, T>(self, f: F) -> std::io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let mut builder = thread::Builder::new();
        
        if let Some(name) = self.name {
            builder = builder.name(name);
        }
        if let Some(size) = self.stack_size {
            builder = builder.stack_size(size);
        }
        
        builder.spawn(f).map(|h| JoinHandle { inner: h })
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread pool for executing tasks.
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    /// Create a new thread pool with n workers.
    pub fn new(size: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        
        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }
        
        Self {
            workers,
            sender: Some(sender),
        }
    }
    
    /// Execute a task on the pool.
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        if let Some(sender) = &self.sender {
            sender.send(job).ok();
        }
    }
    
    /// Get the number of workers.
    pub fn size(&self) -> usize {
        self.workers.len()
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        // Drop sender to signal workers to stop
        drop(self.sender.take());
        
        // Join all workers
        for worker in &mut self.workers {
            if let Some(handle) = worker.handle.take() {
                handle.join().ok();
            }
        }
    }
}

struct Worker {
    id: usize,
    handle: Option<StdJoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Self {
        let handle = thread::spawn(move || loop {
            let job = {
                let receiver = receiver.lock().unwrap();
                receiver.recv()
            };
            
            match job {
                Ok(job) => job(),
                Err(_) => break, // Channel closed
            }
        });
        
        Self {
            id,
            handle: Some(handle),
        }
    }
}

/// Scoped threads - threads that can borrow from the stack.
pub fn scope<'env, F, T>(f: F) -> T
where
    F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
{
    thread::scope(|s| f(unsafe { std::mem::transmute(&Scope { inner: s }) }))
}

/// Scoped thread scope.
pub struct Scope<'scope, 'env: 'scope> {
    inner: &'scope thread::Scope<'scope, 'env>,
}

impl<'scope, 'env> Scope<'scope, 'env> {
    /// Spawn a scoped thread.
    pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        ScopedJoinHandle {
            inner: self.inner.spawn(f),
        }
    }
}

/// Scoped thread handle.
pub struct ScopedJoinHandle<'scope, T> {
    inner: thread::ScopedJoinHandle<'scope, T>,
}

impl<'scope, T> ScopedJoinHandle<'scope, T> {
    /// Wait for the thread to finish.
    pub fn join(self) -> Result<T, Box<dyn std::any::Any + Send + 'static>> {
        self.inner.join()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_spawn() {
        let handle = spawn(|| 42);
        assert_eq!(handle.join().unwrap(), 42);
    }
    
    #[test]
    fn test_thread_pool() {
        let pool = ThreadPool::new(4);
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        
        for _ in 0..100 {
            let counter = Arc::clone(&counter);
            pool.execute(move || {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
        }
        
        // Give time for tasks to complete
        thread::sleep(Duration::from_millis(100));
        
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 100);
    }
}

