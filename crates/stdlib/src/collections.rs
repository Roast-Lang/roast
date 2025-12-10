//! Collection types and utilities.

use roast_runtime::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// List operations.
pub mod list {
    use super::*;

    pub fn append(list: &mut Vec<Value>, item: Value) {
        list.push(item);
    }

    pub fn extend(list: &mut Vec<Value>, items: &[Value]) {
        list.extend(items.iter().cloned());
    }

    pub fn insert(list: &mut Vec<Value>, index: usize, item: Value) {
        if index <= list.len() {
            list.insert(index, item);
        }
    }

    pub fn remove(list: &mut Vec<Value>, index: usize) -> Option<Value> {
        if index < list.len() {
            Some(list.remove(index))
        } else {
            None
        }
    }

    pub fn pop(list: &mut Vec<Value>) -> Option<Value> {
        list.pop()
    }

    pub fn clear(list: &mut Vec<Value>) {
        list.clear();
    }

    pub fn reverse(list: &mut Vec<Value>) {
        list.reverse();
    }

    pub fn sort(list: &mut Vec<Value>) {
        list.sort_by(|a, b| {
            match (a, b) {
                (Value::Int(x), Value::Int(y)) => x.cmp(y),
                (Value::Float(x), Value::Float(y)) => {
                    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                }
                (Value::Str(x), Value::Str(y)) => x.cmp(y),
                _ => std::cmp::Ordering::Equal,
            }
        });
    }

    pub fn index(list: &[Value], item: &Value) -> Option<usize> {
        list.iter().position(|v| v == item)
    }

    pub fn count(list: &[Value], item: &Value) -> usize {
        list.iter().filter(|v| *v == item).count()
    }
}

/// Dict operations.
pub mod dict {
    use super::*;
    use roast_runtime::value::ValueKey;

    pub fn keys(dict: &HashMap<ValueKey, Value>) -> Vec<Value> {
        dict.keys()
            .map(|k| match k {
                ValueKey::None => Value::None,
                ValueKey::Bool(b) => Value::Bool(*b),
                ValueKey::Int(n) => Value::Int(*n),
                ValueKey::Str(s) => Value::Str(s.clone()),
                ValueKey::Bytes(b) => Value::Bytes(b.clone()),
                ValueKey::Tuple(t) => {
                    // Convert tuple keys back to values
                    Value::None // Simplified
                }
            })
            .collect()
    }

    pub fn values(dict: &HashMap<ValueKey, Value>) -> Vec<Value> {
        dict.values().cloned().collect()
    }

    pub fn items(dict: &HashMap<ValueKey, Value>) -> Vec<Value> {
        dict.iter()
            .map(|(k, v)| {
                let key = match k {
                    ValueKey::None => Value::None,
                    ValueKey::Bool(b) => Value::Bool(*b),
                    ValueKey::Int(n) => Value::Int(*n),
                    ValueKey::Str(s) => Value::Str(s.clone()),
                    ValueKey::Bytes(b) => Value::Bytes(b.clone()),
                    ValueKey::Tuple(_) => Value::None,
                };
                Value::Tuple(Arc::new([key, v.clone()]))
            })
            .collect()
    }

    pub fn get(dict: &HashMap<ValueKey, Value>, key: &ValueKey, default: Option<&Value>) -> Value {
        dict.get(key)
            .cloned()
            .unwrap_or_else(|| default.cloned().unwrap_or(Value::None))
    }

    pub fn pop(dict: &mut HashMap<ValueKey, Value>, key: &ValueKey) -> Option<Value> {
        dict.remove(key)
    }

    pub fn clear(dict: &mut HashMap<ValueKey, Value>) {
        dict.clear();
    }

    pub fn update(dict: &mut HashMap<ValueKey, Value>, other: &HashMap<ValueKey, Value>) {
        for (k, v) in other {
            dict.insert(k.clone(), v.clone());
        }
    }
}

/// Set operations.
pub mod set {
    use roast_runtime::value::ValueKey;
    use std::collections::HashSet;

    pub fn add(set: &mut HashSet<ValueKey>, item: ValueKey) {
        set.insert(item);
    }

    pub fn remove(set: &mut HashSet<ValueKey>, item: &ValueKey) -> bool {
        set.remove(item)
    }

    pub fn discard(set: &mut HashSet<ValueKey>, item: &ValueKey) {
        set.remove(item);
    }

    pub fn pop(set: &mut HashSet<ValueKey>) -> Option<ValueKey> {
        let item = set.iter().next().cloned();
        if let Some(ref i) = item {
            set.remove(i);
        }
        item
    }

    pub fn clear(set: &mut HashSet<ValueKey>) {
        set.clear();
    }

    pub fn union(a: &HashSet<ValueKey>, b: &HashSet<ValueKey>) -> HashSet<ValueKey> {
        a.union(b).cloned().collect()
    }

    pub fn intersection(a: &HashSet<ValueKey>, b: &HashSet<ValueKey>) -> HashSet<ValueKey> {
        a.intersection(b).cloned().collect()
    }

    pub fn difference(a: &HashSet<ValueKey>, b: &HashSet<ValueKey>) -> HashSet<ValueKey> {
        a.difference(b).cloned().collect()
    }

    pub fn symmetric_difference(a: &HashSet<ValueKey>, b: &HashSet<ValueKey>) -> HashSet<ValueKey> {
        a.symmetric_difference(b).cloned().collect()
    }

    pub fn is_subset(a: &HashSet<ValueKey>, b: &HashSet<ValueKey>) -> bool {
        a.is_subset(b)
    }

    pub fn is_superset(a: &HashSet<ValueKey>, b: &HashSet<ValueKey>) -> bool {
        a.is_superset(b)
    }
}

/// Deque (double-ended queue).
pub struct Deque {
    inner: VecDeque<Value>,
}

impl Deque {
    pub fn new() -> Self {
        Self {
            inner: VecDeque::new(),
        }
    }

    pub fn append(&mut self, item: Value) {
        self.inner.push_back(item);
    }

    pub fn appendleft(&mut self, item: Value) {
        self.inner.push_front(item);
    }

    pub fn pop(&mut self) -> Option<Value> {
        self.inner.pop_back()
    }

    pub fn popleft(&mut self) -> Option<Value> {
        self.inner.pop_front()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for Deque {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// OrderedDict - Dict that remembers insertion order
// =============================================================================

/// A dictionary that remembers the order keys were added.
#[derive(Clone, Debug)]
pub struct OrderedDict<K, V> {
    map: HashMap<K, V>,
    order: Vec<K>,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> OrderedDict<K, V> {
    /// Create a new empty OrderedDict.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
        }
    }
    
    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            order: Vec::with_capacity(capacity),
        }
    }
    
    /// Insert a key-value pair.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let old = self.map.insert(key.clone(), value);
        if old.is_none() {
            self.order.push(key);
        }
        old
    }
    
    /// Get a value by key.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }
    
    /// Get a mutable reference to a value.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
    }
    
    /// Remove a key-value pair.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.map.remove(key) {
            self.order.retain(|k| k != key);
            Some(value)
        } else {
            None
        }
    }
    
    /// Check if key exists.
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }
    
    /// Get the number of items.
    pub fn len(&self) -> usize {
        self.map.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    
    /// Clear all items.
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
    
    /// Get keys in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.order.iter()
    }
    
    /// Get values in insertion order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.order.iter().filter_map(|k| self.map.get(k))
    }
    
    /// Get key-value pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.order.iter().filter_map(|k| self.map.get(k).map(|v| (k, v)))
    }
    
    /// Get first item.
    pub fn first(&self) -> Option<(&K, &V)> {
        self.order.first().and_then(|k| self.map.get(k).map(|v| (k, v)))
    }
    
    /// Get last item.
    pub fn last(&self) -> Option<(&K, &V)> {
        self.order.last().and_then(|k| self.map.get(k).map(|v| (k, v)))
    }
    
    /// Pop the first item.
    pub fn pop_first(&mut self) -> Option<(K, V)> {
        if self.order.is_empty() {
            return None;
        }
        let key = self.order.remove(0);
        self.map.remove(&key).map(|v| (key, v))
    }
    
    /// Pop the last item.
    pub fn pop_last(&mut self) -> Option<(K, V)> {
        self.order.pop().and_then(|key| {
            self.map.remove(&key).map(|v| (key, v))
        })
    }
    
    /// Move a key to the end.
    pub fn move_to_end(&mut self, key: &K) {
        if self.map.contains_key(key) {
            self.order.retain(|k| k != key);
            self.order.push(key.clone());
        }
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Default for OrderedDict<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> FromIterator<(K, V)> for OrderedDict<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut dict = Self::new();
        for (k, v) in iter {
            dict.insert(k, v);
        }
        dict
    }
}

// =============================================================================
// DefaultDict - Dict with default factory
// =============================================================================

/// A dictionary that returns a default value for missing keys.
#[derive(Clone)]
pub struct DefaultDict<K, V, F> {
    map: HashMap<K, V>,
    default_factory: F,
}

impl<K: std::hash::Hash + Eq, V, F: Fn() -> V> DefaultDict<K, V, F> {
    /// Create a new DefaultDict with a default factory.
    pub fn new(default_factory: F) -> Self {
        Self {
            map: HashMap::new(),
            default_factory,
        }
    }
    
    /// Get a value, inserting the default if missing.
    pub fn get_or_default(&mut self, key: K) -> &mut V
    where
        K: Clone,
    {
        self.map.entry(key).or_insert_with(&self.default_factory)
    }
    
    /// Get a value by reference.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }
    
    /// Insert a value.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.map.insert(key, value)
    }
    
    /// Remove a value.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key)
    }
    
    /// Check if key exists.
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.map.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    
    /// Clear all items.
    pub fn clear(&mut self) {
        self.map.clear();
    }
    
    /// Get keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.map.keys()
    }
    
    /// Get values.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.map.values()
    }
    
    /// Get items.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter()
    }
}

/// Create a defaultdict with a list as default.
pub fn defaultdict_list<K: std::hash::Hash + Eq>() -> DefaultDict<K, Vec<Value>, fn() -> Vec<Value>> {
    DefaultDict::new(Vec::new)
}

/// Create a defaultdict with an int (0) as default.
pub fn defaultdict_int<K: std::hash::Hash + Eq>() -> DefaultDict<K, i64, fn() -> i64> {
    DefaultDict::new(|| 0)
}

/// Create a defaultdict with a set as default.
pub fn defaultdict_set<K: std::hash::Hash + Eq>() -> DefaultDict<K, std::collections::HashSet<String>, fn() -> std::collections::HashSet<String>> {
    DefaultDict::new(std::collections::HashSet::new)
}

// =============================================================================
// Counter - Count hashable items
// =============================================================================

/// A counter for counting hashable items.
#[derive(Clone, Debug)]
pub struct Counter<T> {
    counts: HashMap<T, i64>,
}

impl<T: std::hash::Hash + Eq + Clone> Counter<T> {
    /// Create a new empty Counter.
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }
    
    /// Create from an iterator.
    pub fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut counter = Self::new();
        for item in iter {
            counter.add(item, 1);
        }
        counter
    }
    
    /// Add count for an element.
    pub fn add(&mut self, element: T, count: i64) {
        *self.counts.entry(element).or_insert(0) += count;
    }
    
    /// Subtract count for an element.
    pub fn subtract(&mut self, element: T, count: i64) {
        *self.counts.entry(element).or_insert(0) -= count;
    }
    
    /// Get count for an element.
    pub fn get(&self, element: &T) -> i64 {
        self.counts.get(element).copied().unwrap_or(0)
    }
    
    /// Set count for an element.
    pub fn set(&mut self, element: T, count: i64) {
        self.counts.insert(element, count);
    }
    
    /// Remove an element.
    pub fn remove(&mut self, element: &T) -> Option<i64> {
        self.counts.remove(element)
    }
    
    /// Get total count of all elements.
    pub fn total(&self) -> i64 {
        self.counts.values().sum()
    }
    
    /// Get all elements.
    pub fn elements(&self) -> Vec<T> {
        let mut result = Vec::new();
        for (elem, &count) in &self.counts {
            for _ in 0..count.max(0) {
                result.push(elem.clone());
            }
        }
        result
    }
    
    /// Get most common elements.
    pub fn most_common(&self, n: Option<usize>) -> Vec<(T, i64)> {
        let mut items: Vec<_> = self.counts.iter()
            .map(|(k, &v)| (k.clone(), v))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        
        match n {
            Some(n) => items.into_iter().take(n).collect(),
            None => items,
        }
    }
    
    /// Get least common elements.
    pub fn least_common(&self, n: Option<usize>) -> Vec<(T, i64)> {
        let mut items: Vec<_> = self.counts.iter()
            .map(|(k, &v)| (k.clone(), v))
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1));
        
        match n {
            Some(n) => items.into_iter().take(n).collect(),
            None => items,
        }
    }
    
    /// Update with another counter (add counts).
    pub fn update(&mut self, other: &Counter<T>) {
        for (elem, &count) in &other.counts {
            self.add(elem.clone(), count);
        }
    }
    
    /// Subtract another counter.
    pub fn subtract_counter(&mut self, other: &Counter<T>) {
        for (elem, &count) in &other.counts {
            self.subtract(elem.clone(), count);
        }
    }
    
    /// Keep only positive counts.
    pub fn positive(&self) -> Counter<T> {
        Counter {
            counts: self.counts.iter()
                .filter(|(_, &v)| v > 0)
                .map(|(k, &v)| (k.clone(), v))
                .collect(),
        }
    }
    
    /// Union (max of counts).
    pub fn union(&self, other: &Counter<T>) -> Counter<T> {
        let mut result = self.clone();
        for (elem, &count) in &other.counts {
            let current = result.get(elem);
            if count > current {
                result.set(elem.clone(), count);
            }
        }
        result
    }
    
    /// Intersection (min of counts).
    pub fn intersection(&self, other: &Counter<T>) -> Counter<T> {
        let mut result = Counter::new();
        for (elem, &count) in &self.counts {
            let other_count = other.get(elem);
            if other_count > 0 {
                result.set(elem.clone(), count.min(other_count));
            }
        }
        result
    }
    
    /// Get length (number of unique elements).
    pub fn len(&self) -> usize {
        self.counts.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }
    
    /// Clear all counts.
    pub fn clear(&mut self) {
        self.counts.clear();
    }
    
    /// Iterate over elements and counts.
    pub fn iter(&self) -> impl Iterator<Item = (&T, &i64)> {
        self.counts.iter()
    }
}

impl<T: std::hash::Hash + Eq + Clone> Default for Counter<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: std::hash::Hash + Eq + Clone> std::ops::Add for Counter<T> {
    type Output = Counter<T>;
    
    fn add(self, other: Counter<T>) -> Counter<T> {
        let mut result = self;
        result.update(&other);
        result
    }
}

impl<T: std::hash::Hash + Eq + Clone> std::ops::Sub for Counter<T> {
    type Output = Counter<T>;
    
    fn sub(self, other: Counter<T>) -> Counter<T> {
        let mut result = self;
        result.subtract_counter(&other);
        result
    }
}

// =============================================================================
// ChainMap - Multiple dicts as one
// =============================================================================

/// A ChainMap groups multiple dicts together.
#[derive(Clone, Debug)]
pub struct ChainMap<K, V> {
    maps: Vec<HashMap<K, V>>,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> ChainMap<K, V> {
    /// Create a new ChainMap.
    pub fn new() -> Self {
        Self { maps: vec![HashMap::new()] }
    }
    
    /// Create from multiple maps.
    pub fn from_maps(maps: Vec<HashMap<K, V>>) -> Self {
        if maps.is_empty() {
            Self::new()
        } else {
            Self { maps }
        }
    }
    
    /// Get a value, searching through all maps.
    pub fn get(&self, key: &K) -> Option<&V> {
        for map in &self.maps {
            if let Some(v) = map.get(key) {
                return Some(v);
            }
        }
        None
    }
    
    /// Insert into the first map.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.maps.is_empty() {
            self.maps.push(HashMap::new());
        }
        self.maps[0].insert(key, value)
    }
    
    /// Remove from the first map.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if !self.maps.is_empty() {
            self.maps[0].remove(key)
        } else {
            None
        }
    }
    
    /// Check if key exists in any map.
    pub fn contains_key(&self, key: &K) -> bool {
        self.maps.iter().any(|m| m.contains_key(key))
    }
    
    /// Add a new child map at the front.
    pub fn new_child(&mut self) -> &mut HashMap<K, V> {
        self.maps.insert(0, HashMap::new());
        &mut self.maps[0]
    }
    
    /// Get the parent chain (all maps except first).
    pub fn parents(&self) -> ChainMap<K, V> {
        if self.maps.len() <= 1 {
            ChainMap::new()
        } else {
            ChainMap {
                maps: self.maps[1..].to_vec(),
            }
        }
    }
    
    /// Get all keys.
    pub fn keys(&self) -> Vec<K> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        
        for map in &self.maps {
            for key in map.keys() {
                if seen.insert(key.clone()) {
                    result.push(key.clone());
                }
            }
        }
        
        result
    }
    
    /// Get all values.
    pub fn values(&self) -> Vec<V> {
        self.keys().iter()
            .filter_map(|k| self.get(k).cloned())
            .collect()
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Default for ChainMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// NamedTuple helper
// =============================================================================

/// A named tuple (struct-like tuple with field names).
#[derive(Clone, Debug)]
pub struct NamedTuple {
    name: String,
    fields: Vec<String>,
    values: Vec<Value>,
}

impl NamedTuple {
    /// Create a new named tuple type.
    pub fn new(name: &str, fields: Vec<String>, values: Vec<Value>) -> Self {
        Self {
            name: name.to_string(),
            fields,
            values,
        }
    }
    
    /// Get a field by name.
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.fields.iter()
            .position(|f| f == field)
            .and_then(|i| self.values.get(i))
    }
    
    /// Get a field by index.
    pub fn get_idx(&self, idx: usize) -> Option<&Value> {
        self.values.get(idx)
    }
    
    /// Get all field names.
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
    
    /// Get all values.
    pub fn values(&self) -> &[Value] {
        &self.values
    }
    
    /// Get the tuple name.
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Convert to a dict.
    pub fn to_dict(&self) -> HashMap<String, Value> {
        self.fields.iter()
            .cloned()
            .zip(self.values.iter().cloned())
            .collect()
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.values.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ordered_dict() {
        let mut d: OrderedDict<String, i32> = OrderedDict::new();
        d.insert("c".to_string(), 3);
        d.insert("a".to_string(), 1);
        d.insert("b".to_string(), 2);
        
        let keys: Vec<_> = d.keys().cloned().collect();
        assert_eq!(keys, vec!["c", "a", "b"]);
        
        assert_eq!(d.get(&"a".to_string()), Some(&1));
        
        d.move_to_end(&"c".to_string());
        let keys: Vec<_> = d.keys().cloned().collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }
    
    #[test]
    fn test_counter() {
        let mut c: Counter<String> = Counter::new();
        c.add("apple".to_string(), 3);
        c.add("banana".to_string(), 2);
        c.add("apple".to_string(), 1);
        
        assert_eq!(c.get(&"apple".to_string()), 4);
        assert_eq!(c.get(&"banana".to_string()), 2);
        assert_eq!(c.get(&"cherry".to_string()), 0);
        
        let most = c.most_common(Some(1));
        assert_eq!(most[0].0, "apple");
        assert_eq!(most[0].1, 4);
    }
    
    #[test]
    fn test_counter_from_iter() {
        let items = vec!["a", "b", "a", "c", "a", "b"];
        let counter: Counter<&str> = Counter::from_iter(items);
        
        assert_eq!(counter.get(&"a"), 3);
        assert_eq!(counter.get(&"b"), 2);
        assert_eq!(counter.get(&"c"), 1);
    }
    
    #[test]
    fn test_defaultdict() {
        let mut d: DefaultDict<String, i64, _> = DefaultDict::new(|| 0);
        
        *d.get_or_default("key1".to_string()) += 1;
        *d.get_or_default("key1".to_string()) += 1;
        *d.get_or_default("key2".to_string()) += 5;
        
        assert_eq!(d.get(&"key1".to_string()), Some(&2));
        assert_eq!(d.get(&"key2".to_string()), Some(&5));
    }
    
    #[test]
    fn test_chainmap() {
        let mut map1 = HashMap::new();
        map1.insert("a".to_string(), 1);
        map1.insert("b".to_string(), 2);
        
        let mut map2 = HashMap::new();
        map2.insert("b".to_string(), 3);
        map2.insert("c".to_string(), 4);
        
        let chain = ChainMap::from_maps(vec![map1, map2]);
        
        assert_eq!(chain.get(&"a".to_string()), Some(&1));
        assert_eq!(chain.get(&"b".to_string()), Some(&2)); // First map wins
        assert_eq!(chain.get(&"c".to_string()), Some(&4));
    }
    
    #[test]
    fn test_named_tuple() {
        let point = NamedTuple::new(
            "Point",
            vec!["x".to_string(), "y".to_string()],
            vec![Value::Int(10), Value::Int(20)],
        );
        
        assert_eq!(point.get("x"), Some(&Value::Int(10)));
        assert_eq!(point.get("y"), Some(&Value::Int(20)));
        assert_eq!(point.get_idx(0), Some(&Value::Int(10)));
    }
}

