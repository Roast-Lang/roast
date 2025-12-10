//! Python standard library bindings.
//!
//! Provides Roast-compatible implementations of Python's standard library modules.

use crate::bridge::BridgeError;
use crate::types::PyValue;
use crate::modules::PyModule;
use rustc_hash::FxHashMap;
use std::collections::VecDeque;

/// Creates all stdlib module stubs.
pub fn create_stdlib() -> FxHashMap<String, PyModule> {
    let mut modules = FxHashMap::default();
    
    modules.insert("collections".into(), create_collections_module());
    modules.insert("itertools".into(), create_itertools_module());
    modules.insert("functools".into(), create_functools_module());
    modules.insert("typing".into(), create_typing_module());
    modules.insert("datetime".into(), create_datetime_module());
    modules.insert("pathlib".into(), create_pathlib_module());
    modules.insert("io".into(), create_io_module());
    modules.insert("os.path".into(), create_os_path_module());
    modules.insert("subprocess".into(), create_subprocess_module());
    modules.insert("threading".into(), create_threading_module());
    modules.insert("hashlib".into(), create_hashlib_module());
    modules.insert("base64".into(), create_base64_module());
    modules.insert("urllib".into(), create_urllib_module());
    modules.insert("dataclasses".into(), create_dataclasses_module());
    
    modules
}

fn create_collections_module() -> PyModule {
    let mut module = PyModule::new("collections");
    
    // deque
    module.add_function("deque", |args| {
        let items = match args.first() {
            Some(PyValue::List(l)) => l.clone(),
            Some(PyValue::Tuple(t)) => t.clone(),
            None => vec![],
            _ => return Err(BridgeError::TypeError("deque() argument must be iterable".into())),
        };
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "deque".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("_items".into(), PyValue::List(items));
                attrs
            },
        }))
    });
    
    // defaultdict (simplified)
    module.add_function("defaultdict", |args| {
        let default_factory = args.first().cloned().unwrap_or(PyValue::None);
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "defaultdict".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("default_factory".into(), default_factory);
                attrs.insert("_data".into(), PyValue::Dict(FxHashMap::default()));
                attrs
            },
        }))
    });
    
    // Counter
    module.add_function("Counter", |args| {
        let mut counts: FxHashMap<crate::types::PyKey, PyValue> = FxHashMap::default();
        
        if let Some(PyValue::List(items)) = args.first() {
            for item in items {
                if let Some(key) = crate::types::PyKey::from_value(item) {
                    let count = counts.get(&key)
                        .and_then(|v| if let PyValue::Int(n) = v { Some(*n) } else { None })
                        .unwrap_or(0);
                    counts.insert(key, PyValue::Int(count + 1));
                }
            }
        }
        
        Ok(PyValue::Dict(counts))
    });
    
    // namedtuple
    module.add_function("namedtuple", |args| {
        let name = match args.first() {
            Some(PyValue::Str(s)) => s.clone(),
            _ => return Err(BridgeError::TypeError("namedtuple() requires a name".into())),
        };
        
        let fields = match args.get(1) {
            Some(PyValue::List(l)) => l.iter()
                .filter_map(|v| if let PyValue::Str(s) = v { Some(s.clone()) } else { None })
                .collect::<Vec<_>>(),
            Some(PyValue::Str(s)) => s.split_whitespace().map(String::from).collect(),
            _ => return Err(BridgeError::TypeError("namedtuple() requires field names".into())),
        };
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: format!("namedtuple_{}", name),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("_name".into(), PyValue::Str(name));
                attrs.insert("_fields".into(), PyValue::Tuple(
                    fields.into_iter().map(PyValue::Str).collect()
                ));
                attrs
            },
        }))
    });
    
    // OrderedDict
    module.add_function("OrderedDict", |args| {
        let items = match args.first() {
            Some(PyValue::Dict(d)) => d.clone(),
            None => FxHashMap::default(),
            _ => return Err(BridgeError::TypeError("OrderedDict() argument must be a dict".into())),
        };
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "OrderedDict".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("_data".into(), PyValue::Dict(items));
                attrs
            },
        }))
    });
    
    module
}

fn create_itertools_module() -> PyModule {
    let mut module = PyModule::new("itertools");
    
    // count
    module.add_function("count", |args| {
        let start = match args.first() {
            Some(PyValue::Int(n)) => *n,
            None => 0,
            _ => return Err(BridgeError::TypeError("count() start must be an integer".into())),
        };
        let step = match args.get(1) {
            Some(PyValue::Int(n)) => *n,
            None => 1,
            _ => return Err(BridgeError::TypeError("count() step must be an integer".into())),
        };
        
        // Return a count iterator object
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "count".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("start".into(), PyValue::Int(start));
                attrs.insert("step".into(), PyValue::Int(step));
                attrs.insert("current".into(), PyValue::Int(start));
                attrs
            },
        }))
    });
    
    // cycle
    module.add_function("cycle", |args| {
        let items = match args.first() {
            Some(PyValue::List(l)) => l.clone(),
            Some(PyValue::Tuple(t)) => t.clone(),
            _ => return Err(BridgeError::TypeError("cycle() requires an iterable".into())),
        };
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "cycle".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("items".into(), PyValue::List(items));
                attrs.insert("index".into(), PyValue::Int(0));
                attrs
            },
        }))
    });
    
    // chain
    module.add_function("chain", |args| {
        let mut all_items = Vec::new();
        for arg in args {
            match arg {
                PyValue::List(l) => all_items.extend(l),
                PyValue::Tuple(t) => all_items.extend(t),
                _ => return Err(BridgeError::TypeError("chain() arguments must be iterables".into())),
            }
        }
        Ok(PyValue::List(all_items))
    });
    
    // repeat
    module.add_function("repeat", |args| {
        let item = args.first().cloned().unwrap_or(PyValue::None);
        let times = match args.get(1) {
            Some(PyValue::Int(n)) => Some(*n as usize),
            None => None,
            _ => return Err(BridgeError::TypeError("repeat() times must be an integer".into())),
        };
        
        if let Some(n) = times {
            Ok(PyValue::List(vec![item; n]))
        } else {
            // Infinite iterator
            Ok(PyValue::Object(crate::types::PyObject {
                class_name: "repeat".into(),
                attributes: {
                    let mut attrs = FxHashMap::default();
                    attrs.insert("item".into(), item);
                    attrs
                },
            }))
        }
    });
    
    // product
    module.add_function("product", |args| {
        if args.is_empty() {
            return Ok(PyValue::List(vec![PyValue::Tuple(vec![])]));
        }
        
        let iterables: Vec<Vec<PyValue>> = args.iter()
            .map(|a| match a {
                PyValue::List(l) => l.clone(),
                PyValue::Tuple(t) => t.clone(),
                _ => vec![],
            })
            .collect();
        
        // Cartesian product
        fn cartesian(lists: &[Vec<PyValue>]) -> Vec<Vec<PyValue>> {
            if lists.is_empty() {
                return vec![vec![]];
            }
            
            let first = &lists[0];
            let rest = cartesian(&lists[1..]);
            
            let mut result = Vec::new();
            for item in first {
                for suffix in &rest {
                    let mut combo = vec![item.clone()];
                    combo.extend(suffix.clone());
                    result.push(combo);
                }
            }
            result
        }
        
        let products = cartesian(&iterables);
        Ok(PyValue::List(
            products.into_iter().map(PyValue::Tuple).collect()
        ))
    });
    
    // permutations
    module.add_function("permutations", |args| {
        let items: Vec<PyValue> = match args.first() {
            Some(PyValue::List(l)) => l.clone(),
            Some(PyValue::Tuple(t)) => t.clone(),
            _ => return Err(BridgeError::TypeError("permutations() requires an iterable".into())),
        };
        
        let r = match args.get(1) {
            Some(PyValue::Int(n)) => *n as usize,
            None => items.len(),
            _ => return Err(BridgeError::TypeError("permutations() r must be an integer".into())),
        };
        
        fn permute(items: &[PyValue], r: usize) -> Vec<Vec<PyValue>> {
            if r == 0 {
                return vec![vec![]];
            }
            
            let mut result = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let mut remaining = items.to_vec();
                remaining.remove(i);
                
                for mut perm in permute(&remaining, r - 1) {
                    let mut full_perm = vec![item.clone()];
                    full_perm.append(&mut perm);
                    result.push(full_perm);
                }
            }
            result
        }
        
        let perms = permute(&items, r);
        Ok(PyValue::List(
            perms.into_iter().map(PyValue::Tuple).collect()
        ))
    });
    
    // combinations
    module.add_function("combinations", |args| {
        let items: Vec<PyValue> = match args.first() {
            Some(PyValue::List(l)) => l.clone(),
            Some(PyValue::Tuple(t)) => t.clone(),
            _ => return Err(BridgeError::TypeError("combinations() requires an iterable".into())),
        };
        
        let r = match args.get(1) {
            Some(PyValue::Int(n)) => *n as usize,
            _ => return Err(BridgeError::TypeError("combinations() requires r".into())),
        };
        
        fn combine(items: &[PyValue], r: usize, start: usize) -> Vec<Vec<PyValue>> {
            if r == 0 {
                return vec![vec![]];
            }
            
            let mut result = Vec::new();
            for i in start..=items.len().saturating_sub(r) {
                for mut combo in combine(items, r - 1, i + 1) {
                    let mut full_combo = vec![items[i].clone()];
                    full_combo.append(&mut combo);
                    result.push(full_combo);
                }
            }
            result
        }
        
        let combos = combine(&items, r, 0);
        Ok(PyValue::List(
            combos.into_iter().map(PyValue::Tuple).collect()
        ))
    });
    
    module
}

fn create_functools_module() -> PyModule {
    let mut module = PyModule::new("functools");
    
    // partial (simplified)
    module.add_function("partial", |_args| {
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "partial".into(),
            attributes: FxHashMap::default(),
        }))
    });
    
    // reduce
    module.add_function("reduce", |args| {
        // Would need callable support
        Err(BridgeError::TypeError("reduce() requires callable support".into()))
    });
    
    module.attributes.insert("lru_cache".into(), PyValue::Callable(crate::types::PyCallable {
        name: "lru_cache".into(),
        doc: Some("Decorator to wrap a function with a memoizing callable".into()),
    }));
    
    module
}

fn create_typing_module() -> PyModule {
    let mut module = PyModule::new("typing");
    
    // Type aliases
    for name in ["Any", "Optional", "Union", "List", "Dict", "Set", "Tuple",
                 "Callable", "Type", "Generic", "TypeVar", "Protocol",
                 "Literal", "Final", "ClassVar", "Annotated", "Self"] {
        module.attributes.insert(name.into(), PyValue::Str(format!("typing.{}", name)));
    }
    
    // TypeVar
    module.add_function("TypeVar", |args| {
        let name = match args.first() {
            Some(PyValue::Str(s)) => s.clone(),
            _ => return Err(BridgeError::TypeError("TypeVar() requires a name".into())),
        };
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "TypeVar".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("__name__".into(), PyValue::Str(name));
                attrs
            },
        }))
    });
    
    // cast
    module.add_function("cast", |args| {
        // cast(type, value) just returns value
        args.get(1).cloned().ok_or_else(|| BridgeError::TypeError("cast() requires 2 arguments".into()))
    });
    
    // get_type_hints
    module.add_function("get_type_hints", |_args| {
        Ok(PyValue::Dict(FxHashMap::default()))
    });
    
    module
}

fn create_datetime_module() -> PyModule {
    let mut module = PyModule::new("datetime");
    
    // datetime.now()
    module.add_function("now", |_args| {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        
        let secs = now.as_secs();
        let _nanos = now.subsec_nanos();
        
        // Simplified - just return timestamp
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "datetime".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("timestamp".into(), PyValue::Int(secs as i64));
                attrs
            },
        }))
    });
    
    // date
    module.add_function("date", |args| {
        let year = args.first().and_then(|v| if let PyValue::Int(n) = v { Some(*n) } else { None }).unwrap_or(1970);
        let month = args.get(1).and_then(|v| if let PyValue::Int(n) = v { Some(*n) } else { None }).unwrap_or(1);
        let day = args.get(2).and_then(|v| if let PyValue::Int(n) = v { Some(*n) } else { None }).unwrap_or(1);
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "date".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("year".into(), PyValue::Int(year));
                attrs.insert("month".into(), PyValue::Int(month));
                attrs.insert("day".into(), PyValue::Int(day));
                attrs
            },
        }))
    });
    
    // timedelta
    module.add_function("timedelta", |args| {
        let days = args.first().and_then(|v| if let PyValue::Int(n) = v { Some(*n) } else { None }).unwrap_or(0);
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "timedelta".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("days".into(), PyValue::Int(days));
                attrs.insert("seconds".into(), PyValue::Int(0));
                attrs.insert("microseconds".into(), PyValue::Int(0));
                attrs
            },
        }))
    });
    
    module
}

fn create_pathlib_module() -> PyModule {
    let mut module = PyModule::new("pathlib");
    
    // Path
    module.add_function("Path", |args| {
        let path_str = match args.first() {
            Some(PyValue::Str(s)) => s.clone(),
            None => ".".into(),
            _ => return Err(BridgeError::TypeError("Path() requires a string".into())),
        };
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "Path".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("_path".into(), PyValue::Str(path_str.clone()));
                
                let path = std::path::Path::new(&path_str);
                attrs.insert("name".into(), PyValue::Str(
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                ));
                attrs.insert("stem".into(), PyValue::Str(
                    path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                ));
                attrs.insert("suffix".into(), PyValue::Str(
                    path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default()
                ));
                attrs.insert("parent".into(), PyValue::Str(
                    path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
                ));
                
                attrs
            },
        }))
    });
    
    module
}

fn create_io_module() -> PyModule {
    let mut module = PyModule::new("io");
    
    // StringIO
    module.add_function("StringIO", |args| {
        let initial = match args.first() {
            Some(PyValue::Str(s)) => s.clone(),
            None => String::new(),
            _ => return Err(BridgeError::TypeError("StringIO() requires a string".into())),
        };
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "StringIO".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("_buffer".into(), PyValue::Str(initial));
                attrs.insert("_pos".into(), PyValue::Int(0));
                attrs
            },
        }))
    });
    
    // BytesIO
    module.add_function("BytesIO", |args| {
        let initial = match args.first() {
            Some(PyValue::Bytes(b)) => b.clone(),
            None => vec![],
            _ => return Err(BridgeError::TypeError("BytesIO() requires bytes".into())),
        };
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "BytesIO".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("_buffer".into(), PyValue::Bytes(initial));
                attrs.insert("_pos".into(), PyValue::Int(0));
                attrs
            },
        }))
    });
    
    module
}

fn create_os_path_module() -> PyModule {
    let mut module = PyModule::new("os.path");
    
    module.add_function("exists", |args| {
        let path = match args.first() {
            Some(PyValue::Str(s)) => s,
            _ => return Err(BridgeError::TypeError("exists() requires a path string".into())),
        };
        Ok(PyValue::Bool(std::path::Path::new(path).exists()))
    });
    
    module.add_function("isfile", |args| {
        let path = match args.first() {
            Some(PyValue::Str(s)) => s,
            _ => return Err(BridgeError::TypeError("isfile() requires a path string".into())),
        };
        Ok(PyValue::Bool(std::path::Path::new(path).is_file()))
    });
    
    module.add_function("isdir", |args| {
        let path = match args.first() {
            Some(PyValue::Str(s)) => s,
            _ => return Err(BridgeError::TypeError("isdir() requires a path string".into())),
        };
        Ok(PyValue::Bool(std::path::Path::new(path).is_dir()))
    });
    
    module.add_function("join", |args| {
        let parts: Vec<&str> = args.iter()
            .filter_map(|a| if let PyValue::Str(s) = a { Some(s.as_str()) } else { None })
            .collect();
        
        let path: std::path::PathBuf = parts.iter().collect();
        Ok(PyValue::Str(path.to_string_lossy().to_string()))
    });
    
    module.add_function("basename", |args| {
        let path = match args.first() {
            Some(PyValue::Str(s)) => s,
            _ => return Err(BridgeError::TypeError("basename() requires a path string".into())),
        };
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(PyValue::Str(name))
    });
    
    module.add_function("dirname", |args| {
        let path = match args.first() {
            Some(PyValue::Str(s)) => s,
            _ => return Err(BridgeError::TypeError("dirname() requires a path string".into())),
        };
        let dir = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(PyValue::Str(dir))
    });
    
    module.add_function("splitext", |args| {
        let path = match args.first() {
            Some(PyValue::Str(s)) => s,
            _ => return Err(BridgeError::TypeError("splitext() requires a path string".into())),
        };
        let p = std::path::Path::new(path);
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        
        // Include the parent path with stem
        let parent = p.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let base = if parent.is_empty() { stem } else { format!("{}/{}", parent, stem) };
        
        Ok(PyValue::Tuple(vec![PyValue::Str(base), PyValue::Str(ext)]))
    });
    
    module
}

fn create_subprocess_module() -> PyModule {
    let mut module = PyModule::new("subprocess");
    
    module.attributes.insert("PIPE".into(), PyValue::Int(-1));
    module.attributes.insert("STDOUT".into(), PyValue::Int(-2));
    module.attributes.insert("DEVNULL".into(), PyValue::Int(-3));
    
    module.add_function("run", |args| {
        let cmd = match args.first() {
            Some(PyValue::List(l)) => l.iter()
                .filter_map(|v| if let PyValue::Str(s) = v { Some(s.clone()) } else { None })
                .collect::<Vec<_>>(),
            Some(PyValue::Str(s)) => vec![s.clone()],
            _ => return Err(BridgeError::TypeError("run() requires command".into())),
        };
        
        if cmd.is_empty() {
            return Err(BridgeError::TypeError("run() requires a command".into()));
        }
        
        match std::process::Command::new(&cmd[0])
            .args(&cmd[1..])
            .output()
        {
            Ok(output) => {
                Ok(PyValue::Object(crate::types::PyObject {
                    class_name: "CompletedProcess".into(),
                    attributes: {
                        let mut attrs = FxHashMap::default();
                        attrs.insert("returncode".into(), PyValue::Int(output.status.code().unwrap_or(-1) as i64));
                        attrs.insert("stdout".into(), PyValue::Bytes(output.stdout));
                        attrs.insert("stderr".into(), PyValue::Bytes(output.stderr));
                        attrs
                    },
                }))
            }
            Err(e) => Err(BridgeError::PythonError(e.to_string())),
        }
    });
    
    module
}

fn create_threading_module() -> PyModule {
    let mut module = PyModule::new("threading");
    
    module.add_function("Thread", |_args| {
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "Thread".into(),
            attributes: FxHashMap::default(),
        }))
    });
    
    module.add_function("Lock", |_args| {
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "Lock".into(),
            attributes: FxHashMap::default(),
        }))
    });
    
    module.add_function("Event", |_args| {
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "Event".into(),
            attributes: FxHashMap::default(),
        }))
    });
    
    module.add_function("current_thread", |_args| {
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "Thread".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                attrs.insert("name".into(), PyValue::Str("MainThread".into()));
                attrs
            },
        }))
    });
    
    module
}

fn create_hashlib_module() -> PyModule {
    let mut module = PyModule::new("hashlib");
    
    // Note: Would need actual crypto implementation
    for alg in ["md5", "sha1", "sha256", "sha512", "blake2b", "blake2s"] {
        let alg_name = alg.to_string();
        module.add_function(alg, move |args| {
            let data = match args.first() {
                Some(PyValue::Bytes(b)) => b.clone(),
                Some(PyValue::Str(s)) => s.as_bytes().to_vec(),
                None => vec![],
                _ => return Err(BridgeError::TypeError("hash requires bytes or string".into())),
            };
            
            Ok(PyValue::Object(crate::types::PyObject {
                class_name: format!("{}_hash", alg_name),
                attributes: {
                    let mut attrs = FxHashMap::default();
                    attrs.insert("_data".into(), PyValue::Bytes(data));
                    attrs.insert("name".into(), PyValue::Str(alg_name.clone()));
                    attrs
                },
            }))
        });
    }
    
    module
}

fn create_base64_module() -> PyModule {
    let mut module = PyModule::new("base64");
    
    module.add_function("b64encode", |args| {
        let data = match args.first() {
            Some(PyValue::Bytes(b)) => b,
            Some(PyValue::Str(s)) => return Ok(PyValue::Str(base64_encode(s.as_bytes()))),
            _ => return Err(BridgeError::TypeError("b64encode requires bytes".into())),
        };
        Ok(PyValue::Str(base64_encode(data)))
    });
    
    module.add_function("b64decode", |args| {
        let data = match args.first() {
            Some(PyValue::Str(s)) => s,
            Some(PyValue::Bytes(b)) => {
                return base64_decode(&String::from_utf8_lossy(b))
                    .map(PyValue::Bytes)
                    .map_err(|e| BridgeError::TypeError(e));
            }
            _ => return Err(BridgeError::TypeError("b64decode requires string".into())),
        };
        base64_decode(data)
            .map(PyValue::Bytes)
            .map_err(|e| BridgeError::TypeError(e))
    });
    
    module
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let n = match chunk.len() {
            1 => ((chunk[0] as u32) << 16, 2),
            2 => (((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8), 3),
            3 => (((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32), 4),
            _ => unreachable!(),
        };
        
        for i in 0..n.1 {
            let idx = ((n.0 >> (18 - 6 * i)) & 0x3F) as usize;
            result.push(ALPHABET[idx] as char);
        }
        
        for _ in n.1..4 {
            result.push('=');
        }
    }
    
    result
}

fn base64_decode(data: &str) -> Result<Vec<u8>, String> {
    const DECODE: [i8; 128] = [
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,62,-1,-1,-1,63,
        52,53,54,55,56,57,58,59,60,61,-1,-1,-1,-1,-1,-1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,
        15,16,17,18,19,20,21,22,23,24,25,-1,-1,-1,-1,-1,
        -1,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,
        41,42,43,44,45,46,47,48,49,50,51,-1,-1,-1,-1,-1,
    ];
    
    let data = data.trim_end_matches('=');
    let mut result = Vec::new();
    
    let bytes: Vec<u8> = data.bytes().collect();
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        let mut count = 0;
        
        for &b in chunk {
            if b >= 128 {
                return Err("Invalid base64 character".into());
            }
            let val = DECODE[b as usize];
            if val < 0 {
                return Err("Invalid base64 character".into());
            }
            n = (n << 6) | (val as u32);
            count += 1;
        }
        
        match count {
            2 => result.push((n >> 4) as u8),
            3 => {
                result.push((n >> 10) as u8);
                result.push((n >> 2) as u8);
            }
            4 => {
                result.push((n >> 16) as u8);
                result.push((n >> 8) as u8);
                result.push(n as u8);
            }
            _ => {}
        }
    }
    
    Ok(result)
}

fn create_urllib_module() -> PyModule {
    let mut module = PyModule::new("urllib");
    
    module.add_function("quote", |args| {
        let s = match args.first() {
            Some(PyValue::Str(s)) => s,
            _ => return Err(BridgeError::TypeError("quote() requires a string".into())),
        };
        
        let mut result = String::new();
        for c in s.chars() {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                result.push(c);
            } else {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
        Ok(PyValue::Str(result))
    });
    
    module.add_function("unquote", |args| {
        let s = match args.first() {
            Some(PyValue::Str(s)) => s,
            _ => return Err(BridgeError::TypeError("unquote() requires a string".into())),
        };
        
        let mut result = Vec::new();
        let bytes = s.as_bytes();
        let mut i = 0;
        
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Ok(val) = u8::from_str_radix(
                    &String::from_utf8_lossy(&bytes[i+1..i+3]),
                    16
                ) {
                    result.push(val);
                    i += 3;
                    continue;
                }
            }
            result.push(bytes[i]);
            i += 1;
        }
        
        Ok(PyValue::Str(String::from_utf8_lossy(&result).to_string()))
    });
    
    module
}

fn create_dataclasses_module() -> PyModule {
    let mut module = PyModule::new("dataclasses");
    
    module.attributes.insert("dataclass".into(), PyValue::Callable(crate::types::PyCallable {
        name: "dataclass".into(),
        doc: Some("Decorator that generates __init__ and other methods".into()),
    }));
    
    module.add_function("field", |args| {
        let default = args.first().cloned();
        let default_factory = args.get(1).cloned();
        
        Ok(PyValue::Object(crate::types::PyObject {
            class_name: "Field".into(),
            attributes: {
                let mut attrs = FxHashMap::default();
                if let Some(d) = default {
                    attrs.insert("default".into(), d);
                }
                if let Some(f) = default_factory {
                    attrs.insert("default_factory".into(), f);
                }
                attrs
            },
        }))
    });
    
    module.add_function("asdict", |args| {
        match args.first() {
            Some(PyValue::Object(obj)) => {
                let dict: FxHashMap<crate::types::PyKey, PyValue> = obj.attributes.iter()
                    .filter(|(k, _)| !k.starts_with('_'))
                    .map(|(k, v)| (crate::types::PyKey::Str(k.clone()), v.clone()))
                    .collect();
                Ok(PyValue::Dict(dict))
            }
            _ => Err(BridgeError::TypeError("asdict() requires a dataclass instance".into())),
        }
    });
    
    module.add_function("astuple", |args| {
        match args.first() {
            Some(PyValue::Object(obj)) => {
                let values: Vec<PyValue> = obj.attributes.iter()
                    .filter(|(k, _)| !k.starts_with('_'))
                    .map(|(_, v)| v.clone())
                    .collect();
                Ok(PyValue::Tuple(values))
            }
            _ => Err(BridgeError::TypeError("astuple() requires a dataclass instance".into())),
        }
    });
    
    module
}

