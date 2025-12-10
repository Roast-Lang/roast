//! Roast object trait and implementations.

use crate::value::Value;
use std::any::Any;
use std::fmt::Debug;

/// Trait for Roast objects.
pub trait RoastObject: Any + Debug + Send + Sync {
    /// Returns the type name.
    fn type_name(&self) -> &str;

    /// Gets an attribute.
    fn get_attr(&self, name: &str) -> Option<Value>;

    /// Sets an attribute.
    fn set_attr(&mut self, name: &str, value: Value) -> Result<(), String>;

    /// Calls the object as a function.
    fn call(&self, args: &[Value]) -> Result<Value, String>;

    /// Returns string representation.
    fn repr(&self) -> String;

    /// Returns hash if hashable.
    fn hash(&self) -> Option<u64>;

    /// Compares for equality.
    fn eq(&self, other: &dyn RoastObject) -> bool;

    /// Returns length if sized.
    fn len(&self) -> Option<usize>;

    /// Gets item by index/key.
    fn get_item(&self, key: &Value) -> Result<Value, String>;

    /// Sets item by index/key.
    fn set_item(&mut self, key: &Value, value: Value) -> Result<(), String>;

    /// Iterates over items.
    fn iter(&self) -> Option<Box<dyn Iterator<Item = Value> + '_>>;
}

/// A simple class instance.
#[derive(Debug)]
pub struct ClassInstance {
    pub class_name: String,
    pub attrs: std::collections::HashMap<String, Value>,
}

impl ClassInstance {
    pub fn new(class_name: String) -> Self {
        Self {
            class_name,
            attrs: std::collections::HashMap::new(),
        }
    }
}

impl RoastObject for ClassInstance {
    fn type_name(&self) -> &str {
        &self.class_name
    }

    fn get_attr(&self, name: &str) -> Option<Value> {
        self.attrs.get(name).cloned()
    }

    fn set_attr(&mut self, name: &str, value: Value) -> Result<(), String> {
        self.attrs.insert(name.to_string(), value);
        Ok(())
    }

    fn call(&self, _args: &[Value]) -> Result<Value, String> {
        Err(format!("'{}' object is not callable", self.class_name))
    }

    fn repr(&self) -> String {
        format!("<{} object>", self.class_name)
    }

    fn hash(&self) -> Option<u64> {
        None
    }

    fn eq(&self, _other: &dyn RoastObject) -> bool {
        false
    }

    fn len(&self) -> Option<usize> {
        None
    }

    fn get_item(&self, _key: &Value) -> Result<Value, String> {
        Err(format!("'{}' object is not subscriptable", self.class_name))
    }

    fn set_item(&mut self, _key: &Value, _value: Value) -> Result<(), String> {
        Err(format!("'{}' object does not support item assignment", self.class_name))
    }

    fn iter(&self) -> Option<Box<dyn Iterator<Item = Value> + '_>> {
        None
    }
}

