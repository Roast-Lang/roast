//! Synchronization primitives.
//!
//! Mutexes, RwLocks, Atomics, and more.

use std::sync::{
    Arc, Mutex as StdMutex, RwLock as StdRwLock,
    atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
    Condvar as StdCondvar, Barrier as StdBarrier, Once as StdOnce,
};
use std::cell::UnsafeCell;

/// Mutex guard.
pub struct MutexGuard<'a, T> {
    inner: std::sync::MutexGuard<'a, T>,
}

impl<'a, T> std::ops::Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<'a, T> std::ops::DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.inner
    }
}

/// Thread-safe mutex.
pub struct Mutex<T> {
    inner: StdMutex<T>,
}

impl<T> Mutex<T> {
    /// Create a new mutex.
    pub fn new(value: T) -> Self {
        Self { inner: StdMutex::new(value) }
    }
    
    /// Lock the mutex.
    pub fn lock(&self) -> MutexGuard<T> {
        MutexGuard { inner: self.inner.lock().unwrap() }
    }
    
    /// Try to lock without blocking.
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        self.inner.try_lock().ok().map(|g| MutexGuard { inner: g })
    }
    
    /// Check if the mutex is poisoned.
    pub fn is_poisoned(&self) -> bool {
        self.inner.is_poisoned()
    }
}

/// RwLock read guard.
pub struct RwLockReadGuard<'a, T> {
    inner: std::sync::RwLockReadGuard<'a, T>,
}

impl<'a, T> std::ops::Deref for RwLockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner
    }
}

/// RwLock write guard.
pub struct RwLockWriteGuard<'a, T> {
    inner: std::sync::RwLockWriteGuard<'a, T>,
}

impl<'a, T> std::ops::Deref for RwLockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<'a, T> std::ops::DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.inner
    }
}

/// Read-write lock.
pub struct RwLock<T> {
    inner: StdRwLock<T>,
}

impl<T> RwLock<T> {
    /// Create a new RwLock.
    pub fn new(value: T) -> Self {
        Self { inner: StdRwLock::new(value) }
    }
    
    /// Acquire read lock.
    pub fn read(&self) -> RwLockReadGuard<T> {
        RwLockReadGuard { inner: self.inner.read().unwrap() }
    }
    
    /// Acquire write lock.
    pub fn write(&self) -> RwLockWriteGuard<T> {
        RwLockWriteGuard { inner: self.inner.write().unwrap() }
    }
    
    /// Try to acquire read lock.
    pub fn try_read(&self) -> Option<RwLockReadGuard<T>> {
        self.inner.try_read().ok().map(|g| RwLockReadGuard { inner: g })
    }
    
    /// Try to acquire write lock.
    pub fn try_write(&self) -> Option<RwLockWriteGuard<T>> {
        self.inner.try_write().ok().map(|g| RwLockWriteGuard { inner: g })
    }
}

/// Condition variable.
pub struct Condvar {
    inner: StdCondvar,
}

impl Condvar {
    /// Create a new condition variable.
    pub fn new() -> Self {
        Self { inner: StdCondvar::new() }
    }
    
    /// Wait on the condition variable.
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        MutexGuard {
            inner: self.inner.wait(guard.inner).unwrap()
        }
    }
    
    /// Notify one waiting thread.
    pub fn notify_one(&self) {
        self.inner.notify_one();
    }
    
    /// Notify all waiting threads.
    pub fn notify_all(&self) {
        self.inner.notify_all();
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

/// Barrier for synchronizing threads.
pub struct Barrier {
    inner: StdBarrier,
}

impl Barrier {
    /// Create a new barrier for n threads.
    pub fn new(n: usize) -> Self {
        Self { inner: StdBarrier::new(n) }
    }
    
    /// Wait at the barrier.
    pub fn wait(&self) -> bool {
        self.inner.wait().is_leader()
    }
}

/// Once - execute code exactly once.
pub struct Once {
    inner: StdOnce,
}

impl Once {
    /// Create a new Once.
    pub const fn new() -> Self {
        Self { inner: StdOnce::new() }
    }
    
    /// Execute the closure exactly once.
    pub fn call_once<F: FnOnce()>(&self, f: F) {
        self.inner.call_once(f);
    }
    
    /// Check if call_once has been called.
    pub fn is_completed(&self) -> bool {
        self.inner.is_completed()
    }
}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

/// Atomic boolean.
pub struct AtomicFlag {
    inner: AtomicBool,
}

impl AtomicFlag {
    /// Create a new atomic flag.
    pub const fn new(value: bool) -> Self {
        Self { inner: AtomicBool::new(value) }
    }
    
    /// Load the value.
    pub fn load(&self) -> bool {
        self.inner.load(Ordering::SeqCst)
    }
    
    /// Store a value.
    pub fn store(&self, value: bool) {
        self.inner.store(value, Ordering::SeqCst);
    }
    
    /// Swap and return old value.
    pub fn swap(&self, value: bool) -> bool {
        self.inner.swap(value, Ordering::SeqCst)
    }
    
    /// Compare and swap.
    pub fn compare_exchange(&self, current: bool, new: bool) -> Result<bool, bool> {
        self.inner.compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
    }
}

impl Default for AtomicFlag {
    fn default() -> Self {
        Self::new(false)
    }
}

/// Atomic counter.
pub struct AtomicCounter {
    inner: AtomicI64,
}

impl AtomicCounter {
    /// Create a new counter.
    pub const fn new(value: i64) -> Self {
        Self { inner: AtomicI64::new(value) }
    }
    
    /// Load the value.
    pub fn load(&self) -> i64 {
        self.inner.load(Ordering::SeqCst)
    }
    
    /// Store a value.
    pub fn store(&self, value: i64) {
        self.inner.store(value, Ordering::SeqCst);
    }
    
    /// Add and return new value.
    pub fn add(&self, n: i64) -> i64 {
        self.inner.fetch_add(n, Ordering::SeqCst) + n
    }
    
    /// Subtract and return new value.
    pub fn sub(&self, n: i64) -> i64 {
        self.inner.fetch_sub(n, Ordering::SeqCst) - n
    }
    
    /// Increment and return new value.
    pub fn increment(&self) -> i64 {
        self.add(1)
    }
    
    /// Decrement and return new value.
    pub fn decrement(&self) -> i64 {
        self.sub(1)
    }
    
    /// Swap and return old value.
    pub fn swap(&self, value: i64) -> i64 {
        self.inner.swap(value, Ordering::SeqCst)
    }
}

impl Default for AtomicCounter {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Semaphore.
pub struct Semaphore {
    permits: Mutex<usize>,
    condvar: Condvar,
    max_permits: usize,
}

impl Semaphore {
    /// Create a new semaphore with n permits.
    pub fn new(permits: usize) -> Self {
        Self {
            permits: Mutex::new(permits),
            condvar: Condvar::new(),
            max_permits: permits,
        }
    }
    
    /// Acquire a permit.
    pub fn acquire(&self) {
        let mut permits = self.permits.lock();
        while *permits == 0 {
            permits = self.condvar.wait(permits);
        }
        *permits -= 1;
    }
    
    /// Try to acquire a permit without blocking.
    pub fn try_acquire(&self) -> bool {
        let mut permits = self.permits.lock();
        if *permits > 0 {
            *permits -= 1;
            true
        } else {
            false
        }
    }
    
    /// Release a permit.
    pub fn release(&self) {
        let mut permits = self.permits.lock();
        if *permits < self.max_permits {
            *permits += 1;
            self.condvar.notify_one();
        }
    }
    
    /// Get available permits.
    pub fn available(&self) -> usize {
        *self.permits.lock()
    }
}

/// WaitGroup for waiting on multiple tasks.
pub struct WaitGroup {
    count: AtomicCounter,
    condvar: Condvar,
    mutex: Mutex<()>,
}

impl WaitGroup {
    /// Create a new WaitGroup.
    pub fn new() -> Self {
        Self {
            count: AtomicCounter::new(0),
            condvar: Condvar::new(),
            mutex: Mutex::new(()),
        }
    }
    
    /// Add n tasks.
    pub fn add(&self, n: i64) {
        self.count.add(n);
    }
    
    /// Mark one task as done.
    pub fn done(&self) {
        if self.count.decrement() <= 0 {
            self.condvar.notify_all();
        }
    }
    
    /// Wait for all tasks to complete.
    pub fn wait(&self) {
        let mut guard = self.mutex.lock();
        while self.count.load() > 0 {
            guard = self.condvar.wait(guard);
        }
    }
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Deadlock Detection
// =============================================================================

use std::collections::{HashMap, HashSet};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

/// Lock identifier for tracking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LockId(pub u64);

impl LockId {
    /// Generate a new unique lock ID.
    pub fn new() -> Self {
        use std::sync::atomic::AtomicU64;
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        LockId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for LockId {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a lock hold or wait.
#[derive(Clone, Debug)]
pub struct LockInfo {
    /// The lock's unique identifier.
    pub lock_id: LockId,
    /// Human-readable name for the lock.
    pub name: String,
    /// Location where the lock was created.
    pub created_at: String,
    /// When the lock was acquired.
    pub acquired_at: Option<Instant>,
    /// Stack trace when acquired (if enabled).
    pub stack_trace: Option<String>,
}

/// Edge in the wait-for graph.
#[derive(Clone, Debug)]
struct WaitForEdge {
    /// Thread waiting for the lock.
    waiter: ThreadId,
    /// Thread holding the lock.
    holder: ThreadId,
    /// The lock being waited for.
    lock_id: LockId,
    /// When the wait started.
    started_at: Instant,
}

/// Deadlock detection result.
#[derive(Clone, Debug)]
pub struct DeadlockInfo {
    /// Threads involved in the deadlock cycle.
    pub threads: Vec<ThreadId>,
    /// Locks involved in the deadlock cycle.
    pub locks: Vec<LockId>,
    /// Human-readable description.
    pub description: String,
    /// When the deadlock was detected.
    pub detected_at: Instant,
}

/// Global deadlock detector.
pub struct DeadlockDetector {
    /// Locks currently held by each thread.
    held_locks: StdMutex<HashMap<ThreadId, Vec<LockId>>>,
    /// Lock wait graph: thread -> (lock_id, holder_thread)
    waiting_for: StdMutex<HashMap<ThreadId, (LockId, ThreadId)>>,
    /// Lock metadata.
    lock_info: StdMutex<HashMap<LockId, LockInfo>>,
    /// Lock acquisition order for each thread (for lock order detection).
    lock_order: StdMutex<HashMap<ThreadId, Vec<LockId>>>,
    /// Global lock ordering (for detecting potential deadlocks).
    global_order: StdMutex<HashSet<(LockId, LockId)>>,
    /// Whether detection is enabled.
    enabled: AtomicBool,
    /// Detected deadlocks.
    deadlocks: StdMutex<Vec<DeadlockInfo>>,
}

impl DeadlockDetector {
    /// Create a new deadlock detector.
    pub fn new() -> Self {
        Self {
            held_locks: StdMutex::new(HashMap::new()),
            waiting_for: StdMutex::new(HashMap::new()),
            lock_info: StdMutex::new(HashMap::new()),
            lock_order: StdMutex::new(HashMap::new()),
            global_order: StdMutex::new(HashSet::new()),
            enabled: AtomicBool::new(true),
            deadlocks: StdMutex::new(Vec::new()),
        }
    }
    
    /// Enable or disable deadlock detection.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }
    
    /// Check if detection is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }
    
    /// Register a new lock.
    pub fn register_lock(&self, lock_id: LockId, name: &str, location: &str) {
        if !self.is_enabled() {
            return;
        }
        
        let info = LockInfo {
            lock_id,
            name: name.to_string(),
            created_at: location.to_string(),
            acquired_at: None,
            stack_trace: None,
        };
        
        self.lock_info.lock().unwrap().insert(lock_id, info);
    }
    
    /// Record that a thread is about to wait for a lock.
    pub fn before_lock(&self, lock_id: LockId, holder: Option<ThreadId>) {
        if !self.is_enabled() {
            return;
        }
        
        let thread_id = std::thread::current().id();
        
        if let Some(holder_id) = holder {
            // Record the wait
            self.waiting_for.lock().unwrap().insert(thread_id, (lock_id, holder_id));
            
            // Check for deadlock
            if let Some(deadlock) = self.detect_cycle(thread_id) {
                self.deadlocks.lock().unwrap().push(deadlock);
            }
        }
    }
    
    /// Record that a thread has acquired a lock.
    pub fn after_lock(&self, lock_id: LockId) {
        if !self.is_enabled() {
            return;
        }
        
        let thread_id = std::thread::current().id();
        
        // Remove from waiting
        self.waiting_for.lock().unwrap().remove(&thread_id);
        
        // Add to held locks
        {
            let mut held = self.held_locks.lock().unwrap();
            held.entry(thread_id).or_insert_with(Vec::new).push(lock_id);
        }
        
        // Track lock order for this thread
        {
            let mut order = self.lock_order.lock().unwrap();
            let thread_order = order.entry(thread_id).or_insert_with(Vec::new);
            
            // Record all pairs in global order
            let mut global = self.global_order.lock().unwrap();
            for &prev_lock in thread_order.iter() {
                // Check for order violation
                if global.contains(&(lock_id, prev_lock)) {
                    // Potential deadlock: lock_id was previously acquired before prev_lock
                    // but now we're acquiring it after
                    self.record_order_violation(prev_lock, lock_id, thread_id);
                }
                global.insert((prev_lock, lock_id));
            }
            
            thread_order.push(lock_id);
        }
        
        // Update lock info
        if let Some(info) = self.lock_info.lock().unwrap().get_mut(&lock_id) {
            info.acquired_at = Some(Instant::now());
        }
    }
    
    /// Record that a thread has released a lock.
    pub fn after_unlock(&self, lock_id: LockId) {
        if !self.is_enabled() {
            return;
        }
        
        let thread_id = std::thread::current().id();
        
        // Remove from held locks
        if let Some(held) = self.held_locks.lock().unwrap().get_mut(&thread_id) {
            if let Some(pos) = held.iter().position(|&id| id == lock_id) {
                held.remove(pos);
            }
        }
        
        // Remove from lock order
        if let Some(order) = self.lock_order.lock().unwrap().get_mut(&thread_id) {
            if let Some(pos) = order.iter().position(|&id| id == lock_id) {
                order.remove(pos);
            }
        }
    }
    
    /// Detect a cycle in the wait-for graph starting from a thread.
    fn detect_cycle(&self, start: ThreadId) -> Option<DeadlockInfo> {
        let waiting = self.waiting_for.lock().unwrap();
        let held = self.held_locks.lock().unwrap();
        
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        let mut lock_path = Vec::new();
        let mut current = start;
        
        loop {
            if visited.contains(&current) {
                // Found a cycle
                // Trim path to just the cycle
                if let Some(pos) = path.iter().position(|&t| t == current) {
                    let cycle_threads: Vec<_> = path[pos..].to_vec();
                    let cycle_locks: Vec<_> = lock_path[pos..].to_vec();
                    
                    let description = self.format_deadlock(&cycle_threads, &cycle_locks);
                    
                    return Some(DeadlockInfo {
                        threads: cycle_threads,
                        locks: cycle_locks,
                        description,
                        detected_at: Instant::now(),
                    });
                }
                break;
            }
            
            visited.insert(current);
            path.push(current);
            
            if let Some(&(lock_id, holder)) = waiting.get(&current) {
                lock_path.push(lock_id);
                current = holder;
            } else {
                break;
            }
        }
        
        None
    }
    
    /// Format a deadlock for display.
    fn format_deadlock(&self, threads: &[ThreadId], locks: &[LockId]) -> String {
        let lock_info = self.lock_info.lock().unwrap();
        
        let mut desc = String::from("Deadlock detected!\n");
        desc.push_str("Cycle:\n");
        
        for (i, (thread, lock)) in threads.iter().zip(locks.iter()).enumerate() {
            let lock_name = lock_info.get(lock)
                .map(|info| info.name.as_str())
                .unwrap_or("unknown");
            let next_thread = threads.get(i + 1).unwrap_or(&threads[0]);
            
            desc.push_str(&format!(
                "  Thread {:?} is waiting for lock '{}' held by Thread {:?}\n",
                thread, lock_name, next_thread
            ));
        }
        
        desc
    }
    
    /// Record a lock order violation (potential deadlock).
    fn record_order_violation(&self, lock1: LockId, lock2: LockId, thread: ThreadId) {
        let lock_info = self.lock_info.lock().unwrap();
        let name1 = lock_info.get(&lock1).map(|i| i.name.as_str()).unwrap_or("?");
        let name2 = lock_info.get(&lock2).map(|i| i.name.as_str()).unwrap_or("?");
        
        eprintln!(
            "Warning: Lock order violation detected on thread {:?}:\n\
             Lock '{}' was previously acquired before '{}',\n\
             but now '{}' is being acquired while '{}' is held.\n\
             This could lead to a deadlock.",
            thread, name2, name1, name2, name1
        );
    }
    
    /// Get all detected deadlocks.
    pub fn get_deadlocks(&self) -> Vec<DeadlockInfo> {
        self.deadlocks.lock().unwrap().clone()
    }
    
    /// Clear all detected deadlocks.
    pub fn clear_deadlocks(&self) {
        self.deadlocks.lock().unwrap().clear();
    }
    
    /// Get the locks currently held by a thread.
    pub fn get_held_locks(&self, thread: ThreadId) -> Vec<LockId> {
        self.held_locks.lock().unwrap()
            .get(&thread)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Check if any thread is currently waiting.
    pub fn has_waiting_threads(&self) -> bool {
        !self.waiting_for.lock().unwrap().is_empty()
    }
    
    /// Get summary of current lock state.
    pub fn get_summary(&self) -> String {
        let held = self.held_locks.lock().unwrap();
        let waiting = self.waiting_for.lock().unwrap();
        let deadlocks = self.deadlocks.lock().unwrap();
        
        format!(
            "DeadlockDetector Summary:\n\
             - Active threads holding locks: {}\n\
             - Threads waiting for locks: {}\n\
             - Deadlocks detected: {}",
            held.len(),
            waiting.len(),
            deadlocks.len()
        )
    }
}

impl Default for DeadlockDetector {
    fn default() -> Self {
        Self::new()
    }
}

// Global deadlock detector instance
lazy_static::lazy_static! {
    /// Global deadlock detector.
    pub static ref DEADLOCK_DETECTOR: DeadlockDetector = DeadlockDetector::new();
}

/// A mutex with built-in deadlock detection.
pub struct TrackedMutex<T> {
    inner: StdMutex<T>,
    lock_id: LockId,
    holder: StdMutex<Option<ThreadId>>,
}

impl<T> TrackedMutex<T> {
    /// Create a new tracked mutex.
    pub fn new(value: T, name: &str) -> Self {
        let lock_id = LockId::new();
        DEADLOCK_DETECTOR.register_lock(lock_id, name, "");
        
        Self {
            inner: StdMutex::new(value),
            lock_id,
            holder: StdMutex::new(None),
        }
    }
    
    /// Lock with deadlock detection.
    pub fn lock(&self) -> TrackedMutexGuard<T> {
        let current_holder = *self.holder.lock().unwrap();
        DEADLOCK_DETECTOR.before_lock(self.lock_id, current_holder);
        
        let guard = self.inner.lock().unwrap();
        
        *self.holder.lock().unwrap() = Some(std::thread::current().id());
        DEADLOCK_DETECTOR.after_lock(self.lock_id);
        
        TrackedMutexGuard {
            inner: guard,
            lock_id: self.lock_id,
            holder: &self.holder,
        }
    }
    
    /// Try to lock without blocking.
    pub fn try_lock(&self) -> Option<TrackedMutexGuard<T>> {
        self.inner.try_lock().ok().map(|guard| {
            *self.holder.lock().unwrap() = Some(std::thread::current().id());
            DEADLOCK_DETECTOR.after_lock(self.lock_id);
            
            TrackedMutexGuard {
                inner: guard,
                lock_id: self.lock_id,
                holder: &self.holder,
            }
        })
    }
}

/// Guard for TrackedMutex.
pub struct TrackedMutexGuard<'a, T> {
    inner: std::sync::MutexGuard<'a, T>,
    lock_id: LockId,
    holder: &'a StdMutex<Option<ThreadId>>,
}

impl<'a, T> std::ops::Deref for TrackedMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<'a, T> std::ops::DerefMut for TrackedMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.inner
    }
}

impl<'a, T> Drop for TrackedMutexGuard<'a, T> {
    fn drop(&mut self) {
        *self.holder.lock().unwrap() = None;
        DEADLOCK_DETECTOR.after_unlock(self.lock_id);
    }
}

// =============================================================================
// Lock Timeout Support
// =============================================================================

/// A mutex with timeout support to prevent deadlocks.
pub struct TimeoutMutex<T> {
    inner: StdMutex<T>,
    default_timeout: Duration,
}

impl<T> TimeoutMutex<T> {
    /// Create a new timeout mutex.
    pub fn new(value: T, timeout: Duration) -> Self {
        Self {
            inner: StdMutex::new(value),
            default_timeout: timeout,
        }
    }
    
    /// Try to lock with the default timeout.
    pub fn lock(&self) -> Result<TimeoutMutexGuard<T>, LockTimeoutError> {
        self.lock_timeout(self.default_timeout)
    }
    
    /// Try to lock with a specific timeout.
    pub fn lock_timeout(&self, timeout: Duration) -> Result<TimeoutMutexGuard<T>, LockTimeoutError> {
        let start = Instant::now();
        let spin_duration = Duration::from_micros(100);
        
        loop {
            if let Ok(guard) = self.inner.try_lock() {
                return Ok(TimeoutMutexGuard { inner: guard });
            }
            
            if start.elapsed() >= timeout {
                return Err(LockTimeoutError {
                    timeout,
                    elapsed: start.elapsed(),
                });
            }
            
            std::thread::sleep(spin_duration);
        }
    }
    
    /// Try to lock without blocking.
    pub fn try_lock(&self) -> Option<TimeoutMutexGuard<T>> {
        self.inner.try_lock().ok().map(|g| TimeoutMutexGuard { inner: g })
    }
}

/// Error when lock acquisition times out.
#[derive(Debug)]
pub struct LockTimeoutError {
    pub timeout: Duration,
    pub elapsed: Duration,
}

impl std::fmt::Display for LockTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lock acquisition timed out after {:?} (limit: {:?})", 
            self.elapsed, self.timeout)
    }
}

impl std::error::Error for LockTimeoutError {}

/// Guard for TimeoutMutex.
pub struct TimeoutMutexGuard<'a, T> {
    inner: std::sync::MutexGuard<'a, T>,
}

impl<'a, T> std::ops::Deref for TimeoutMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<'a, T> std::ops::DerefMut for TimeoutMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_deadlock_detector_basic() {
        let detector = DeadlockDetector::new();
        
        let lock1 = LockId::new();
        let lock2 = LockId::new();
        
        detector.register_lock(lock1, "lock1", "test");
        detector.register_lock(lock2, "lock2", "test");
        
        // Simulate acquiring locks
        detector.after_lock(lock1);
        detector.after_lock(lock2);
        
        // Release locks
        detector.after_unlock(lock2);
        detector.after_unlock(lock1);
        
        assert!(detector.get_deadlocks().is_empty());
    }
    
    #[test]
    fn test_tracked_mutex() {
        let mutex = TrackedMutex::new(42, "test_mutex");
        
        {
            let mut guard = mutex.lock();
            *guard = 100;
        }
        
        let guard = mutex.lock();
        assert_eq!(*guard, 100);
    }
    
    #[test]
    fn test_timeout_mutex() {
        let mutex = TimeoutMutex::new(42, Duration::from_millis(100));
        
        let guard = mutex.lock().unwrap();
        assert_eq!(*guard, 42);
        drop(guard);
        
        // Try lock should succeed
        assert!(mutex.try_lock().is_some());
    }
    
    #[test]
    fn test_lock_timeout_error() {
        use std::thread;
        use std::sync::Arc;
        
        let mutex = Arc::new(TimeoutMutex::new(42, Duration::from_millis(50)));
        let mutex2 = Arc::clone(&mutex);
        
        // Hold the lock in another thread
        let handle = thread::spawn(move || {
            let _guard = mutex2.lock().unwrap();
            thread::sleep(Duration::from_millis(200));
        });
        
        // Give the other thread time to acquire
        thread::sleep(Duration::from_millis(10));
        
        // This should timeout
        let result = mutex.lock();
        assert!(result.is_err());
        
        handle.join().unwrap();
    }
}