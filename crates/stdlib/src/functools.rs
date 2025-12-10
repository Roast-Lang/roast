//! Function utilities (Python's functools equivalent).
//!
//! Higher-order functions and operations on callable objects.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// =============================================================================
// Reduce
// =============================================================================

/// Reduce an iterable to a single value.
pub fn reduce<I, T, F>(iter: I, func: F) -> Option<T>
where
    I: IntoIterator<Item = T>,
    F: Fn(T, T) -> T,
{
    let mut iter = iter.into_iter();
    let mut acc = iter.next()?;
    
    for item in iter {
        acc = func(acc, item);
    }
    
    Some(acc)
}

/// Reduce with an initial value.
pub fn reduce_with<I, T, A, F>(iter: I, initial: A, func: F) -> A
where
    I: IntoIterator<Item = T>,
    F: Fn(A, T) -> A,
{
    iter.into_iter().fold(initial, func)
}

// =============================================================================
// Partial Application
// =============================================================================

/// Partial application wrapper.
pub struct Partial<F, A> {
    func: F,
    args: A,
}

impl<F, A> Partial<F, A> {
    pub fn new(func: F, args: A) -> Self {
        Self { func, args }
    }
}

/// Create a partial function with one argument.
pub fn partial1<A, B, R, F>(func: F, arg: A) -> impl Fn(B) -> R
where
    F: Fn(A, B) -> R,
    A: Clone,
{
    move |b| func(arg.clone(), b)
}

/// Create a partial function with two arguments.
pub fn partial2<A, B, C, R, F>(func: F, a: A, b: B) -> impl Fn(C) -> R
where
    F: Fn(A, B, C) -> R,
    A: Clone,
    B: Clone,
{
    move |c| func(a.clone(), b.clone(), c)
}

// =============================================================================
// Memoization / Caching
// =============================================================================

/// Memoized function wrapper (thread-safe).
pub struct Memoized<F, A, R>
where
    A: Eq + Hash + Clone,
    R: Clone,
{
    func: F,
    cache: Arc<Mutex<HashMap<A, R>>>,
}

impl<F, A, R> Memoized<F, A, R>
where
    A: Eq + Hash + Clone,
    R: Clone,
    F: Fn(A) -> R,
{
    pub fn new(func: F) -> Self {
        Self {
            func,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn call(&self, arg: A) -> R {
        let mut cache = self.cache.lock().unwrap();
        
        if let Some(result) = cache.get(&arg) {
            return result.clone();
        }
        
        let result = (self.func)(arg.clone());
        cache.insert(arg, result.clone());
        result
    }
    
    pub fn cache_size(&self) -> usize {
        self.cache.lock().unwrap().len()
    }
    
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
    }
}

/// Create a memoized function.
pub fn memoize<F, A, R>(func: F) -> Memoized<F, A, R>
where
    A: Eq + Hash + Clone,
    R: Clone,
    F: Fn(A) -> R,
{
    Memoized::new(func)
}

/// LRU cache wrapper.
pub struct LruCache<A, R>
where
    A: Eq + Hash + Clone,
    R: Clone,
{
    cache: Arc<Mutex<LruCacheInner<A, R>>>,
    max_size: usize,
}

struct LruCacheInner<A, R> {
    entries: HashMap<A, (R, u64)>,
    access_counter: u64,
}

impl<A, R> LruCache<A, R>
where
    A: Eq + Hash + Clone,
    R: Clone,
{
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCacheInner {
                entries: HashMap::new(),
                access_counter: 0,
            })),
            max_size,
        }
    }
    
    pub fn get_or_insert<F>(&self, key: A, compute: F) -> R
    where
        F: FnOnce() -> R,
    {
        let mut inner = self.cache.lock().unwrap();
        inner.access_counter += 1;
        let access = inner.access_counter;
        
        if let Some((value, last_access)) = inner.entries.get_mut(&key) {
            *last_access = access;
            return value.clone();
        }
        
        // Evict if full
        while inner.entries.len() >= self.max_size {
            let oldest = inner.entries.iter()
                .min_by_key(|(_, (_, access))| access)
                .map(|(k, _)| k.clone());
            
            if let Some(k) = oldest {
                inner.entries.remove(&k);
            }
        }
        
        let value = compute();
        inner.entries.insert(key, (value.clone(), access));
        value
    }
    
    pub fn cache_info(&self) -> (usize, usize) {
        let inner = self.cache.lock().unwrap();
        (inner.entries.len(), self.max_size)
    }
    
    pub fn clear(&self) {
        self.cache.lock().unwrap().entries.clear();
    }
}

/// Create an LRU-cached function.
pub fn lru_cache<A, R, F>(max_size: usize, func: F) -> impl Fn(A) -> R
where
    A: Eq + Hash + Clone,
    R: Clone,
    F: Fn(A) -> R,
{
    let cache = LruCache::new(max_size);
    move |arg: A| {
        cache.get_or_insert(arg.clone(), || func(arg))
    }
}

// =============================================================================
// Function Composition
// =============================================================================

/// Compose two functions: (f ∘ g)(x) = f(g(x))
pub fn compose<A, B, C, F, G>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(B) -> C,
    G: Fn(A) -> B,
{
    move |a| f(g(a))
}

/// Compose multiple functions.
pub fn compose_all<T, F>(funcs: Vec<F>) -> impl Fn(T) -> T
where
    F: Fn(T) -> T,
{
    move |x| {
        funcs.iter().rev().fold(x, |acc, f| f(acc))
    }
}

/// Pipe: apply functions left to right.
pub fn pipe<A, B, C, F, G>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
{
    move |a| g(f(a))
}

// =============================================================================
// Higher-Order Functions
// =============================================================================

/// Identity function.
pub fn identity<T>(x: T) -> T {
    x
}

/// Constant function - always returns the same value.
pub fn constant<T: Clone>(value: T) -> impl Fn() -> T {
    move || value.clone()
}

/// Flip the arguments of a binary function.
pub fn flip<A, B, R, F>(func: F) -> impl Fn(B, A) -> R
where
    F: Fn(A, B) -> R,
{
    move |b, a| func(a, b)
}

/// Apply a function n times.
pub fn apply_n<T, F>(n: usize, func: F, initial: T) -> T
where
    F: Fn(T) -> T,
{
    (0..n).fold(initial, |acc, _| func(acc))
}

/// Negate a predicate.
pub fn negate<T, P>(predicate: P) -> impl Fn(&T) -> bool
where
    P: Fn(&T) -> bool,
{
    move |x| !predicate(x)
}

/// Combine predicates with AND.
pub fn all_of<T, P>(predicates: Vec<P>) -> impl Fn(&T) -> bool
where
    P: Fn(&T) -> bool,
{
    move |x| predicates.iter().all(|p| p(x))
}

/// Combine predicates with OR.
pub fn any_of<T, P>(predicates: Vec<P>) -> impl Fn(&T) -> bool
where
    P: Fn(&T) -> bool,
{
    move |x| predicates.iter().any(|p| p(x))
}

// =============================================================================
// Comparison Functions
// =============================================================================

/// Compare by key.
pub fn cmp_by<T, K, F>(key_fn: F) -> impl Fn(&T, &T) -> std::cmp::Ordering
where
    K: Ord,
    F: Fn(&T) -> K,
{
    move |a, b| key_fn(a).cmp(&key_fn(b))
}

/// Compare by key in reverse.
pub fn cmp_by_rev<T, K, F>(key_fn: F) -> impl Fn(&T, &T) -> std::cmp::Ordering
where
    K: Ord,
    F: Fn(&T) -> K,
{
    move |a, b| key_fn(b).cmp(&key_fn(a))
}

/// Chain comparators.
pub fn cmp_chain<T, C>(comparators: Vec<C>) -> impl Fn(&T, &T) -> std::cmp::Ordering
where
    C: Fn(&T, &T) -> std::cmp::Ordering,
{
    move |a, b| {
        for cmp in &comparators {
            match cmp(a, b) {
                std::cmp::Ordering::Equal => continue,
                ord => return ord,
            }
        }
        std::cmp::Ordering::Equal
    }
}

// =============================================================================
// Operator Functions
// =============================================================================

/// Addition operator as function.
pub fn add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
    a + b
}

/// Subtraction operator as function.
pub fn sub<T: std::ops::Sub<Output = T>>(a: T, b: T) -> T {
    a - b
}

/// Multiplication operator as function.
pub fn mul<T: std::ops::Mul<Output = T>>(a: T, b: T) -> T {
    a * b
}

/// Division operator as function.
pub fn div<T: std::ops::Div<Output = T>>(a: T, b: T) -> T {
    a / b
}

/// Modulo operator as function.
pub fn rem<T: std::ops::Rem<Output = T>>(a: T, b: T) -> T {
    a % b
}

/// Negation operator as function.
pub fn neg<T: std::ops::Neg<Output = T>>(a: T) -> T {
    -a
}

/// Logical AND.
pub fn and(a: bool, b: bool) -> bool {
    a && b
}

/// Logical OR.
pub fn or(a: bool, b: bool) -> bool {
    a || b
}

/// Logical NOT.
pub fn not(a: bool) -> bool {
    !a
}

/// Equality check.
pub fn eq<T: PartialEq>(a: T, b: T) -> bool {
    a == b
}

/// Inequality check.
pub fn ne<T: PartialEq>(a: T, b: T) -> bool {
    a != b
}

/// Less than.
pub fn lt<T: PartialOrd>(a: T, b: T) -> bool {
    a < b
}

/// Less than or equal.
pub fn le<T: PartialOrd>(a: T, b: T) -> bool {
    a <= b
}

/// Greater than.
pub fn gt<T: PartialOrd>(a: T, b: T) -> bool {
    a > b
}

/// Greater than or equal.
pub fn ge<T: PartialOrd>(a: T, b: T) -> bool {
    a >= b
}

// =============================================================================
// Attribute/Item Access
// =============================================================================

/// Get item by index.
pub fn get_item<T: Clone>(items: &[T], index: usize) -> Option<T> {
    items.get(index).cloned()
}

/// Create an item getter.
pub fn itemgetter<T: Clone>(index: usize) -> impl Fn(&[T]) -> Option<T> {
    move |items| items.get(index).cloned()
}

/// Create a multi-item getter.
pub fn itemgetter_multi<T: Clone>(indices: Vec<usize>) -> impl Fn(&[T]) -> Vec<Option<T>> {
    move |items| {
        indices.iter()
            .map(|&i| items.get(i).cloned())
            .collect()
    }
}

// =============================================================================
// Total Ordering
// =============================================================================

/// Wrapper for total ordering from partial ordering.
pub struct TotalOrdering<T>(pub T);

impl<T: PartialOrd> PartialEq for TotalOrdering<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.partial_cmp(&other.0) == Some(std::cmp::Ordering::Equal)
    }
}

impl<T: PartialOrd> Eq for TotalOrdering<T> {}

impl<T: PartialOrd> PartialOrd for TotalOrdering<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl<T: PartialOrd> Ord for TotalOrdering<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_reduce() {
        let sum = reduce(vec![1, 2, 3, 4, 5], |a, b| a + b);
        assert_eq!(sum, Some(15));
        
        let empty: Vec<i32> = vec![];
        assert_eq!(reduce(empty, |a, b| a + b), None);
    }
    
    #[test]
    fn test_reduce_with() {
        let sum = reduce_with(vec![1, 2, 3, 4, 5], 10, |a, b| a + b);
        assert_eq!(sum, 25);
    }
    
    #[test]
    fn test_partial() {
        fn add(a: i32, b: i32) -> i32 { a + b }
        
        let add5 = partial1(add, 5);
        assert_eq!(add5(3), 8);
        assert_eq!(add5(10), 15);
    }
    
    #[test]
    fn test_memoize() {
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();
        
        let expensive = move |x: i32| {
            *counter_clone.lock().unwrap() += 1;
            x * 2
        };
        
        let memoized = memoize(expensive);
        
        assert_eq!(memoized.call(5), 10);
        assert_eq!(memoized.call(5), 10); // Cached
        assert_eq!(memoized.call(3), 6);
        
        assert_eq!(*counter.lock().unwrap(), 2); // Only 2 actual calls
        assert_eq!(memoized.cache_size(), 2);
    }
    
    #[test]
    fn test_compose() {
        let double = |x: i32| x * 2;
        let add1 = |x: i32| x + 1;
        
        let double_then_add1 = compose(add1, double);
        assert_eq!(double_then_add1(5), 11); // (5 * 2) + 1
        
        let add1_then_double = pipe(add1, double);
        assert_eq!(add1_then_double(5), 12); // (5 + 1) * 2
    }
    
    #[test]
    fn test_flip() {
        fn sub(a: i32, b: i32) -> i32 { a - b }
        
        let flipped = flip(sub);
        assert_eq!(sub(10, 3), 7);
        assert_eq!(flipped(10, 3), -7); // 3 - 10
    }
    
    #[test]
    fn test_apply_n() {
        let double = |x: i32| x * 2;
        assert_eq!(apply_n(3, double, 1), 8); // 1 -> 2 -> 4 -> 8
    }
    
    #[test]
    fn test_negate() {
        let is_even = |x: &i32| x % 2 == 0;
        let is_odd = negate(is_even);
        
        assert!(is_even(&4));
        assert!(is_odd(&3));
    }
    
    #[test]
    fn test_cmp_by() {
        let by_len = cmp_by(|s: &String| s.len());
        
        let a = "hello".to_string();
        let b = "hi".to_string();
        
        assert_eq!(by_len(&a, &b), std::cmp::Ordering::Greater);
    }
}

