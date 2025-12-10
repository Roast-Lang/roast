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

