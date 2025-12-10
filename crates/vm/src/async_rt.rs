//! Async runtime for Roast.
//!
//! Provides async/await support for the VM.

use roast_runtime::Value;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::sync::{Arc, Mutex};

/// A task in the async runtime.
pub struct Task {
    /// Unique task ID.
    pub id: u64,
    /// The task state.
    pub state: TaskState,
    /// The result value (when completed).
    pub result: Option<Value>,
    /// Waker for this task.
    pub waker: Option<Waker>,
}

/// State of an async task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    /// Task is ready to run.
    Ready,
    /// Task is waiting for something.
    Pending,
    /// Task completed successfully.
    Completed,
    /// Task failed with an error.
    Failed(String),
    /// Task was cancelled.
    Cancelled,
}

impl Task {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            state: TaskState::Ready,
            result: None,
            waker: None,
        }
    }

    pub fn complete(&mut self, value: Value) {
        self.state = TaskState::Completed;
        self.result = Some(value);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn fail(&mut self, error: String) {
        self.state = TaskState::Failed(error);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn is_done(&self) -> bool {
        matches!(self.state, TaskState::Completed | TaskState::Failed(_) | TaskState::Cancelled)
    }
}

/// The async runtime scheduler.
pub struct AsyncRuntime {
    /// Ready queue of tasks.
    ready_queue: VecDeque<u64>,
    /// All tasks by ID.
    tasks: rustc_hash::FxHashMap<u64, Task>,
    /// Next task ID.
    next_id: u64,
}

impl AsyncRuntime {
    /// Creates a new async runtime.
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
            tasks: rustc_hash::FxHashMap::default(),
            next_id: 1,
        }
    }

    /// Spawns a new task.
    pub fn spawn(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let task = Task::new(id);
        self.tasks.insert(id, task);
        self.ready_queue.push_back(id);
        id
    }

    /// Gets a task by ID.
    pub fn get_task(&self, id: u64) -> Option<&Task> {
        self.tasks.get(&id)
    }

    /// Gets a mutable task by ID.
    pub fn get_task_mut(&mut self, id: u64) -> Option<&mut Task> {
        self.tasks.get_mut(&id)
    }

    /// Marks a task as ready.
    pub fn wake(&mut self, id: u64) {
        if let Some(task) = self.tasks.get_mut(&id) {
            if task.state == TaskState::Pending {
                task.state = TaskState::Ready;
                self.ready_queue.push_back(id);
            }
        }
    }

    /// Gets the next ready task.
    pub fn next_ready(&mut self) -> Option<u64> {
        self.ready_queue.pop_front()
    }

    /// Completes a task with a value.
    pub fn complete(&mut self, id: u64, value: Value) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.complete(value);
        }
    }

    /// Fails a task with an error.
    pub fn fail(&mut self, id: u64, error: String) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.fail(error);
        }
    }

    /// Cancels a task.
    pub fn cancel(&mut self, id: u64) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.state = TaskState::Cancelled;
        }
    }

    /// Returns the number of pending tasks.
    pub fn pending_count(&self) -> usize {
        self.tasks.values().filter(|t| !t.is_done()).count()
    }

    /// Returns true if all tasks are complete.
    pub fn is_complete(&self) -> bool {
        self.tasks.values().all(|t| t.is_done())
    }

    /// Runs until all tasks complete.
    pub fn run_until_complete(&mut self) {
        while !self.is_complete() {
            if let Some(task_id) = self.next_ready() {
                // In a real implementation, we'd execute the task here
                // For now, just mark it as pending
                if let Some(task) = self.tasks.get_mut(&task_id) {
                    if task.state == TaskState::Ready {
                        task.state = TaskState::Pending;
                    }
                }
            } else {
                // No ready tasks, would normally block on I/O here
                break;
            }
        }
    }
}

impl Default for AsyncRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// A future that resolves after a delay.
pub struct Sleep {
    duration_ms: u64,
    started: bool,
}

impl Sleep {
    pub fn new(duration_ms: u64) -> Self {
        Self {
            duration_ms,
            started: false,
        }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.started {
            self.started = true;
            // In a real implementation, we'd register a timer here
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

/// Channel for async communication.
pub struct Channel<T> {
    inner: Arc<Mutex<ChannelInner<T>>>,
}

struct ChannelInner<T> {
    queue: VecDeque<T>,
    closed: bool,
    send_wakers: Vec<Waker>,
    recv_wakers: Vec<Waker>,
    capacity: usize,
}

impl<T> Channel<T> {
    /// Creates a new unbounded channel.
    pub fn unbounded() -> (Sender<T>, Receiver<T>) {
        let inner = Arc::new(Mutex::new(ChannelInner {
            queue: VecDeque::new(),
            closed: false,
            send_wakers: Vec::new(),
            recv_wakers: Vec::new(),
            capacity: usize::MAX,
        }));
        (
            Sender { inner: inner.clone() },
            Receiver { inner },
        )
    }

    /// Creates a bounded channel.
    pub fn bounded(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let inner = Arc::new(Mutex::new(ChannelInner {
            queue: VecDeque::new(),
            closed: false,
            send_wakers: Vec::new(),
            recv_wakers: Vec::new(),
            capacity,
        }));
        (
            Sender { inner: inner.clone() },
            Receiver { inner },
        )
    }
}

/// Sender end of a channel.
pub struct Sender<T> {
    inner: Arc<Mutex<ChannelInner<T>>>,
}

impl<T> Sender<T> {
    /// Sends a value.
    pub fn send(&self, value: T) -> Result<(), T> {
        let mut inner = self.inner.lock().unwrap();
        if inner.closed {
            return Err(value);
        }
        if inner.queue.len() >= inner.capacity {
            return Err(value);
        }
        inner.queue.push_back(value);
        for waker in inner.recv_wakers.drain(..) {
            waker.wake();
        }
        Ok(())
    }

    /// Closes the sender.
    pub fn close(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.closed = true;
        for waker in inner.recv_wakers.drain(..) {
            waker.wake();
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

/// Receiver end of a channel.
pub struct Receiver<T> {
    inner: Arc<Mutex<ChannelInner<T>>>,
}

impl<T> Receiver<T> {
    /// Tries to receive a value.
    pub fn try_recv(&self) -> Option<T> {
        let mut inner = self.inner.lock().unwrap();
        inner.queue.pop_front()
    }

    /// Checks if the channel is closed and empty.
    pub fn is_closed(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.closed && inner.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_runtime() {
        let mut rt = AsyncRuntime::new();
        let task_id = rt.spawn();
        assert!(!rt.is_complete());
        rt.complete(task_id, Value::Int(42));
        assert!(rt.is_complete());
    }

    #[test]
    fn test_channel() {
        let (tx, rx) = Channel::<i32>::unbounded();
        tx.send(42).unwrap();
        assert_eq!(rx.try_recv(), Some(42));
        assert_eq!(rx.try_recv(), None);
    }
}

