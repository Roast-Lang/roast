//! LLVM Runtime library declarations and helpers.
//!
//! This module provides the declarations for the Roast runtime library
//! that will be linked with the generated LLVM code.

/// Runtime function declarations as LLVM IR.
pub const RUNTIME_DECLARATIONS: &str = r#"
; === Roast Runtime Library Declarations ===

; Memory management
declare i8* @roast_alloc(i64) nounwind
declare void @roast_free(i8*) nounwind
declare i8* @roast_realloc(i8*, i64) nounwind
declare void @roast_gc_collect() nounwind

; Reference counting
declare void @roast_incref(i8*) nounwind
declare void @roast_decref(i8*) nounwind

; String operations
declare i8* @roast_str_new(i8*, i64) nounwind
declare i8* @roast_str_concat(i8*, i8*) nounwind
declare i64 @roast_str_len(i8*) nounwind
declare i1 @roast_str_eq(i8*, i8*) nounwind
declare i64 @roast_str_cmp(i8*, i8*) nounwind
declare i8* @roast_str_slice(i8*, i64, i64) nounwind
declare i8* @roast_str_format(i8*, ...) nounwind
declare i8* @roast_int_to_str(i64) nounwind
declare i8* @roast_float_to_str(double) nounwind
declare i64 @roast_str_to_int(i8*) nounwind
declare double @roast_str_to_float(i8*) nounwind

; List operations
declare i8* @roast_list_new(i64) nounwind
declare void @roast_list_append(i8*, i64) nounwind
declare i64 @roast_list_get(i8*, i64) nounwind
declare void @roast_list_set(i8*, i64, i64) nounwind
declare i64 @roast_list_len(i8*) nounwind
declare i8* @roast_list_slice(i8*, i64, i64, i64) nounwind
declare i8* @roast_list_concat(i8*, i8*) nounwind
declare void @roast_list_extend(i8*, i8*) nounwind
declare i64 @roast_list_pop(i8*) nounwind
declare void @roast_list_insert(i8*, i64, i64) nounwind
declare void @roast_list_remove(i8*, i64) nounwind
declare i64 @roast_list_index(i8*, i64) nounwind
declare i1 @roast_list_contains(i8*, i64) nounwind

; Dict operations
declare i8* @roast_dict_new() nounwind
declare void @roast_dict_set(i8*, i64, i64) nounwind
declare i64 @roast_dict_get(i8*, i64) nounwind
declare i1 @roast_dict_contains(i8*, i64) nounwind
declare void @roast_dict_delete(i8*, i64) nounwind
declare i64 @roast_dict_len(i8*) nounwind
declare i8* @roast_dict_keys(i8*) nounwind
declare i8* @roast_dict_values(i8*) nounwind
declare i8* @roast_dict_items(i8*) nounwind

; Set operations
declare i8* @roast_set_new() nounwind
declare void @roast_set_add(i8*, i64) nounwind
declare i1 @roast_set_contains(i8*, i64) nounwind
declare void @roast_set_remove(i8*, i64) nounwind
declare i64 @roast_set_len(i8*) nounwind

; Tuple operations
declare i8* @roast_tuple_new(i64) nounwind
declare i64 @roast_tuple_get(i8*, i64) nounwind
declare i64 @roast_tuple_len(i8*) nounwind

; Iterator operations
declare i8* @roast_iter_new(i8*) nounwind
declare i64 @roast_iter_next(i8*, i1*) nounwind
declare i8* @roast_make_iter(i8*) nounwind
declare i8* @roast_range_new(i64, i64, i64) nounwind
declare i8* @roast_enumerate_new(i8*, i64) nounwind
declare i8* @roast_zip_new(i8*, i8*) nounwind
declare i8* @roast_map_new(i8*, i8*) nounwind
declare i8* @roast_filter_new(i8*, i8*) nounwind

; Object/class operations
declare i8* @roast_object_new(i8*) nounwind
declare i64 @roast_object_getattr(i8*, i8*) nounwind
declare void @roast_object_setattr(i8*, i8*, i64) nounwind
declare i1 @roast_object_hasattr(i8*, i8*) nounwind
declare i8* @roast_object_type(i8*) nounwind
declare i1 @roast_isinstance(i8*, i8*) nounwind

; Class operations
declare i8* @roast_class_new(i8*, i8*, i64) nounwind
declare i8* @roast_class_getmethod(i8*, i8*) nounwind

; Closure operations
declare i8* @roast_closure_new(i8*, i64, i8*) nounwind
declare i64 @roast_closure_call(i8*, i64*, i64) nounwind
declare i8* @roast_closure_get_env(i8*, i64) nounwind

; I/O operations
declare void @roast_print(i8*) nounwind
declare void @roast_print_int(i64) nounwind
declare void @roast_print_float(double) nounwind
declare void @roast_print_bool(i1) nounwind
declare void @roast_print_newline() nounwind
declare i8* @roast_input(i8*) nounwind

; Math operations
declare i64 @roast_pow_int(i64, i64) nounwind
declare double @roast_pow_float(double, double) nounwind
declare i64 @roast_abs_int(i64) nounwind
declare double @roast_abs_float(double) nounwind
declare i64 @roast_min_int(i64, i64) nounwind
declare i64 @roast_max_int(i64, i64) nounwind
declare double @roast_min_float(double, double) nounwind
declare double @roast_max_float(double, double) nounwind
declare double @roast_sqrt(double) nounwind
declare double @roast_sin(double) nounwind
declare double @roast_cos(double) nounwind
declare double @roast_tan(double) nounwind
declare double @roast_floor(double) nounwind
declare double @roast_ceil(double) nounwind

; Type conversions
declare i64 @roast_float_to_int(double) nounwind
declare double @roast_int_to_float(i64) nounwind
declare i1 @roast_int_to_bool(i64) nounwind
declare i64 @roast_bool_to_int(i1) nounwind

; Comparison helpers
declare i64 @roast_cmp(i64, i64) nounwind
declare i1 @roast_eq(i64, i64) nounwind

; Exception handling
declare void @roast_raise(i8*, i8*) noreturn
declare void @roast_assert(i1, i8*) nounwind

; Hash operations
declare i64 @roast_hash(i64) nounwind
declare i64 @roast_hash_str(i8*) nounwind

; Async operations
declare i8* @roast_coroutine_new(i8*) nounwind
declare i64 @roast_coroutine_resume(i8*) nounwind
declare void @roast_coroutine_yield(i8*, i64) nounwind
declare i1 @roast_coroutine_done(i8*) nounwind
declare i8* @roast_task_new(i8*) nounwind
declare void @roast_await(i8*) nounwind

; Debug
declare void @roast_debug_print(i8*) nounwind
"#;

/// Type tags for runtime values.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeTag {
    None = 0,
    Bool = 1,
    Int = 2,
    Float = 3,
    Str = 4,
    List = 5,
    Dict = 6,
    Set = 7,
    Tuple = 8,
    Object = 9,
    Class = 10,
    Function = 11,
    Closure = 12,
    Iterator = 13,
    Coroutine = 14,
    Bytes = 15,
}

/// Value representation in LLVM.
/// Uses tagged pointer or NaN-boxing for efficient representation.
#[derive(Clone, Copy, Debug)]
pub enum ValueRepr {
    /// Immediate integer (fits in 63 bits + sign)
    Int,
    /// Immediate boolean
    Bool,
    /// Boxed float (NaN-boxed or pointer)
    Float,
    /// Pointer to heap object
    Ptr,
}

/// Object header layout.
pub const OBJECT_HEADER_SIZE: u64 = 16; // refcount (8) + type_tag (8)

/// List object layout.
pub const LIST_HEADER_SIZE: u64 = 32; // header (16) + len (8) + capacity (8)

/// String object layout.
pub const STRING_HEADER_SIZE: u64 = 24; // header (16) + len (8)

/// Dict bucket size.
pub const DICT_BUCKET_SIZE: u64 = 24; // hash (8) + key (8) + value (8)

/// Generate runtime initialization code.
pub fn runtime_init() -> String {
    String::from(r#"
; Initialize Roast runtime
define internal void @roast_runtime_init() {
entry:
    ; Initialize GC, allocator, etc.
    ret void
}

; Shutdown Roast runtime
define internal void @roast_runtime_shutdown() {
entry:
    call void @roast_gc_collect()
    ret void
}
"#)
}

/// Generate value packing/unpacking helpers.
pub fn value_helpers() -> String {
    String::from(r#"
; Pack an integer into a tagged value
define i64 @roast_pack_int(i64 %val) alwaysinline {
entry:
    ; Tag = 0 for int, value in upper 63 bits
    %shifted = shl i64 %val, 1
    ret i64 %shifted
}

; Unpack an integer from a tagged value
define i64 @roast_unpack_int(i64 %val) alwaysinline {
entry:
    %shifted = ashr i64 %val, 1
    ret i64 %shifted
}

; Pack a boolean into a tagged value
define i64 @roast_pack_bool(i1 %val) alwaysinline {
entry:
    %ext = zext i1 %val to i64
    %tagged = or i64 %ext, 2  ; Tag = 2 for bool
    ret i64 %tagged
}

; Unpack a boolean from a tagged value
define i1 @roast_unpack_bool(i64 %val) alwaysinline {
entry:
    %masked = and i64 %val, 1
    %result = icmp ne i64 %masked, 0
    ret i1 %result
}

; Check if value is a pointer (heap object)
define i1 @roast_is_ptr(i64 %val) alwaysinline {
entry:
    %tag = and i64 %val, 7
    %is_ptr = icmp eq i64 %tag, 4  ; Tag = 4 for pointer
    ret i1 %is_ptr
}

; Pack a pointer into a tagged value
define i64 @roast_pack_ptr(i8* %ptr) alwaysinline {
entry:
    %int = ptrtoint i8* %ptr to i64
    %tagged = or i64 %int, 4  ; Tag = 4 for pointer
    ret i64 %tagged
}

; Unpack a pointer from a tagged value
define i8* @roast_unpack_ptr(i64 %val) alwaysinline {
entry:
    %masked = and i64 %val, -8  ; Clear tag bits
    %ptr = inttoptr i64 %masked to i8*
    ret i8* %ptr
}
"#)
}
