//! Python built-in functions and types.

use crate::bridge::BridgeError;
use crate::types::{PyValue, PyKey, PyException};
use rustc_hash::FxHashMap;

/// Gets Python's built-in functions.
pub fn get_builtins() -> FxHashMap<String, BuiltinFn> {
    let mut builtins: FxHashMap<String, BuiltinFn> = FxHashMap::default();
    
    builtins.insert("print".into(), builtin_print);
    builtins.insert("len".into(), builtin_len);
    builtins.insert("type".into(), builtin_type);
    builtins.insert("int".into(), builtin_int);
    builtins.insert("float".into(), builtin_float);
    builtins.insert("str".into(), builtin_str);
    builtins.insert("bool".into(), builtin_bool);
    builtins.insert("list".into(), builtin_list);
    builtins.insert("tuple".into(), builtin_tuple);
    builtins.insert("dict".into(), builtin_dict);
    builtins.insert("set".into(), builtin_set);
    builtins.insert("range".into(), builtin_range);
    builtins.insert("abs".into(), builtin_abs);
    builtins.insert("min".into(), builtin_min);
    builtins.insert("max".into(), builtin_max);
    builtins.insert("sum".into(), builtin_sum);
    builtins.insert("sorted".into(), builtin_sorted);
    builtins.insert("reversed".into(), builtin_reversed);
    builtins.insert("enumerate".into(), builtin_enumerate);
    builtins.insert("zip".into(), builtin_zip);
    builtins.insert("map".into(), builtin_map);
    builtins.insert("filter".into(), builtin_filter);
    builtins.insert("any".into(), builtin_any);
    builtins.insert("all".into(), builtin_all);
    builtins.insert("ord".into(), builtin_ord);
    builtins.insert("chr".into(), builtin_chr);
    builtins.insert("repr".into(), builtin_repr);
    builtins.insert("hash".into(), builtin_hash);
    builtins.insert("id".into(), builtin_id);
    builtins.insert("isinstance".into(), builtin_isinstance);
    builtins.insert("hasattr".into(), builtin_hasattr);
    builtins.insert("getattr".into(), builtin_getattr);
    builtins.insert("setattr".into(), builtin_setattr);
    builtins.insert("input".into(), builtin_input);
    builtins.insert("open".into(), builtin_open);
    
    builtins
}

pub type BuiltinFn = fn(Vec<PyValue>) -> Result<PyValue, BridgeError>;

fn builtin_print(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    let output: Vec<String> = args.iter().map(|v| format_value(v)).collect();
    println!("{}", output.join(" "));
    Ok(PyValue::None)
}

fn format_value(v: &PyValue) -> String {
    match v {
        PyValue::None => "None".into(),
        PyValue::Bool(b) => if *b { "True" } else { "False" }.into(),
        PyValue::Int(n) => n.to_string(),
        PyValue::Float(f) => f.to_string(),
        PyValue::Str(s) => s.clone(),
        PyValue::Bytes(b) => format!("b'{}'", String::from_utf8_lossy(b)),
        PyValue::List(l) => {
            let items: Vec<_> = l.iter().map(repr_value).collect();
            format!("[{}]", items.join(", "))
        }
        PyValue::Tuple(t) => {
            let items: Vec<_> = t.iter().map(repr_value).collect();
            if t.len() == 1 {
                format!("({},)", items[0])
            } else {
                format!("({})", items.join(", "))
            }
        }
        PyValue::Dict(d) => {
            let items: Vec<_> = d.iter()
                .map(|(k, v)| format!("{}: {}", repr_key(k), repr_value(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        PyValue::Set(s) => {
            let items: Vec<_> = s.iter().map(repr_key).collect();
            format!("{{{}}}", items.join(", "))
        }
        _ => format!("{:?}", v),
    }
}

fn repr_value(v: &PyValue) -> String {
    match v {
        PyValue::Str(s) => format!("'{}'", s.replace('\'', "\\'")),
        _ => format_value(v),
    }
}

fn repr_key(k: &PyKey) -> String {
    match k {
        PyKey::None => "None".into(),
        PyKey::Bool(b) => if *b { "True" } else { "False" }.into(),
        PyKey::Int(n) => n.to_string(),
        PyKey::Str(s) => format!("'{}'", s.replace('\'', "\\'")),
        PyKey::Bytes(b) => format!("b'{}'", String::from_utf8_lossy(b)),
        PyKey::Tuple(t) => {
            let items: Vec<_> = t.iter().map(repr_key).collect();
            format!("({})", items.join(", "))
        }
    }
}

fn builtin_len(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("len() takes exactly 1 argument".into()));
    }
    let len = match &args[0] {
        PyValue::Str(s) => s.len(),
        PyValue::Bytes(b) => b.len(),
        PyValue::List(l) => l.len(),
        PyValue::Tuple(t) => t.len(),
        PyValue::Dict(d) => d.len(),
        PyValue::Set(s) => s.len(),
        _ => return Err(BridgeError::TypeError("object has no len()".into())),
    };
    Ok(PyValue::Int(len as i64))
}

fn builtin_type(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("type() takes 1 argument".into()));
    }
    Ok(PyValue::Str(format!("<class '{}'>", args[0].type_name())))
}

fn builtin_int(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Ok(PyValue::Int(0));
    }
    match &args[0] {
        PyValue::Int(n) => Ok(PyValue::Int(*n)),
        PyValue::Float(f) => Ok(PyValue::Int(*f as i64)),
        PyValue::Bool(b) => Ok(PyValue::Int(if *b { 1 } else { 0 })),
        PyValue::Str(s) => s.trim().parse::<i64>()
            .map(PyValue::Int)
            .map_err(|_| BridgeError::TypeError(format!("invalid literal for int(): '{}'", s))),
        _ => Err(BridgeError::TypeError("int() argument must be a string or number".into())),
    }
}

fn builtin_float(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Ok(PyValue::Float(0.0));
    }
    match &args[0] {
        PyValue::Int(n) => Ok(PyValue::Float(*n as f64)),
        PyValue::Float(f) => Ok(PyValue::Float(*f)),
        PyValue::Str(s) => s.trim().parse::<f64>()
            .map(PyValue::Float)
            .map_err(|_| BridgeError::TypeError(format!("could not convert string to float: '{}'", s))),
        _ => Err(BridgeError::TypeError("float() argument must be a string or number".into())),
    }
}

fn builtin_str(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Ok(PyValue::Str(String::new()));
    }
    Ok(PyValue::Str(format_value(&args[0])))
}

fn builtin_bool(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Ok(PyValue::Bool(false));
    }
    Ok(PyValue::Bool(args[0].is_truthy()))
}

fn builtin_list(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Ok(PyValue::List(vec![]));
    }
    match &args[0] {
        PyValue::List(l) => Ok(PyValue::List(l.clone())),
        PyValue::Tuple(t) => Ok(PyValue::List(t.clone())),
        PyValue::Str(s) => Ok(PyValue::List(s.chars().map(|c| PyValue::Str(c.to_string())).collect())),
        _ => Err(BridgeError::TypeError("list() argument must be iterable".into())),
    }
}

fn builtin_tuple(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Ok(PyValue::Tuple(vec![]));
    }
    match &args[0] {
        PyValue::List(l) => Ok(PyValue::Tuple(l.clone())),
        PyValue::Tuple(t) => Ok(PyValue::Tuple(t.clone())),
        _ => Err(BridgeError::TypeError("tuple() argument must be iterable".into())),
    }
}

fn builtin_dict(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    Ok(PyValue::Dict(FxHashMap::default()))
}

fn builtin_set(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Ok(PyValue::Set(vec![]));
    }
    match &args[0] {
        PyValue::List(l) | PyValue::Tuple(l) => {
            let keys: Result<Vec<_>, _> = l.iter()
                .map(|v| PyKey::from_value(v).ok_or_else(|| BridgeError::TypeError("unhashable type".into())))
                .collect();
            Ok(PyValue::Set(keys?))
        }
        _ => Err(BridgeError::TypeError("set() argument must be iterable".into())),
    }
}

fn builtin_range(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    let (start, stop, step) = match args.len() {
        1 => {
            if let PyValue::Int(stop) = args[0] {
                (0, stop, 1)
            } else {
                return Err(BridgeError::TypeError("range() integer expected".into()));
            }
        }
        2 => {
            if let (PyValue::Int(start), PyValue::Int(stop)) = (&args[0], &args[1]) {
                (*start, *stop, 1)
            } else {
                return Err(BridgeError::TypeError("range() integer expected".into()));
            }
        }
        3 => {
            if let (PyValue::Int(start), PyValue::Int(stop), PyValue::Int(step)) = 
                (&args[0], &args[1], &args[2]) 
            {
                if *step == 0 {
                    return Err(BridgeError::TypeError("range() step cannot be zero".into()));
                }
                (*start, *stop, *step)
            } else {
                return Err(BridgeError::TypeError("range() integer expected".into()));
            }
        }
        _ => return Err(BridgeError::TypeError("range() takes 1 to 3 arguments".into())),
    };
    
    let mut items = Vec::new();
    let mut i = start;
    while (step > 0 && i < stop) || (step < 0 && i > stop) {
        items.push(PyValue::Int(i));
        i += step;
    }
    Ok(PyValue::List(items))
}

fn builtin_abs(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("abs() takes exactly 1 argument".into()));
    }
    match &args[0] {
        PyValue::Int(n) => Ok(PyValue::Int(n.abs())),
        PyValue::Float(f) => Ok(PyValue::Float(f.abs())),
        _ => Err(BridgeError::TypeError("bad operand type for abs()".into())),
    }
}

fn builtin_min(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Err(BridgeError::TypeError("min() requires at least 1 argument".into()));
    }
    let items = if args.len() == 1 {
        match &args[0] {
            PyValue::List(l) => l.clone(),
            PyValue::Tuple(t) => t.clone(),
            _ => return Err(BridgeError::TypeError("min() argument must be iterable".into())),
        }
    } else {
        args
    };
    
    if items.is_empty() {
        return Err(BridgeError::TypeError("min() arg is an empty sequence".into()));
    }
    
    let mut min_val = items[0].clone();
    for item in items.iter().skip(1) {
        match (&min_val, item) {
            (PyValue::Int(a), PyValue::Int(b)) if b < a => min_val = item.clone(),
            (PyValue::Float(a), PyValue::Float(b)) if b < a => min_val = item.clone(),
            (PyValue::Str(a), PyValue::Str(b)) if b < a => min_val = item.clone(),
            _ => {}
        }
    }
    Ok(min_val)
}

fn builtin_max(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Err(BridgeError::TypeError("max() requires at least 1 argument".into()));
    }
    let items = if args.len() == 1 {
        match &args[0] {
            PyValue::List(l) => l.clone(),
            PyValue::Tuple(t) => t.clone(),
            _ => return Err(BridgeError::TypeError("max() argument must be iterable".into())),
        }
    } else {
        args
    };
    
    if items.is_empty() {
        return Err(BridgeError::TypeError("max() arg is an empty sequence".into()));
    }
    
    let mut max_val = items[0].clone();
    for item in items.iter().skip(1) {
        match (&max_val, item) {
            (PyValue::Int(a), PyValue::Int(b)) if b > a => max_val = item.clone(),
            (PyValue::Float(a), PyValue::Float(b)) if b > a => max_val = item.clone(),
            (PyValue::Str(a), PyValue::Str(b)) if b > a => max_val = item.clone(),
            _ => {}
        }
    }
    Ok(max_val)
}

fn builtin_sum(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Err(BridgeError::TypeError("sum() requires at least 1 argument".into()));
    }
    let items = match &args[0] {
        PyValue::List(l) => l,
        PyValue::Tuple(t) => t,
        _ => return Err(BridgeError::TypeError("sum() argument must be iterable".into())),
    };
    
    let start = if args.len() > 1 {
        match &args[1] {
            PyValue::Int(n) => *n,
            _ => return Err(BridgeError::TypeError("sum() start must be a number".into())),
        }
    } else {
        0
    };
    
    let mut total = start;
    for item in items {
        match item {
            PyValue::Int(n) => total += n,
            _ => return Err(BridgeError::TypeError("unsupported operand type for sum()".into())),
        }
    }
    Ok(PyValue::Int(total))
}

fn builtin_sorted(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Err(BridgeError::TypeError("sorted() requires at least 1 argument".into()));
    }
    let mut items = match &args[0] {
        PyValue::List(l) => l.clone(),
        PyValue::Tuple(t) => t.clone(),
        _ => return Err(BridgeError::TypeError("sorted() argument must be iterable".into())),
    };
    
    items.sort_by(|a, b| {
        match (a, b) {
            (PyValue::Int(x), PyValue::Int(y)) => x.cmp(y),
            (PyValue::Float(x), PyValue::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
            (PyValue::Str(x), PyValue::Str(y)) => x.cmp(y),
            _ => std::cmp::Ordering::Equal,
        }
    });
    Ok(PyValue::List(items))
}

fn builtin_reversed(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("reversed() takes exactly 1 argument".into()));
    }
    let mut items = match &args[0] {
        PyValue::List(l) => l.clone(),
        PyValue::Tuple(t) => t.clone(),
        PyValue::Str(s) => s.chars().map(|c| PyValue::Str(c.to_string())).collect(),
        _ => return Err(BridgeError::TypeError("argument must be a sequence".into())),
    };
    items.reverse();
    Ok(PyValue::List(items))
}

fn builtin_enumerate(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Err(BridgeError::TypeError("enumerate() requires at least 1 argument".into()));
    }
    let items = match &args[0] {
        PyValue::List(l) => l.clone(),
        PyValue::Tuple(t) => t.clone(),
        _ => return Err(BridgeError::TypeError("argument must be iterable".into())),
    };
    let start = if args.len() > 1 {
        match &args[1] {
            PyValue::Int(n) => *n,
            _ => 0,
        }
    } else {
        0
    };
    
    let result: Vec<_> = items.into_iter()
        .enumerate()
        .map(|(i, v)| PyValue::Tuple(vec![PyValue::Int(start + i as i64), v]))
        .collect();
    Ok(PyValue::List(result))
}

fn builtin_zip(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.is_empty() {
        return Ok(PyValue::List(vec![]));
    }
    
    let lists: Vec<Vec<PyValue>> = args.iter()
        .map(|a| match a {
            PyValue::List(l) => l.clone(),
            PyValue::Tuple(t) => t.clone(),
            _ => vec![],
        })
        .collect();
    
    let min_len = lists.iter().map(|l| l.len()).min().unwrap_or(0);
    let mut result = Vec::new();
    
    for i in 0..min_len {
        let tuple: Vec<_> = lists.iter().map(|l| l[i].clone()).collect();
        result.push(PyValue::Tuple(tuple));
    }
    
    Ok(PyValue::List(result))
}

fn builtin_map(_args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    // Would need callable support
    Err(BridgeError::TypeError("map() requires callable support".into()))
}

fn builtin_filter(_args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    Err(BridgeError::TypeError("filter() requires callable support".into()))
}

fn builtin_any(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("any() takes exactly 1 argument".into()));
    }
    let items = match &args[0] {
        PyValue::List(l) => l,
        PyValue::Tuple(t) => t,
        _ => return Err(BridgeError::TypeError("argument must be iterable".into())),
    };
    Ok(PyValue::Bool(items.iter().any(|v| v.is_truthy())))
}

fn builtin_all(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("all() takes exactly 1 argument".into()));
    }
    let items = match &args[0] {
        PyValue::List(l) => l,
        PyValue::Tuple(t) => t,
        _ => return Err(BridgeError::TypeError("argument must be iterable".into())),
    };
    Ok(PyValue::Bool(items.iter().all(|v| v.is_truthy())))
}

fn builtin_ord(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("ord() takes exactly 1 argument".into()));
    }
    if let PyValue::Str(s) = &args[0] {
        if s.chars().count() != 1 {
            return Err(BridgeError::TypeError("ord() expected a character".into()));
        }
        Ok(PyValue::Int(s.chars().next().unwrap() as i64))
    } else {
        Err(BridgeError::TypeError("ord() expected string of length 1".into()))
    }
}

fn builtin_chr(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("chr() takes exactly 1 argument".into()));
    }
    if let PyValue::Int(n) = args[0] {
        if n < 0 || n > 0x10FFFF {
            return Err(BridgeError::TypeError("chr() arg not in range".into()));
        }
        Ok(PyValue::Str(char::from_u32(n as u32).unwrap_or('\0').to_string()))
    } else {
        Err(BridgeError::TypeError("chr() integer expected".into()))
    }
}

fn builtin_repr(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("repr() takes exactly 1 argument".into()));
    }
    Ok(PyValue::Str(repr_value(&args[0])))
}

fn builtin_hash(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("hash() takes exactly 1 argument".into()));
    }
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    
    if let Some(key) = PyKey::from_value(&args[0]) {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        Ok(PyValue::Int(hasher.finish() as i64))
    } else {
        Err(BridgeError::TypeError("unhashable type".into()))
    }
}

fn builtin_id(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() != 1 {
        return Err(BridgeError::TypeError("id() takes exactly 1 argument".into()));
    }
    Ok(PyValue::Int(&args[0] as *const _ as i64))
}

fn builtin_isinstance(_args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    // Simplified version
    Ok(PyValue::Bool(false))
}

fn builtin_hasattr(_args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    Ok(PyValue::Bool(false))
}

fn builtin_getattr(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if args.len() < 2 {
        return Err(BridgeError::TypeError("getattr() takes at least 2 arguments".into()));
    }
    if args.len() > 2 {
        Ok(args[2].clone())
    } else {
        Err(BridgeError::TypeError("attribute not found".into()))
    }
}

fn builtin_setattr(_args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    Err(BridgeError::TypeError("setattr() not implemented".into()))
}

fn builtin_input(args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    if !args.is_empty() {
        if let PyValue::Str(prompt) = &args[0] {
            print!("{}", prompt);
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    }
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).map_err(|e| BridgeError::PythonError(e.to_string()))?;
    Ok(PyValue::Str(line.trim_end().to_string()))
}

fn builtin_open(_args: Vec<PyValue>) -> Result<PyValue, BridgeError> {
    // Would need file object support
    Err(BridgeError::TypeError("open() not fully implemented".into()))
}

