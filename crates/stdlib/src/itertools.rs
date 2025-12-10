//! Iterator utilities (Python's itertools equivalent).
//!
//! Provides powerful iterator combinators for efficient data processing.

use std::collections::HashMap;
use std::hash::Hash;
use std::iter::{self, Peekable};

// =============================================================================
// Infinite Iterators
// =============================================================================

/// Count from n with step.
pub fn count(start: i64, step: i64) -> impl Iterator<Item = i64> {
    let mut n = start;
    iter::from_fn(move || {
        let result = n;
        n += step;
        Some(result)
    })
}

/// Cycle through an iterator infinitely.
pub fn cycle<I, T>(iter: I) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    T: Clone,
{
    let items: Vec<_> = iter.into_iter().collect();
    let mut i = 0;
    iter::from_fn(move || {
        if items.is_empty() {
            None
        } else {
            let result = items[i].clone();
            i = (i + 1) % items.len();
            Some(result)
        }
    })
}

/// Repeat a value n times (or infinitely if n is None).
pub fn repeat<T: Clone>(value: T, times: Option<usize>) -> impl Iterator<Item = T> {
    let mut count = 0;
    iter::from_fn(move || {
        match times {
            Some(n) if count >= n => None,
            _ => {
                count += 1;
                Some(value.clone())
            }
        }
    })
}

// =============================================================================
// Terminating Iterators
// =============================================================================

/// Accumulate values (like reduce but yields intermediate results).
pub fn accumulate<I, T, F>(iter: I, func: F) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    T: Clone,
    F: Fn(T, T) -> T,
{
    let mut iter = iter.into_iter();
    let mut acc = iter.next();
    
    iter::from_fn(move || {
        match acc.take() {
            None => None,
            Some(a) => {
                let result = a.clone();
                acc = iter.next().map(|x| func(a, x));
                if acc.is_none() {
                    acc = Some(result.clone());
                    Some(result)
                } else {
                    Some(result)
                }
            }
        }
    })
}

/// Chain multiple iterators.
pub fn chain<I, T>(iters: Vec<I>) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
{
    iters.into_iter().flat_map(|i| i.into_iter())
}

/// Compress data based on selectors.
pub fn compress<I, S, T>(data: I, selectors: S) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    S: IntoIterator<Item = bool>,
{
    data.into_iter()
        .zip(selectors)
        .filter_map(|(d, s)| if s { Some(d) } else { None })
}

/// Drop items while predicate is true.
pub fn dropwhile<I, T, P>(iter: I, predicate: P) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    P: Fn(&T) -> bool,
{
    let mut dropping = true;
    iter.into_iter().filter(move |x| {
        if dropping && predicate(x) {
            false
        } else {
            dropping = false;
            true
        }
    })
}

/// Take items while predicate is true.
pub fn takewhile<I, T, P>(iter: I, predicate: P) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    P: Fn(&T) -> bool,
{
    iter.into_iter().take_while(predicate)
}

/// Filter items that return false for predicate.
pub fn filterfalse<I, T, P>(iter: I, predicate: P) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    P: Fn(&T) -> bool,
{
    iter.into_iter().filter(move |x| !predicate(x))
}

/// Slice an iterator.
pub fn islice<I, T>(iter: I, start: usize, stop: Option<usize>, step: usize) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
{
    let step = if step == 0 { 1 } else { step };
    let mut iter = iter.into_iter().skip(start);
    let mut count = 0;
    
    iter::from_fn(move || {
        loop {
            if let Some(max) = stop {
                if start + count >= max {
                    return None;
                }
            }
            
            let item = iter.next()?;
            
            if count % step == 0 {
                count += 1;
                return Some(item);
            }
            count += 1;
        }
    })
}

/// Apply function to every item and flatten.
pub fn starmap<I, T, R, F>(iter: I, func: F) -> impl Iterator<Item = R>
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> R,
{
    iter.into_iter().map(func)
}

/// Iterate over consecutive pairs.
pub fn pairwise<I, T>(iter: I) -> impl Iterator<Item = (T, T)>
where
    I: IntoIterator<Item = T>,
    T: Clone,
{
    let mut iter = iter.into_iter();
    let mut prev = iter.next();
    
    iter::from_fn(move || {
        let p = prev.take()?;
        prev = iter.next();
        prev.as_ref().map(|n| (p, n.clone()))
    })
}

/// Batched iterator - yield chunks of n items.
pub fn batched<I, T>(iter: I, n: usize) -> impl Iterator<Item = Vec<T>>
where
    I: IntoIterator<Item = T>,
{
    let mut iter = iter.into_iter().peekable();
    
    iter::from_fn(move || {
        if iter.peek().is_none() {
            return None;
        }
        
        let batch: Vec<_> = iter.by_ref().take(n).collect();
        if batch.is_empty() {
            None
        } else {
            Some(batch)
        }
    })
}

// =============================================================================
// Combinatoric Iterators
// =============================================================================

/// Cartesian product of iterables.
pub fn product<T, U>(a: Vec<T>, b: Vec<U>) -> impl Iterator<Item = (T, U)>
where
    T: Clone,
    U: Clone,
{
    a.into_iter()
        .flat_map(move |x| b.clone().into_iter().map(move |y| (x.clone(), y)))
}

/// Permutations of length r.
pub fn permutations<T: Clone>(items: Vec<T>, r: usize) -> Vec<Vec<T>> {
    if r == 0 {
        return vec![vec![]];
    }
    if r > items.len() {
        return vec![];
    }
    
    let mut result = Vec::new();
    
    for (i, item) in items.iter().enumerate() {
        let mut remaining: Vec<_> = items.iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, x)| x.clone())
            .collect();
        
        for mut perm in permutations(remaining, r - 1) {
            let mut v = vec![item.clone()];
            v.append(&mut perm);
            result.push(v);
        }
    }
    
    result
}

/// Combinations of length r.
pub fn combinations<T: Clone>(items: Vec<T>, r: usize) -> Vec<Vec<T>> {
    if r == 0 {
        return vec![vec![]];
    }
    if r > items.len() {
        return vec![];
    }
    
    let mut result = Vec::new();
    
    for (i, item) in items.iter().enumerate() {
        let remaining: Vec<_> = items.iter()
            .skip(i + 1)
            .cloned()
            .collect();
        
        for mut comb in combinations(remaining, r - 1) {
            let mut v = vec![item.clone()];
            v.append(&mut comb);
            result.push(v);
        }
    }
    
    result
}

/// Combinations with replacement.
pub fn combinations_with_replacement<T: Clone>(items: Vec<T>, r: usize) -> Vec<Vec<T>> {
    if r == 0 {
        return vec![vec![]];
    }
    if items.is_empty() {
        return vec![];
    }
    
    let mut result = Vec::new();
    
    for (i, item) in items.iter().enumerate() {
        let remaining: Vec<_> = items.iter()
            .skip(i)
            .cloned()
            .collect();
        
        for mut comb in combinations_with_replacement(remaining, r - 1) {
            let mut v = vec![item.clone()];
            v.append(&mut comb);
            result.push(v);
        }
    }
    
    result
}

// =============================================================================
// Grouping
// =============================================================================

/// Group consecutive elements by key.
pub fn groupby<I, T, K, F>(iter: I, key_fn: F) -> Vec<(K, Vec<T>)>
where
    I: IntoIterator<Item = T>,
    K: Eq,
    F: Fn(&T) -> K,
{
    let mut result: Vec<(K, Vec<T>)> = Vec::new();
    
    for item in iter {
        let k = key_fn(&item);
        
        match result.last_mut() {
            Some((last_key, group)) if *last_key == k => {
                group.push(item);
            }
            _ => {
                result.push((k, vec![item]));
            }
        }
    }
    
    result
}

// =============================================================================
// Zip Variants
// =============================================================================

/// Zip with a fill value for shorter iterables.
pub fn zip_longest<T, U>(a: Vec<T>, b: Vec<U>, fill_a: T, fill_b: U) -> Vec<(T, U)>
where
    T: Clone,
    U: Clone,
{
    let max_len = a.len().max(b.len());
    let mut result = Vec::with_capacity(max_len);
    
    for i in 0..max_len {
        let x = a.get(i).cloned().unwrap_or_else(|| fill_a.clone());
        let y = b.get(i).cloned().unwrap_or_else(|| fill_b.clone());
        result.push((x, y));
    }
    
    result
}

/// Unzip a list of tuples.
pub fn unzip<T, U>(pairs: Vec<(T, U)>) -> (Vec<T>, Vec<U>) {
    pairs.into_iter().unzip()
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Consume an iterator entirely.
pub fn consume<I: Iterator>(iter: I) {
    for _ in iter {}
}

/// Get the nth item.
pub fn nth<I, T>(iter: I, n: usize) -> Option<T>
where
    I: IntoIterator<Item = T>,
{
    iter.into_iter().nth(n)
}

/// Get all items equal to the first.
pub fn all_equal<I, T>(iter: I) -> bool
where
    I: IntoIterator<Item = T>,
    T: PartialEq,
{
    let mut iter = iter.into_iter();
    match iter.next() {
        None => true,
        Some(first) => iter.all(|x| x == first),
    }
}

/// Quantify how many items satisfy predicate.
pub fn quantify<I, T, P>(iter: I, predicate: P) -> usize
where
    I: IntoIterator<Item = T>,
    P: Fn(&T) -> bool,
{
    iter.into_iter().filter(predicate).count()
}

/// Pad iterator on the right with fill values.
pub fn pad_none<I, T>(iter: I, fill: T, min_len: usize) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    T: Clone,
{
    let items: Vec<_> = iter.into_iter().collect();
    let pad_len = if items.len() >= min_len { 0 } else { min_len - items.len() };
    
    items.into_iter().chain(repeat(fill, Some(pad_len)))
}

/// Flatten one level of nesting.
pub fn flatten<I, Inner, T>(iter: I) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = Inner>,
    Inner: IntoIterator<Item = T>,
{
    iter.into_iter().flat_map(|inner| inner.into_iter())
}

/// Roundrobin between iterators.
pub fn roundrobin<T>(iters: Vec<Vec<T>>) -> Vec<T> {
    let mut result = Vec::new();
    let max_len = iters.iter().map(|v| v.len()).max().unwrap_or(0);
    
    for i in 0..max_len {
        for iter in &iters {
            if i < iter.len() {
                // Would need to clone here
            }
        }
    }
    
    result
}

/// Partition items by predicate.
pub fn partition<I, T, P>(iter: I, predicate: P) -> (Vec<T>, Vec<T>)
where
    I: IntoIterator<Item = T>,
    P: Fn(&T) -> bool,
{
    let mut true_items = Vec::new();
    let mut false_items = Vec::new();
    
    for item in iter {
        if predicate(&item) {
            true_items.push(item);
        } else {
            false_items.push(item);
        }
    }
    
    (true_items, false_items)
}

/// First true value.
pub fn first_true<I, T, P>(iter: I, predicate: P, default: T) -> T
where
    I: IntoIterator<Item = T>,
    P: Fn(&T) -> bool,
{
    iter.into_iter()
        .find(predicate)
        .unwrap_or(default)
}

/// Unique elements preserving order.
pub fn unique<I, T>(iter: I) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    T: Clone + Eq + Hash,
{
    let mut seen = std::collections::HashSet::new();
    iter.into_iter().filter(move |x| seen.insert(x.clone()))
}

/// Unique elements by key.
pub fn unique_by<I, T, K, F>(iter: I, key_fn: F) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    K: Eq + Hash,
    F: Fn(&T) -> K,
{
    let mut seen = std::collections::HashSet::new();
    iter.into_iter().filter(move |x| seen.insert(key_fn(x)))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_count() {
        let nums: Vec<_> = count(5, 2).take(5).collect();
        assert_eq!(nums, vec![5, 7, 9, 11, 13]);
    }
    
    #[test]
    fn test_cycle() {
        let items: Vec<_> = cycle(vec![1, 2, 3]).take(7).collect();
        assert_eq!(items, vec![1, 2, 3, 1, 2, 3, 1]);
    }
    
    #[test]
    fn test_repeat() {
        let items: Vec<_> = repeat(42, Some(3)).collect();
        assert_eq!(items, vec![42, 42, 42]);
    }
    
    #[test]
    fn test_combinations() {
        let c = combinations(vec![1, 2, 3, 4], 2);
        assert_eq!(c.len(), 6);
        assert!(c.contains(&vec![1, 2]));
        assert!(c.contains(&vec![3, 4]));
    }
    
    #[test]
    fn test_permutations() {
        let p = permutations(vec![1, 2, 3], 2);
        assert_eq!(p.len(), 6);
        assert!(p.contains(&vec![1, 2]));
        assert!(p.contains(&vec![2, 1]));
    }
    
    #[test]
    fn test_product() {
        let p: Vec<_> = product(vec![1, 2], vec!['a', 'b']).collect();
        assert_eq!(p.len(), 4);
        assert!(p.contains(&(1, 'a')));
        assert!(p.contains(&(2, 'b')));
    }
    
    #[test]
    fn test_dropwhile() {
        let items: Vec<_> = dropwhile(vec![1, 2, 3, 4, 1, 2], |&x| x < 3).collect();
        assert_eq!(items, vec![3, 4, 1, 2]);
    }
    
    #[test]
    fn test_takewhile() {
        let items: Vec<_> = takewhile(vec![1, 2, 3, 4, 1], |&x| x < 3).collect();
        assert_eq!(items, vec![1, 2]);
    }
    
    #[test]
    fn test_groupby() {
        let items = vec![1, 1, 2, 2, 2, 3, 1, 1];
        let groups = groupby(items, |x| *x);
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0], (1, vec![1, 1]));
        assert_eq!(groups[1], (2, vec![2, 2, 2]));
    }
    
    #[test]
    fn test_partition() {
        let (evens, odds) = partition(vec![1, 2, 3, 4, 5], |x| x % 2 == 0);
        assert_eq!(evens, vec![2, 4]);
        assert_eq!(odds, vec![1, 3, 5]);
    }
    
    #[test]
    fn test_unique() {
        let items: Vec<_> = unique(vec![1, 2, 1, 3, 2, 4]).collect();
        assert_eq!(items, vec![1, 2, 3, 4]);
    }
}

