//! Built-in functions and types.

use roast_runtime::Value;

/// Print values to stdout.
pub fn print(args: &[Value]) {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        match arg {
            Value::None => print!("None"),
            Value::Bool(b) => print!("{}", if *b { "True" } else { "False" }),
            Value::Int(n) => print!("{}", n),
            Value::Float(f) => print!("{}", f),
            Value::Str(s) => print!("{}", s),
            Value::List(l) => {
                print!("[");
                let l_guard = l.lock().unwrap();
                for (i, item) in l_guard.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    print_value(item);
                }
                print!("]");
            }
            Value::Tuple(t) => {
                print!("(");
                for (i, item) in t.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    print_value(item);
                }
                print!(")");
            }
            _ => print!("<value>"),
        }
    }
    println!();
}

fn print_value(v: &Value) {
    match v {
        Value::None => print!("None"),
        Value::Bool(b) => print!("{}", if *b { "True" } else { "False" }),
        Value::Int(n) => print!("{}", n),
        Value::Float(f) => print!("{}", f),
        Value::Str(s) => print!("'{}'", s),
        _ => print!("<value>"),
    }
}

/// Get length of a value.
pub fn len(v: &Value) -> Option<i64> {
    match v {
        Value::Str(s) => Some(s.len() as i64),
        Value::List(l) => Some(l.lock().unwrap().len() as i64),
        Value::Tuple(t) => Some(t.len() as i64),
        Value::Dict(d) => Some(d.lock().unwrap().len() as i64),
        Value::Set(s) => Some(s.lock().unwrap().len() as i64),
        Value::Bytes(b) => Some(b.len() as i64),
        _ => None,
    }
}

/// Convert to string.
pub fn str_builtin(v: &Value) -> String {
    match v {
        Value::None => "None".to_string(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Str(s) => s.to_string(),
        _ => "<value>".to_string(),
    }
}

/// Convert to int.
pub fn int_builtin(v: &Value) -> Option<i64> {
    match v {
        Value::Int(n) => Some(*n),
        Value::Float(f) => Some(*f as i64),
        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
        Value::Str(s) => s.parse().ok(),
        _ => None,
    }
}

/// Convert to float.
pub fn float_builtin(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Str(s) => s.parse().ok(),
        _ => None,
    }
}

/// Convert to bool.
pub fn bool_builtin(v: &Value) -> bool {
    v.is_truthy()
}

/// Create a range.
pub fn range(start: i64, stop: i64, step: i64) -> Vec<Value> {
    let mut result = Vec::new();
    let mut i = start;
    if step > 0 {
        while i < stop {
            result.push(Value::Int(i));
            i += step;
        }
    } else if step < 0 {
        while i > stop {
            result.push(Value::Int(i));
            i += step;
        }
    }
    result
}

/// Absolute value.
pub fn abs(v: &Value) -> Option<Value> {
    match v {
        Value::Int(n) => Some(Value::Int(n.abs())),
        Value::Float(f) => Some(Value::Float(f.abs())),
        _ => None,
    }
}

/// Minimum value.
pub fn min(args: &[Value]) -> Option<Value> {
    args.iter().min_by(|a, b| {
        match (a, b) {
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
            _ => std::cmp::Ordering::Equal,
        }
    }).cloned()
}

/// Maximum value.
pub fn max(args: &[Value]) -> Option<Value> {
    args.iter().max_by(|a, b| {
        match (a, b) {
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
            _ => std::cmp::Ordering::Equal,
        }
    }).cloned()
}

/// Sum of values.
pub fn sum(args: &[Value]) -> Value {
    let mut total = 0i64;
    let mut is_float = false;
    let mut float_total = 0.0f64;

    for v in args {
        match v {
            Value::Int(n) => {
                if is_float {
                    float_total += *n as f64;
                } else {
                    total += n;
                }
            }
            Value::Float(f) => {
                if !is_float {
                    is_float = true;
                    float_total = total as f64;
                }
                float_total += f;
            }
            _ => {}
        }
    }

    if is_float {
        Value::Float(float_total)
    } else {
        Value::Int(total)
    }
}

/// Check if all values are truthy.
pub fn all(args: &[Value]) -> bool {
    args.iter().all(|v| v.is_truthy())
}

/// Check if any value is truthy.
pub fn any(args: &[Value]) -> bool {
    args.iter().any(|v| v.is_truthy())
}

/// Enumerate an iterable.
pub fn enumerate(items: &[Value], start: i64) -> Vec<Value> {
    items
        .iter()
        .enumerate()
        .map(|(i, v)| {
            Value::Tuple(std::sync::Arc::new([
                Value::Int(start + i as i64),
                v.clone(),
            ]))
        })
        .collect()
}

/// Zip iterables together.
pub fn zip(a: &[Value], b: &[Value]) -> Vec<Value> {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| Value::Tuple(std::sync::Arc::new([x.clone(), y.clone()])))
        .collect()
}
