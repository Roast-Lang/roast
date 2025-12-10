//! Array bisection algorithms.
//!
//! Python-compatible bisect module for binary search operations.

// =============================================================================
// Bisect Functions
// =============================================================================

/// Locate the leftmost position where x should be inserted to keep a sorted.
///
/// Returns i such that a[:i] < x <= a[i:].
pub fn bisect_left<T: Ord>(a: &[T], x: &T) -> usize {
    bisect_left_by(a, |item| item.cmp(x))
}

/// Same as bisect_left, with custom comparison.
pub fn bisect_left_by<T, F>(a: &[T], mut cmp: F) -> usize
where
    F: FnMut(&T) -> std::cmp::Ordering,
{
    let mut lo = 0;
    let mut hi = a.len();
    
    while lo < hi {
        let mid = (lo + hi) / 2;
        if cmp(&a[mid]) == std::cmp::Ordering::Less {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    
    lo
}

/// Locate the rightmost position where x should be inserted to keep a sorted.
///
/// Returns i such that a[:i] <= x < a[i:].
pub fn bisect_right<T: Ord>(a: &[T], x: &T) -> usize {
    bisect_right_by(a, |item| item.cmp(x))
}

/// Alias for bisect_right.
pub fn bisect<T: Ord>(a: &[T], x: &T) -> usize {
    bisect_right(a, x)
}

/// Same as bisect_right, with custom comparison.
pub fn bisect_right_by<T, F>(a: &[T], mut cmp: F) -> usize
where
    F: FnMut(&T) -> std::cmp::Ordering,
{
    let mut lo = 0;
    let mut hi = a.len();
    
    while lo < hi {
        let mid = (lo + hi) / 2;
        if cmp(&a[mid]) == std::cmp::Ordering::Greater {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    
    lo
}

/// Insert x in a, keeping it sorted.
///
/// If x is already in a, insert just before the leftmost occurrence.
pub fn insort_left<T: Ord + Clone>(a: &mut Vec<T>, x: T) {
    let i = bisect_left(a, &x);
    a.insert(i, x);
}

/// Insert x in a, keeping it sorted.
///
/// If x is already in a, insert just after the rightmost occurrence.
pub fn insort_right<T: Ord + Clone>(a: &mut Vec<T>, x: T) {
    let i = bisect_right(a, &x);
    a.insert(i, x);
}

/// Alias for insort_right.
pub fn insort<T: Ord + Clone>(a: &mut Vec<T>, x: T) {
    insort_right(a, x);
}

// =============================================================================
// Extended Operations
// =============================================================================

/// Find element using binary search. Returns index if found.
pub fn index<T: Ord>(a: &[T], x: &T) -> Option<usize> {
    let i = bisect_left(a, x);
    if i < a.len() && a[i] == *x {
        Some(i)
    } else {
        None
    }
}

/// Check if element exists in sorted array.
pub fn contains<T: Ord>(a: &[T], x: &T) -> bool {
    index(a, x).is_some()
}

/// Count occurrences of x in sorted array.
pub fn count<T: Ord>(a: &[T], x: &T) -> usize {
    let left = bisect_left(a, x);
    let right = bisect_right(a, x);
    right - left
}

/// Find the range of indices where x appears.
pub fn equal_range<T: Ord>(a: &[T], x: &T) -> std::ops::Range<usize> {
    let left = bisect_left(a, x);
    let right = bisect_right(a, x);
    left..right
}

/// Find the floor (largest element <= x).
pub fn floor<'a, T: Ord + Clone>(a: &'a [T], x: &T) -> Option<&'a T> {
    if a.is_empty() {
        return None;
    }
    
    let i = bisect_right(a, x);
    if i == 0 {
        None
    } else {
        Some(&a[i - 1])
    }
}

/// Find the ceiling (smallest element >= x).
pub fn ceiling<'a, T: Ord + Clone>(a: &'a [T], x: &T) -> Option<&'a T> {
    let i = bisect_left(a, x);
    if i >= a.len() {
        None
    } else {
        Some(&a[i])
    }
}

/// Find elements in range [lo, hi).
pub fn range<'a, T: Ord>(a: &'a [T], lo: &T, hi: &T) -> &'a [T] {
    let left = bisect_left(a, lo);
    let right = bisect_left(a, hi);
    &a[left..right]
}

// =============================================================================
// Sorted List
// =============================================================================

/// A list that maintains sorted order.
#[derive(Clone, Debug)]
pub struct SortedList<T> {
    items: Vec<T>,
}

impl<T: Ord + Clone> SortedList<T> {
    /// Create a new sorted list.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }
    
    /// Create from unsorted items.
    pub fn from_unsorted(mut items: Vec<T>) -> Self {
        items.sort();
        Self { items }
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.items.len()
    }
    
    /// Add an element, maintaining sort order.
    pub fn add(&mut self, x: T) {
        insort(&mut self.items, x);
    }
    
    /// Remove the first occurrence of x.
    pub fn remove(&mut self, x: &T) -> bool {
        if let Some(i) = index(&self.items, x) {
            self.items.remove(i);
            true
        } else {
            false
        }
    }
    
    /// Check if x is in the list.
    pub fn contains(&self, x: &T) -> bool {
        contains(&self.items, x)
    }
    
    /// Count occurrences of x.
    pub fn count(&self, x: &T) -> usize {
        count(&self.items, x)
    }
    
    /// Get element at index.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }
    
    /// Get the minimum element.
    pub fn min(&self) -> Option<&T> {
        self.items.first()
    }
    
    /// Get the maximum element.
    pub fn max(&self) -> Option<&T> {
        self.items.last()
    }
    
    /// Pop the minimum element.
    pub fn pop_min(&mut self) -> Option<T> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.remove(0))
        }
    }
    
    /// Pop the maximum element.
    pub fn pop_max(&mut self) -> Option<T> {
        self.items.pop()
    }
    
    /// Get elements in range [lo, hi).
    pub fn range(&self, lo: &T, hi: &T) -> &[T] {
        range(&self.items, lo, hi)
    }
    
    /// Iterate over elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }
    
    /// Get slice of all items.
    pub fn as_slice(&self) -> &[T] {
        &self.items
    }
}

impl<T: Ord + Clone> Default for SortedList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord + Clone> FromIterator<T> for SortedList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut items: Vec<T> = iter.into_iter().collect();
        items.sort();
        Self { items }
    }
}

impl<T: Ord + Clone> IntoIterator for SortedList<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T: Ord + Clone> IntoIterator for &'a SortedList<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

// =============================================================================
// Sorted Dict
// =============================================================================

/// A dictionary with sorted keys.
#[derive(Clone, Debug)]
pub struct SortedDict<K, V> {
    keys: SortedList<K>,
    values: std::collections::HashMap<K, V>,
}

impl<K: Ord + Clone + std::hash::Hash + Eq, V: Clone> SortedDict<K, V> {
    /// Create a new sorted dict.
    pub fn new() -> Self {
        Self {
            keys: SortedList::new(),
            values: std::collections::HashMap::new(),
        }
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.keys.len()
    }
    
    /// Insert a key-value pair.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if !self.keys.contains(&key) {
            self.keys.add(key.clone());
        }
        self.values.insert(key, value)
    }
    
    /// Get a value by key.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.values.get(key)
    }
    
    /// Remove a key.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.keys.remove(key);
        self.values.remove(key)
    }
    
    /// Check if key exists.
    pub fn contains_key(&self, key: &K) -> bool {
        self.keys.contains(key)
    }
    
    /// Iterate over keys in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.keys.iter()
    }
    
    /// Iterate over values in key order.
    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.keys.iter().filter_map(|k| self.values.get(k))
    }
    
    /// Iterate over key-value pairs in sorted key order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> + '_ {
        self.keys.iter().filter_map(|k| self.values.get(k).map(|v| (k, v)))
    }
    
    /// Get the minimum key.
    pub fn min_key(&self) -> Option<&K> {
        self.keys.min()
    }
    
    /// Get the maximum key.
    pub fn max_key(&self) -> Option<&K> {
        self.keys.max()
    }
}

impl<K: Ord + Clone + std::hash::Hash + Eq, V: Clone> Default for SortedDict<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bisect_left() {
        let a = vec![1, 2, 2, 3, 4, 5];
        assert_eq!(bisect_left(&a, &2), 1);
        assert_eq!(bisect_left(&a, &0), 0);
        assert_eq!(bisect_left(&a, &6), 6);
    }
    
    #[test]
    fn test_bisect_right() {
        let a = vec![1, 2, 2, 3, 4, 5];
        assert_eq!(bisect_right(&a, &2), 3);
        assert_eq!(bisect_right(&a, &0), 0);
        assert_eq!(bisect_right(&a, &6), 6);
    }
    
    #[test]
    fn test_insort() {
        let mut a = vec![1, 3, 5];
        insort(&mut a, 2);
        assert_eq!(a, vec![1, 2, 3, 5]);
        
        insort(&mut a, 4);
        assert_eq!(a, vec![1, 2, 3, 4, 5]);
    }
    
    #[test]
    fn test_index() {
        let a = vec![1, 2, 3, 4, 5];
        assert_eq!(index(&a, &3), Some(2));
        assert_eq!(index(&a, &6), None);
    }
    
    #[test]
    fn test_count() {
        let a = vec![1, 2, 2, 2, 3, 4];
        assert_eq!(count(&a, &2), 3);
        assert_eq!(count(&a, &5), 0);
    }
    
    #[test]
    fn test_floor_ceiling() {
        let a = vec![1, 3, 5, 7, 9];
        
        assert_eq!(floor(&a, &4), Some(&3));
        assert_eq!(floor(&a, &5), Some(&5));
        assert_eq!(floor(&a, &0), None);
        
        assert_eq!(ceiling(&a, &4), Some(&5));
        assert_eq!(ceiling(&a, &5), Some(&5));
        assert_eq!(ceiling(&a, &10), None);
    }
    
    #[test]
    fn test_range() {
        let a = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        assert_eq!(range(&a, &3, &7), &[3, 4, 5, 6]);
    }
    
    #[test]
    fn test_sorted_list() {
        let mut list = SortedList::new();
        list.add(3);
        list.add(1);
        list.add(4);
        list.add(1);
        list.add(5);
        
        assert_eq!(list.as_slice(), &[1, 1, 3, 4, 5]);
        assert_eq!(list.count(&1), 2);
        assert_eq!(list.min(), Some(&1));
        assert_eq!(list.max(), Some(&5));
        
        assert!(list.remove(&4));
        assert_eq!(list.as_slice(), &[1, 1, 3, 5]);
    }
    
    #[test]
    fn test_sorted_dict() {
        let mut dict = SortedDict::new();
        dict.insert("c", 3);
        dict.insert("a", 1);
        dict.insert("b", 2);
        
        let keys: Vec<_> = dict.keys().cloned().collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
        
        assert_eq!(dict.get(&"b"), Some(&2));
    }
}

