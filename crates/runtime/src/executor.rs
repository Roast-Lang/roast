//! Async executor for Roast.
//!
//! Provides a work-stealing async runtime for Roast's async/await.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Condvar};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Task ID type.
pub type TaskId = u64;

/// A spawned task.
pub struct Task {
    /// Unique task ID.
    id: TaskId,
    /// The future to execute.
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
    /// Whether the task is ready to run.
    ready: AtomicBool,
    /// Task state.
    state: AtomicUsize,
}

const STATE_IDLE: usize = 0;
const STATE_SCHEDULED: usize = 1;
const STATE_RUNNING: usize = 2;
const STATE_COMPLETED: usize = 3;
const STATE_CANCELLED: usize = 4;

impl Task {
    fn new<F>(id: TaskId, future: F) -> Arc<Self>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Arc::new(Self {
            id,
            future: Mutex::new(Box::pin(future)),
            ready: AtomicBool::new(true),
            state: AtomicUsize::new(STATE_SCHEDULED),
        })
    }
    
    fn poll(&self, waker: &Waker) -> Poll<()> {
        self.state.store(STATE_RUNNING, Ordering::SeqCst);
        
        let mut future = self.future.lock().unwrap();
        let mut cx = Context::from_waker(waker);
        
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(()) => {
                self.state.store(STATE_COMPLETED, Ordering::SeqCst);
                Poll::Ready(())
            }
            Poll::Pending => {
                self.state.store(STATE_IDLE, Ordering::SeqCst);
                self.ready.store(false, Ordering::SeqCst);
                Poll::Pending
            }
        }
    }
    
    fn wake(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }
    
    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
    
    fn is_completed(&self) -> bool {
        self.state.load(Ordering::SeqCst) == STATE_COMPLETED
    }
    
    fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::SeqCst) == STATE_CANCELLED
    }
    
    fn cancel(&self) {
        self.state.store(STATE_CANCELLED, Ordering::SeqCst);
    }
}

/// Task handle for cancellation and joining.
#[derive(Clone)]
pub struct TaskHandle {
    task: Arc<Task>,
}

impl TaskHandle {
    /// Get the task ID.
    pub fn id(&self) -> TaskId {
        self.task.id
    }
    
    /// Cancel the task.
    pub fn cancel(&self) {
        self.task.cancel();
    }
    
    /// Check if task is completed.
    pub fn is_completed(&self) -> bool {
        self.task.is_completed()
    }
    
    /// Check if task is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.task.is_cancelled()
    }
}

/// Executor configuration.
#[derive(Clone, Debug)]
pub struct ExecutorConfig {
    /// Number of worker threads.
    pub num_workers: usize,
    /// Worker thread stack size.
    pub stack_size: usize,
    /// Enable work stealing.
    pub work_stealing: bool,
    /// Maximum tasks per batch.
    pub batch_size: usize,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            num_workers: num_cpus(),
            stack_size: 2 * 1024 * 1024, // 2MB
            work_stealing: true,
            batch_size: 32,
        }
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}

/// Work-stealing async executor.
pub struct Executor {
    /// Configuration.
    config: ExecutorConfig,
    /// Task ID counter.
    next_task_id: AtomicU64,
    /// Global task queue.
    global_queue: Mutex<VecDeque<Arc<Task>>>,
    /// Shutdown signal.
    shutdown: AtomicBool,
    /// Condition variable for workers.
    condvar: Condvar,
    /// Worker threads.
    workers: Mutex<Vec<JoinHandle<()>>>,
    /// Worker local queues (for work stealing).
    worker_queues: Vec<Mutex<VecDeque<Arc<Task>>>>,
}

impl Executor {
    /// Create a new executor.
    pub fn new(config: ExecutorConfig) -> Arc<Self> {
        let num_workers = config.num_workers;
        let worker_queues = (0..num_workers)
            .map(|_| Mutex::new(VecDeque::new()))
            .collect();
        
        let executor = Arc::new(Self {
            config: config.clone(),
            next_task_id: AtomicU64::new(1),
            global_queue: Mutex::new(VecDeque::new()),
            shutdown: AtomicBool::new(false),
            condvar: Condvar::new(),
            workers: Mutex::new(Vec::new()),
            worker_queues,
        });
        
        // Start worker threads
        {
            let mut workers = executor.workers.lock().unwrap();
            for id in 0..num_workers {
                let exec = Arc::clone(&executor);
                let stack_size = config.stack_size;
                let handle = thread::Builder::new()
                    .name(format!("roast-worker-{}", id))
                    .stack_size(stack_size)
                    .spawn(move || {
                        exec.worker_loop(id);
                    })
                    .expect("Failed to spawn worker thread");
                workers.push(handle);
            }
        }
        
        executor
    }
    
    /// Spawn a new task.
    pub fn spawn<F>(&self, future: F) -> TaskHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let id = self.next_task_id.fetch_add(1, Ordering::SeqCst);
        let task = Task::new(id, future);
        let handle = TaskHandle { task: Arc::clone(&task) };
        
        // Add to global queue
        {
            let mut queue = self.global_queue.lock().unwrap();
            queue.push_back(task);
        }
        
        // Wake a worker
        self.condvar.notify_one();
        
        handle
    }
    
    /// Block on a future.
    pub fn block_on<F, T>(&self, future: F) -> T
    where
        F: Future<Output = T>,
    {
        // Pin the future
        let mut future = std::pin::pin!(future);
        
        // Create a simple waker that does nothing
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        
        // Poll until ready
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(result) => return result,
                Poll::Pending => {
                    // Run some tasks while waiting
                    self.run_pending();
                    thread::yield_now();
                }
            }
        }
    }
    
    /// Run pending tasks.
    fn run_pending(&self) {
        let task = {
            let mut queue = self.global_queue.lock().unwrap();
            queue.pop_front()
        };
        
        if let Some(task) = task {
            if task.is_ready() && !task.is_completed() && !task.is_cancelled() {
                let waker = task_waker(Arc::clone(&task), self);
                let _ = task.poll(&waker);
                
                // Re-queue if not done
                if !task.is_completed() && !task.is_cancelled() {
                    let mut queue = self.global_queue.lock().unwrap();
                    queue.push_back(task);
                }
            }
        }
    }
    
    /// Worker thread main loop.
    fn worker_loop(self: &Arc<Self>, worker_id: usize) {
        while !self.shutdown.load(Ordering::SeqCst) {
            // Try to get a task
            let task = self.steal_task(worker_id);
            
            match task {
                Some(task) => {
                    if task.is_ready() && !task.is_completed() && !task.is_cancelled() {
                        let waker = task_waker(Arc::clone(&task), self);
                        let _ = task.poll(&waker);
                        
                        // Re-queue if not done
                        if !task.is_completed() && !task.is_cancelled() {
                            let mut queue = self.worker_queues[worker_id].lock().unwrap();
                            queue.push_back(task);
                        }
                    }
                }
                None => {
                    // Wait for work
                    let guard = self.global_queue.lock().unwrap();
                    let _ = self.condvar.wait_timeout(guard, Duration::from_millis(10));
                }
            }
        }
    }
    
    /// Steal a task (work-stealing).
    fn steal_task(&self, worker_id: usize) -> Option<Arc<Task>> {
        // First, try local queue
        {
            let mut queue = self.worker_queues[worker_id].lock().unwrap();
            if let Some(task) = queue.pop_front() {
                return Some(task);
            }
        }
        
        // Then, try global queue
        {
            let mut queue = self.global_queue.lock().unwrap();
            if let Some(task) = queue.pop_front() {
                return Some(task);
            }
        }
        
        // Finally, try stealing from other workers
        if self.config.work_stealing {
            for i in 0..self.worker_queues.len() {
                if i != worker_id {
                    let mut queue = self.worker_queues[i].lock().unwrap();
                    if let Some(task) = queue.pop_back() { // Steal from back
                        return Some(task);
                    }
                }
            }
        }
        
        None
    }
    
    /// Shutdown the executor.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.condvar.notify_all();
    }
    
    /// Wait for all workers to finish.
    pub fn join(self: Arc<Self>) {
        self.shutdown();
        
        // Wait for workers
        let workers = std::mem::take(&mut *self.workers.lock().unwrap());
        for worker in workers {
            let _ = worker.join();
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.condvar.notify_all();
    }
}

/// Create a waker for a task.
fn task_waker(task: Arc<Task>, _executor: &Executor) -> Waker {
    // Clone for the waker
    let task = Arc::into_raw(task);
    
    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        |ptr| {
            let task = unsafe { Arc::from_raw(ptr as *const Task) };
            let cloned = Arc::clone(&task);
            std::mem::forget(task); // Don't drop the original
            RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
        },
        |ptr| {
            let task = unsafe { Arc::from_raw(ptr as *const Task) };
            task.wake();
        },
        |ptr| {
            let task = unsafe { &*(ptr as *const Task) };
            task.wake();
        },
        |ptr| {
            let _ = unsafe { Arc::from_raw(ptr as *const Task) };
        },
    );
    
    let raw = RawWaker::new(task as *const (), &VTABLE);
    unsafe { Waker::from_raw(raw) }
}

/// Create a no-op waker.
fn noop_waker() -> Waker {
    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| RawWaker::new(std::ptr::null(), &VTABLE),
        |_| {},
        |_| {},
        |_| {},
    );
    
    let raw = RawWaker::new(std::ptr::null(), &VTABLE);
    unsafe { Waker::from_raw(raw) }
}

/// Sleep for a duration.
pub async fn sleep(duration: Duration) {
    let deadline = Instant::now() + duration;
    SleepFuture { deadline }.await
}

struct SleepFuture {
    deadline: Instant,
}

impl Future for SleepFuture {
    type Output = ();
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if Instant::now() >= self.deadline {
            Poll::Ready(())
        } else {
            // Schedule a wake-up
            let waker = cx.waker().clone();
            let deadline = self.deadline;
            thread::spawn(move || {
                let now = Instant::now();
                if deadline > now {
                    thread::sleep(deadline - now);
                }
                waker.wake();
            });
            Poll::Pending
        }
    }
}

/// Yield control back to the executor.
pub async fn yield_now() {
    YieldFuture { yielded: false }.await
}

struct YieldFuture {
    yielded: bool,
}

impl Future for YieldFuture {
    type Output = ();
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Join multiple futures.
pub async fn join_all<I, F, T>(futures: I) -> Vec<T>
where
    I: IntoIterator<Item = F>,
    F: Future<Output = T>,
{
    let mut futures: Vec<_> = futures.into_iter().map(|f| Box::pin(f)).collect();
    let mut results = Vec::with_capacity(futures.len());
    
    while !futures.is_empty() {
        let mut remaining = Vec::new();
        
        for mut future in futures {
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);
            
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(result) => results.push(result),
                Poll::Pending => remaining.push(future),
            }
        }
        
        futures = remaining;
        
        if !futures.is_empty() {
            yield_now().await;
        }
    }
    
    results
}

/// Select the first future to complete.
pub async fn select<A, B, T>(a: A, b: B) -> T
where
    A: Future<Output = T>,
    B: Future<Output = T>,
{
    let mut a = Box::pin(a);
    let mut b = Box::pin(b);
    
    loop {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        
        if let Poll::Ready(result) = a.as_mut().poll(&mut cx) {
            return result;
        }
        
        if let Poll::Ready(result) = b.as_mut().poll(&mut cx) {
            return result;
        }
        
        yield_now().await;
    }
}

/// Timeout wrapper.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    let deadline = Instant::now() + duration;
    let mut future = Box::pin(future);
    
    loop {
        if Instant::now() >= deadline {
            return Err(TimeoutError);
        }
        
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        
        if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
            return Ok(result);
        }
        
        yield_now().await;
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

/// Channel for async communication.
pub mod channel {
    use super::*;
    use std::collections::VecDeque;
    
    /// Create a bounded channel.
    pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let inner = Arc::new(Mutex::new(ChannelInner {
            queue: VecDeque::with_capacity(capacity),
            capacity,
            closed: false,
        }));
        
        (
            Sender { inner: Arc::clone(&inner) },
            Receiver { inner },
        )
    }
    
    /// Create an unbounded channel.
    pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
        bounded(usize::MAX)
    }
    
    struct ChannelInner<T> {
        queue: VecDeque<T>,
        capacity: usize,
        closed: bool,
    }
    
    /// Sender half of a channel.
    pub struct Sender<T> {
        inner: Arc<Mutex<ChannelInner<T>>>,
    }
    
    impl<T> Sender<T> {
        /// Send a value.
        pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
            loop {
                {
                    let mut inner = self.inner.lock().unwrap();
                    if inner.closed {
                        return Err(SendError(value));
                    }
                    if inner.queue.len() < inner.capacity {
                        inner.queue.push_back(value);
                        return Ok(());
                    }
                }
                yield_now().await;
            }
        }
        
        /// Close the channel.
        pub fn close(&self) {
            let mut inner = self.inner.lock().unwrap();
            inner.closed = true;
        }
    }
    
    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            Self { inner: Arc::clone(&self.inner) }
        }
    }
    
    /// Receiver half of a channel.
    pub struct Receiver<T> {
        inner: Arc<Mutex<ChannelInner<T>>>,
    }
    
    impl<T> Receiver<T> {
        /// Receive a value.
        pub async fn recv(&self) -> Option<T> {
            loop {
                {
                    let mut inner = self.inner.lock().unwrap();
                    if let Some(value) = inner.queue.pop_front() {
                        return Some(value);
                    }
                    if inner.closed {
                        return None;
                    }
                }
                yield_now().await;
            }
        }
        
        /// Try to receive without blocking.
        pub fn try_recv(&self) -> Option<T> {
            let mut inner = self.inner.lock().unwrap();
            inner.queue.pop_front()
        }
    }
    
    /// Send error.
    #[derive(Debug)]
    pub struct SendError<T>(pub T);
    
    impl<T> std::fmt::Display for SendError<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "channel closed")
        }
    }
    
    impl<T: std::fmt::Debug> std::error::Error for SendError<T> {}
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_executor_spawn() {
        let executor = Executor::new(ExecutorConfig::default());
        
        let (tx, rx) = std::sync::mpsc::channel();
        
        executor.spawn(async move {
            tx.send(42).unwrap();
        });
        
        // Give it time to run
        std::thread::sleep(Duration::from_millis(100));
        
        assert_eq!(rx.recv_timeout(Duration::from_millis(100)).unwrap(), 42);
        
        executor.shutdown();
    }
    
    #[test]
    fn test_sleep() {
        let executor = Executor::new(ExecutorConfig::default());
        
        let result = executor.block_on(async {
            let start = Instant::now();
            sleep(Duration::from_millis(50)).await;
            start.elapsed()
        });
        
        assert!(result >= Duration::from_millis(50));
        
        executor.shutdown();
    }
    
    #[test]
    fn test_channel() {
        use channel::*;
        
        let executor = Executor::new(ExecutorConfig::default());
        
        let result = executor.block_on(async {
            let (tx, rx) = bounded(10);
            
            tx.send(1).await.unwrap();
            tx.send(2).await.unwrap();
            tx.send(3).await.unwrap();
            
            let a = rx.recv().await.unwrap();
            let b = rx.recv().await.unwrap();
            let c = rx.recv().await.unwrap();
            
            a + b + c
        });
        
        assert_eq!(result, 6);
        
        executor.shutdown();
    }
}

