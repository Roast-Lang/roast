//! Result type and utilities.

use crate::error::RoastError;

/// Roast result type.
pub type RoastResult<T> = Result<T, RoastError>;

/// Re-export standard Result for convenience.
pub use std::result::Result;

/// Extension methods for Result.
pub trait ResultExt<T, E> {
    /// Map the error to a RoastError.
    fn map_roast_err(self) -> RoastResult<T>
    where
        E: std::error::Error + Send + Sync + 'static;
    
    /// Unwrap or return a default value.
    fn unwrap_or_default_with<F: FnOnce() -> T>(self, f: F) -> T;
    
    /// Log error and return Ok(default) or the original Ok value.
    fn ok_or_log(self, default: T) -> T
    where
        E: std::fmt::Display;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn map_roast_err(self) -> RoastResult<T>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.map_err(|e| RoastError::with_cause(
            crate::error::ErrorKind::RuntimeError,
            e.to_string(),
            e,
        ))
    }
    
    fn unwrap_or_default_with<F: FnOnce() -> T>(self, f: F) -> T {
        match self {
            Ok(v) => v,
            Err(_) => f(),
        }
    }
    
    fn ok_or_log(self, default: T) -> T
    where
        E: std::fmt::Display,
    {
        match self {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error: {}", e);
                default
            }
        }
    }
}

/// Extension methods for Option.
pub trait OptionExt<T> {
    /// Convert to RoastResult with an error message.
    fn ok_or_err(self, message: impl Into<String>) -> RoastResult<T>;
    
    /// Unwrap or compute a default.
    fn unwrap_or_compute<F: FnOnce() -> T>(self, f: F) -> T;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_or_err(self, message: impl Into<String>) -> RoastResult<T> {
        self.ok_or_else(|| RoastError::value_error(message))
    }
    
    fn unwrap_or_compute<F: FnOnce() -> T>(self, f: F) -> T {
        self.unwrap_or_else(f)
    }
}

/// Try to execute a closure and return a Result.
pub fn try_exec<T, F: FnOnce() -> T>(f: F) -> RoastResult<T> {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    
    catch_unwind(AssertUnwindSafe(f)).map_err(|e| {
        let message = if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        RoastError::runtime_error(message)
    })
}

/// Collect results, stopping at the first error.
pub fn collect_results<T, E, I>(iter: I) -> Result<Vec<T>, E>
where
    I: IntoIterator<Item = Result<T, E>>,
{
    iter.into_iter().collect()
}

/// Collect results, gathering all errors.
pub fn collect_all_results<T, E, I>(iter: I) -> Result<Vec<T>, Vec<E>>
where
    I: IntoIterator<Item = Result<T, E>>,
{
    let mut values = Vec::new();
    let mut errors = Vec::new();
    
    for result in iter {
        match result {
            Ok(v) => values.push(v),
            Err(e) => errors.push(e),
        }
    }
    
    if errors.is_empty() {
        Ok(values)
    } else {
        Err(errors)
    }
}

/// Partition results into successes and failures.
pub fn partition_results<T, E, I>(iter: I) -> (Vec<T>, Vec<E>)
where
    I: IntoIterator<Item = Result<T, E>>,
{
    let mut oks = Vec::new();
    let mut errs = Vec::new();
    
    for result in iter {
        match result {
            Ok(v) => oks.push(v),
            Err(e) => errs.push(e),
        }
    }
    
    (oks, errs)
}

// =============================================================================
// Additional Result Combinators
// =============================================================================

/// Flatten a nested Result.
pub fn flatten<T, E>(result: Result<Result<T, E>, E>) -> Result<T, E> {
    match result {
        Ok(inner) => inner,
        Err(e) => Err(e),
    }
}

/// Transpose Result<Option<T>, E> to Option<Result<T, E>>.
pub fn transpose<T, E>(result: Result<Option<T>, E>) -> Option<Result<T, E>> {
    match result {
        Ok(Some(v)) => Some(Ok(v)),
        Ok(None) => None,
        Err(e) => Some(Err(e)),
    }
}

/// Apply a function that may fail to each element.
pub fn try_map<T, U, E, F>(items: Vec<T>, f: F) -> Result<Vec<U>, E>
where
    F: Fn(T) -> Result<U, E>,
{
    items.into_iter().map(f).collect()
}

/// Filter with a fallible predicate.
pub fn try_filter<T, E, F>(items: Vec<T>, f: F) -> Result<Vec<T>, E>
where
    F: Fn(&T) -> Result<bool, E>,
{
    let mut result = Vec::new();
    for item in items {
        if f(&item)? {
            result.push(item);
        }
    }
    Ok(result)
}

/// Find with a fallible predicate.
pub fn try_find<T, E, F>(items: Vec<T>, f: F) -> Result<Option<T>, E>
where
    F: Fn(&T) -> Result<bool, E>,
{
    for item in items {
        if f(&item)? {
            return Ok(Some(item));
        }
    }
    Ok(None)
}

/// Fold with a fallible function.
pub fn try_fold<T, A, E, F>(items: Vec<T>, init: A, f: F) -> Result<A, E>
where
    F: Fn(A, T) -> Result<A, E>,
{
    items.into_iter().try_fold(init, f)
}

/// Execute multiple fallible operations and collect results.
pub fn try_all<T, E, F>(operations: Vec<F>) -> Result<Vec<T>, E>
where
    F: FnOnce() -> Result<T, E>,
{
    operations.into_iter().map(|op| op()).collect()
}

/// Execute operations until one succeeds.
pub fn try_first<T, E, F>(operations: Vec<F>) -> Result<T, Vec<E>>
where
    F: FnOnce() -> Result<T, E>,
{
    let mut errors = Vec::new();
    
    for op in operations {
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => errors.push(e),
        }
    }
    
    Err(errors)
}

// =============================================================================
// Retry Logic
// =============================================================================

/// Retry a fallible operation.
pub fn retry<T, E, F>(max_attempts: usize, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut last_error = None;
    
    for _ in 0..max_attempts {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => last_error = Some(e),
        }
    }
    
    Err(last_error.unwrap())
}

/// Retry with a delay between attempts.
pub fn retry_with_delay<T, E, F>(
    max_attempts: usize,
    delay: std::time::Duration,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut last_error = None;
    
    for i in 0..max_attempts {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_error = Some(e);
                if i < max_attempts - 1 {
                    std::thread::sleep(delay);
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}

/// Retry with exponential backoff.
pub fn retry_exponential<T, E, F>(
    max_attempts: usize,
    initial_delay: std::time::Duration,
    max_delay: std::time::Duration,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut last_error = None;
    let mut delay = initial_delay;
    
    for i in 0..max_attempts {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_error = Some(e);
                if i < max_attempts - 1 {
                    std::thread::sleep(delay);
                    delay = std::cmp::min(delay * 2, max_delay);
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_option_ext() {
        let some: Option<i32> = Some(42);
        let none: Option<i32> = None;
        
        assert!(some.ok_or_err("error").is_ok());
        assert!(none.ok_or_err("error").is_err());
    }
    
    #[test]
    fn test_partition() {
        let results: Vec<Result<i32, &str>> = vec![Ok(1), Err("a"), Ok(2), Err("b")];
        let (oks, errs) = partition_results(results);
        
        assert_eq!(oks, vec![1, 2]);
        assert_eq!(errs, vec!["a", "b"]);
    }
    
    #[test]
    fn test_try_map() {
        let items = vec![1, 2, 3];
        let result: Result<Vec<_>, &str> = try_map(items, |x| Ok(x * 2));
        assert_eq!(result.unwrap(), vec![2, 4, 6]);
    }
    
    #[test]
    fn test_retry() {
        let mut counter = 0;
        let result = retry(3, || {
            counter += 1;
            if counter < 3 {
                Err("not yet")
            } else {
                Ok(42)
            }
        });
        
        assert_eq!(result, Ok(42));
        assert_eq!(counter, 3);
    }
    
    #[test]
    fn test_try_first() {
        let ops: Vec<Box<dyn FnOnce() -> Result<i32, String>>> = vec![
            Box::new(|| Err("first failed".to_string())),
            Box::new(|| Err("second failed".to_string())),
            Box::new(|| Ok(42)),
        ];
        
        let result = try_first(ops);
        assert_eq!(result, Ok(42));
    }
}

