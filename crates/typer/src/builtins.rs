//! Built-in types and functions for Roast.

use crate::types::*;
use roast_common::Symbol;
use std::sync::Arc;

/// Registers built-in types and functions in the type context.
pub fn register_builtins(ctx: &mut crate::context::TypeContext, interner: &roast_common::Interner) {
    // Register built-in types
    let type_builtins = [
        ("int", Type::Int),
        ("float", Type::Float),
        ("str", Type::Str),
        ("bytes", Type::Bytes),
        ("bool", Type::Bool),
        ("None", Type::NoneType),
        ("Any", Type::Any),
        ("Never", Type::Never),
    ];

    for (name, ty) in type_builtins {
        let sym = interner.intern(name);
        ctx.register_type(sym, ty);
    }

    // Register built-in functions
    let func_builtins: Vec<(&str, Type)> = vec![
        ("print", print_type()),
        ("len", len_type()),
        ("range", range_type()),
        ("input", input_type()),
        ("int", int_constructor_type()),
        ("float", float_constructor_type()),
        ("str", str_constructor_type()),
        ("bool", bool_constructor_type()),
        ("list", list_constructor_type()),
        ("dict", dict_constructor_type()),
        ("set", set_constructor_type()),
        ("tuple", tuple_constructor_type()),
        ("type", type_type()),
        ("isinstance", isinstance_type()),
        ("issubclass", issubclass_type()),
        ("hasattr", hasattr_type()),
        ("getattr", getattr_type()),
        ("setattr", setattr_type()),
        ("delattr", delattr_type()),
        ("callable", callable_type()),
        ("abs", abs_type()),
        ("min", min_type()),
        ("max", max_type()),
        ("sum", sum_type()),
        ("sorted", sorted_type()),
        ("reversed", reversed_type()),
        ("enumerate", enumerate_type()),
        ("zip", zip_type()),
        ("map", map_type()),
        ("filter", filter_type()),
        ("all", all_type()),
        ("any", any_type()),
        ("open", open_type()),
        ("ord", ord_type()),
        ("chr", chr_type()),
        ("hex", hex_type()),
        ("oct", oct_type()),
        ("bin", bin_type()),
        ("repr", repr_type()),
        ("hash", hash_type()),
        ("id", id_type()),
        ("iter", iter_type()),
        ("next", next_type()),
        ("round", round_type()),
        ("pow", pow_type()),
        ("divmod", divmod_type()),
        ("asyncio_run", asyncio_run_type()),
        ("assert_eq", assert_eq_type()),
        ("assert_ne", assert_ne_type()),
        ("super", super_type()),
        ("bint", bint_constructor_type()),
        ("Some", some_constructor_type()),
        ("rc", rc_constructor_type()),
        // Exception types
        ("Exception", exception_type()),
        ("ValueError", exception_type()),
        ("TypeError", exception_type()),
        ("IndexError", exception_type()),
        ("KeyError", exception_type()),
        ("AttributeError", exception_type()),
        ("RuntimeError", exception_type()),
        ("NotImplementedError", exception_type()),
        ("StopIteration", exception_type()),
        ("ZeroDivisionError", exception_type()),
        // TCP Server functions
        ("tcp_server_create", tcp_server_create_type()),
        ("tcp_server_accept", tcp_server_accept_type()),
        ("tcp_server_close", tcp_server_close_type()),
        ("tcp_client_read", tcp_client_read_type()),
        ("tcp_client_write", tcp_client_write_type()),
        ("tcp_client_close", tcp_client_close_type()),
        // Logging functions
        ("log_debug", log_message_type()),
        ("log_info", log_message_type()),
        ("log_warning", log_message_type()),
        ("log_error", log_message_type()),
        ("log_critical", log_message_type()),
        ("log_set_level", log_set_level_type()),
        ("log_get_level", log_get_level_type()),
    ];

    for (name, ty) in func_builtins {
        let sym = interner.intern(name);
        ctx.define(sym, ty);
    }

    // Register special names
    ctx.define(interner.intern("True"), Type::Bool);
    ctx.define(interner.intern("False"), Type::Bool);
    ctx.define(interner.intern("None"), Type::NoneType);
    ctx.define(interner.intern("__name__"), Type::Str);
    ctx.define(interner.intern("__file__"), Type::Str);
}

/// Creates the type of the built-in `print` function.
pub fn print_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: false,
            kind: ParamKind::VarPositional,
        }],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

/// Creates the type of the built-in `input` function.
pub fn input_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Str,
            default: true,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Str),
        is_async: false,
    }
}

/// Creates the type of the built-in `len` function.
pub fn len_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}

/// Creates the type of the built-in `range` function.
pub fn range_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Int,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Int,
                default: true,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Int,
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::list(Type::Int)),
        is_async: false,
    }
}

/// Constructor types for built-in types
pub fn int_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: true,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}

/// Constructor for bint (BigInt) type
pub fn bint_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Int,  // Takes an int and converts to BigInt
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::BigInt),
        is_async: false,
    }
}

/// Constructor for Some(value) -> Optional[T]
/// Wraps a value in an Optional type (like Rust's Some)
pub fn some_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,  // Takes any value T
            default: false,
            kind: ParamKind::Regular,
        }],
        // Returns Optional[Any] - the actual type is inferred from usage
        returns: Arc::new(Type::Optional(Arc::new(Type::Any))),
        is_async: false,
    }
}

/// Constructor for rc(value) -> rc[T]
/// Wraps a value in a reference-counted container (shared ownership)
pub fn rc_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,  // Takes any value T
            default: false,
            kind: ParamKind::Regular,
        }],
        // Returns rc[Any] - the actual type is inferred from usage
        returns: Arc::new(Type::Rc(Arc::new(Type::Any))),
        is_async: false,
    }
}


pub fn float_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: true,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Float),
        is_async: false,
    }
}

pub fn str_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: true,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Str),
        is_async: false,
    }
}

pub fn bool_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: true,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Bool),
        is_async: false,
    }
}

pub fn list_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: true,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::list(Type::Any)),
        is_async: false,
    }
}

pub fn dict_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: true,
            kind: ParamKind::VarKeyword,
        }],
        returns: Arc::new(Type::dict(Type::Any, Type::Any)),
        is_async: false,
    }
}

pub fn set_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: true,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::set(Type::Any)),
        is_async: false,
    }
}

pub fn tuple_constructor_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: true,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Tuple(vec![])),
        is_async: false,
    }
}

/// Type introspection functions
pub fn type_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Any), // Returns a type object
        is_async: false,
    }
}

pub fn isinstance_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any, // type or tuple of types
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Bool),
        is_async: false,
    }
}

pub fn issubclass_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Bool),
        is_async: false,
    }
}

pub fn hasattr_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Str,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Bool),
        is_async: false,
    }
}

pub fn getattr_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Str,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any),
        is_async: false,
    }
}

pub fn setattr_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Str,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

pub fn delattr_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Str,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

pub fn callable_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Bool),
        is_async: false,
    }
}

/// Numeric functions
pub fn abs_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any, // Works with int, float, complex
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Any),
        is_async: false,
    }
}

pub fn min_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: false,
            kind: ParamKind::VarPositional,
        }],
        returns: Arc::new(Type::Any),
        is_async: false,
    }
}

pub fn max_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: false,
            kind: ParamKind::VarPositional,
        }],
        returns: Arc::new(Type::Any),
        is_async: false,
    }
}

pub fn sum_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any, // Iterable
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any),
        is_async: false,
    }
}

pub fn round_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Float,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Int,
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any), // int or float depending on ndigits
        is_async: false,
    }
}

pub fn pow_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any),
        is_async: false,
    }
}

pub fn divmod_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Tuple(vec![Type::Any, Type::Any])),
        is_async: false,
    }
}

/// Iterable functions
pub fn sorted_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any, // Iterable
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::list(Type::Any)),
        is_async: false,
    }
}

pub fn reversed_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any, // Reversible
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Any), // Iterator
        is_async: false,
    }
}

pub fn enumerate_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any, // Iterable
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Int,
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any), // Iterator of (int, T)
        is_async: false,
    }
}

pub fn zip_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any, // Iterable
            default: false,
            kind: ParamKind::VarPositional,
        }],
        returns: Arc::new(Type::Any), // Iterator of tuples
        is_async: false,
    }
}

pub fn map_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any, // Callable
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any, // Iterable
                default: false,
                kind: ParamKind::VarPositional,
            },
        ],
        returns: Arc::new(Type::Any), // Iterator
        is_async: false,
    }
}

pub fn filter_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any, // Callable or None
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any, // Iterable
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any), // Iterator
        is_async: false,
    }
}

pub fn all_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any, // Iterable
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Bool),
        is_async: false,
    }
}

pub fn any_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any, // Iterable
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Bool),
        is_async: false,
    }
}

pub fn iter_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any, // Iterable
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Any), // Iterator
        is_async: false,
    }
}

pub fn next_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any, // Iterator
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any),
        is_async: false,
    }
}

/// String/character functions
pub fn ord_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Str,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}

pub fn chr_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Int,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Str),
        is_async: false,
    }
}

pub fn hex_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Int,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Str),
        is_async: false,
    }
}

pub fn oct_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Int,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Str),
        is_async: false,
    }
}

pub fn bin_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Int,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Str),
        is_async: false,
    }
}

pub fn repr_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Str),
        is_async: false,
    }
}

/// Object identity/hashing
pub fn hash_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any, // Hashable
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}

pub fn id_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}

/// File I/O
pub fn open_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Str, // path
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Str, // mode
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::File), // File object
        is_async: false,
    }
}

/// Async runtime - runs a coroutine to completion
pub fn asyncio_run_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Any, // Coroutine (any async function result)
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::Any), // The result of the coroutine
        is_async: false,
    }
}

/// Assert that two values are equal
pub fn assert_eq_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Str, // Optional message
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

/// Assert that two values are not equal
pub fn assert_ne_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Any,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Str, // Optional message
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

/// super() builtin - accesses parent class
pub fn super_type() -> Type {
    Type::Callable {
        params: vec![
            // Optional: type argument
            FuncParam {
                name: None,
                ty: Type::Any,
                default: true,
                kind: ParamKind::Regular,
            },
            // Optional: object argument
            FuncParam {
                name: None,
                ty: Type::Any,
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any), // Returns proxy object for parent class
        is_async: false,
    }
}

/// Exception type constructor (ValueError, TypeError, etc.)
pub fn exception_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Str, // Error message
                default: true,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Any), // Returns exception object
        is_async: false,
    }
}

// =============================================================================
// TCP Server Functions
// =============================================================================

/// tcp_server_create(host: str, port: int) -> int
/// Creates a TCP server bound to host:port, returns server handle or -1 on error
pub fn tcp_server_create_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Str,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Int,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}

/// tcp_server_accept(server_handle: int) -> int
/// Accepts a client connection, returns client handle or -1 on error
pub fn tcp_server_accept_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Int,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}

/// tcp_server_close(server_handle: int) -> None
/// Closes a TCP server
pub fn tcp_server_close_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Int,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

/// tcp_client_read(client_handle: int, max_bytes: int) -> str
/// Reads data from client, returns data as string
pub fn tcp_client_read_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Int,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Int,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Str),
        is_async: false,
    }
}

/// tcp_client_write(client_handle: int, data: str) -> int
/// Writes data to client, returns bytes written or -1 on error
pub fn tcp_client_write_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Int,
                default: false,
                kind: ParamKind::Regular,
            },
            FuncParam {
                name: None,
                ty: Type::Str,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}

/// tcp_client_close(client_handle: int) -> None
/// Closes a client connection
pub fn tcp_client_close_type() -> Type {
    Type::Callable {
        params: vec![
            FuncParam {
                name: None,
                ty: Type::Int,
                default: false,
                kind: ParamKind::Regular,
            },
        ],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

// =============================================================================
// Logging Functions
// =============================================================================

/// log_debug(message: str) -> None
/// log_info(message: str) -> None
/// log_warning(message: str) -> None
/// log_error(message: str) -> None
/// log_critical(message: str) -> None
/// Log a message at the specified level
pub fn log_message_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Str,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

/// log_set_level(level: int) -> None
/// Set the global log level (10=DEBUG, 20=INFO, 30=WARNING, 40=ERROR, 50=CRITICAL)
pub fn log_set_level_type() -> Type {
    Type::Callable {
        params: vec![FuncParam {
            name: None,
            ty: Type::Int,
            default: false,
            kind: ParamKind::Regular,
        }],
        returns: Arc::new(Type::NoneType),
        is_async: false,
    }
}

/// log_get_level() -> int
/// Get the current global log level
pub fn log_get_level_type() -> Type {
    Type::Callable {
        params: vec![],
        returns: Arc::new(Type::Int),
        is_async: false,
    }
}
