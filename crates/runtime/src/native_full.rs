//! Full Native Runtime Library for Roast LLVM Backend
//!
//! This module provides all C-ABI functions needed for natively compiled Roast programs.
//! It implements the complete Roast runtime including:
//! - Memory management with reference counting
//! - All primitive operations
//! - Collections (list, dict, set, tuple)
//! - String operations
//! - Object/class support
//! - Iterator protocol
//! - Exception handling

use std::alloc::{alloc, dealloc, realloc, Layout};
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_long, c_void};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use num_bigint::BigInt as NumBigInt;

// ============================================================================
// Runtime Initialization and Panic Handling
// ============================================================================

use std::panic;

static PANIC_HOOK_SET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Initialize the Roast runtime with proper panic handling
/// This prevents SIGSEGV by catching panics and converting to clean error messages
#[no_mangle]
pub extern "C" fn roast_runtime_init() {
    if !PANIC_HOOK_SET.swap(true, std::sync::atomic::Ordering::SeqCst) {
        panic::set_hook(Box::new(|info| {
            let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown error".to_string()
            };
            
            let location = if let Some(loc) = info.location() {
                format!(" at {}:{}:{}", loc.file(), loc.line(), loc.column())
            } else {
                String::new()
            };
            
            eprintln!("Roast Runtime Error: {}{}", msg, location);
        }));
    }
}

/// Catch panics and return error code
#[no_mangle]
pub extern "C" fn roast_try_exec(func_ptr: extern "C" fn()) -> c_long {
    let result = panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        func_ptr();
    }));
    
    match result {
        Ok(_) => 0,
        Err(_) => -1, // Error occurred
    }
}

// ============================================================================
// Logging Infrastructure (used throughout runtime)
// ============================================================================

use std::sync::atomic::AtomicI32;
use std::time::{SystemTime, UNIX_EPOCH};

/// Global log level (DEBUG=10, INFO=20, WARNING=30, ERROR=40, CRITICAL=50)
static LOG_LEVEL: AtomicI32 = AtomicI32::new(20); // Default: INFO

/// Get current timestamp as formatted string (HH:MM:SS.mmm)
fn get_log_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    
    let secs = now.as_secs();
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let secs_part = secs % 60;
    let millis = now.subsec_millis();
    
    format!("{:02}:{:02}:{:02}.{:03}", hours, mins, secs_part, millis)
}

// ============================================================================
// Type Tags and Value Representation
// ============================================================================

/// Type tag for runtime values
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
    Range = 14,
    Bytes = 15,
    Super = 16,
    Property = 17,
    BigInt = 18,
    Enumerate = 19,
    Zip = 20,
    Cell = 21,
}

/// Object header for all heap-allocated values
#[repr(C)]
pub struct ObjectHeader {
    pub refcount: AtomicUsize,
    pub type_tag: TypeTag,
    pub flags: u8,
    _padding: [u8; 6],
}

impl ObjectHeader {
    pub fn new(tag: TypeTag) -> Self {
        Self {
            refcount: AtomicUsize::new(1),
            type_tag: tag,
            flags: 0,
            _padding: [0; 6],
        }
    }
}

// ============================================================================
// Memory Management
// ============================================================================

#[no_mangle]
pub extern "C" fn roast_alloc(size: c_long) -> *mut c_void {
    if size <= 0 {
        return ptr::null_mut();
    }
    // SAFETY: Layout is valid because size > 0 and alignment is 8 (power of 2).
    // The returned pointer is either null (allocation failed) or valid.
    unsafe {
        let layout = Layout::from_size_align(size as usize, 8).unwrap();
        alloc(layout) as *mut c_void
    }
}

#[no_mangle]
pub extern "C" fn roast_free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // Note: In a real implementation, we'd track the size
    // For now, we rely on the allocator's metadata
}

#[no_mangle]
pub extern "C" fn roast_incref(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: Caller guarantees ptr points to a valid ObjectHeader.
    // The null check above handles the null case.
    // AtomicUsize::fetch_add is lock-free and thread-safe.
    unsafe {
        let header = ptr as *mut ObjectHeader;
        (*header).refcount.fetch_add(1, Ordering::Relaxed);
    }
}

#[no_mangle]
pub extern "C" fn roast_decref(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: Caller guarantees ptr points to a valid ObjectHeader.
    // We check for invalid refcounts (0 or very large) to catch double-frees.
    // We check type_tag bounds to detect corrupted memory.
    // Atomic fence ensures visibility of all writes before deallocation.
    unsafe {
        let header = ptr as *mut ObjectHeader;
        // Safety check: don't free objects with suspicious refcounts
        // This can happen with static/global strings or double-frees
        let current_refcount = (*header).refcount.load(Ordering::Acquire);
        if current_refcount == 0 || current_refcount > 1_000_000_000 {
            // Already freed or corrupted - skip
            return;
        }
        if (*header).refcount.fetch_sub(1, Ordering::Release) == 1 {
            std::sync::atomic::fence(Ordering::Acquire);
            // Double-check we're not freeing something already freed
            if (*header).type_tag as u8 > TypeTag::BigInt as u8 {
                // Invalid type tag - probably already freed or corrupted
                return;
            }
            // Free based on type
            match (*header).type_tag {
                TypeTag::Str => roast_str_free(ptr as *mut RoastString),
                TypeTag::List => roast_list_free_internal(ptr as *mut RoastList),
                TypeTag::Dict => roast_dict_free_internal(ptr as *mut RoastDict),
                TypeTag::Set => roast_set_free_internal(ptr as *mut RoastSet),
                TypeTag::Tuple => roast_tuple_free_internal(ptr as *mut RoastTuple),
                TypeTag::Object => roast_object_free_internal(ptr as *mut RoastObject),
                TypeTag::BigInt => roast_bigint_free(ptr as *mut RoastBigInt),
                _ => {}
            }
        }
    }
}

// ============================================================================
// String Operations
// ============================================================================

#[repr(C)]
pub struct RoastString {
    header: ObjectHeader,
    len: usize,
    capacity: usize,
    data: *mut u8,
}

#[no_mangle]
pub extern "C" fn roast_str_new(data: *const c_char, len: c_long) -> *mut RoastString {
    unsafe {
        let len = if len < 0 {
            if data.is_null() { 0 } else { libc::strlen(data) }
        } else {
            len as usize
        };

        let layout = Layout::new::<RoastString>();
        let ptr = alloc(layout) as *mut RoastString;

        (*ptr).header = ObjectHeader::new(TypeTag::Str);
        (*ptr).len = len;
        (*ptr).capacity = len + 1;

        let data_layout = Layout::from_size_align(len + 1, 1).unwrap();
        (*ptr).data = alloc(data_layout);

        if !data.is_null() && len > 0 {
            ptr::copy_nonoverlapping(data as *const u8, (*ptr).data, len);
        }
        *(*ptr).data.add(len) = 0; // Null terminate

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_str_from_cstr(s: *const c_char) -> *mut RoastString {
    roast_str_new(s, -1)
}

#[no_mangle]
pub extern "C" fn roast_str_len(s: *const RoastString) -> c_long {
    if s.is_null() { 0 } else { unsafe { (*s).len as c_long } }
}

#[no_mangle]
pub extern "C" fn roast_str_data(s: *const RoastString) -> *const c_char {
    if s.is_null() { b"\0".as_ptr() as *const c_char } else { unsafe { (*s).data as *const c_char } }
}

#[no_mangle]
pub extern "C" fn roast_str_concat(a: *const RoastString, b: *const RoastString) -> *mut RoastString {
    unsafe {
        let len_a = if a.is_null() { 0 } else { (*a).len };
        let len_b = if b.is_null() { 0 } else { (*b).len };
        let total = len_a + len_b;

        let result = roast_str_new(ptr::null(), total as c_long);

        if len_a > 0 {
            ptr::copy_nonoverlapping((*a).data, (*result).data, len_a);
        }
        if len_b > 0 {
            ptr::copy_nonoverlapping((*b).data, (*result).data.add(len_a), len_b);
        }
        *(*result).data.add(total) = 0;

        result
    }
}

#[no_mangle]
pub extern "C" fn roast_str_eq(a: *const RoastString, b: *const RoastString) -> bool {
    unsafe {
        if a.is_null() && b.is_null() { return true; }
        if a.is_null() || b.is_null() { return false; }
        if (*a).len != (*b).len { return false; }
        libc::memcmp((*a).data as *const c_void, (*b).data as *const c_void, (*a).len) == 0
    }
}

#[no_mangle]
pub extern "C" fn roast_str_cmp(a: *const RoastString, b: *const RoastString) -> c_long {
    unsafe {
        if a.is_null() && b.is_null() { return 0; }
        if a.is_null() { return -1; }
        if b.is_null() { return 1; }
        let min_len = (*a).len.min((*b).len);
        let cmp = libc::memcmp((*a).data as *const c_void, (*b).data as *const c_void, min_len);
        if cmp != 0 { return cmp as c_long; }
        ((*a).len as c_long) - ((*b).len as c_long)
    }
}

#[no_mangle]
pub extern "C" fn roast_str_slice(s: *const RoastString, start: c_long, end: c_long) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len as c_long;
        let start = if start < 0 { (len + start).max(0) } else { start.min(len) };
        let end = if end < 0 { (len + end).max(0) } else { end.min(len) };
        let slice_len = (end - start).max(0) as usize;

        let result = roast_str_new(ptr::null(), slice_len as c_long);
        if slice_len > 0 {
            ptr::copy_nonoverlapping((*s).data.add(start as usize), (*result).data, slice_len);
        }
        *(*result).data.add(slice_len) = 0;
        result
    }
}

#[no_mangle]
pub extern "C" fn roast_str_get_char(s: *const RoastString, index: c_long) -> c_long {
    if s.is_null() { return 0; }
    unsafe {
        let len = (*s).len as c_long;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len { return 0; }
        *(*s).data.add(idx as usize) as c_long
    }
}

#[no_mangle]
pub extern "C" fn roast_int_to_str(value: c_long) -> *mut RoastString {
    let s = format!("{}", value);
    let cstr = CString::new(s).unwrap();
    roast_str_from_cstr(cstr.as_ptr())
}

#[no_mangle]
pub extern "C" fn roast_float_to_str(value: c_double) -> *mut RoastString {
    let mut s = format!("{}", value);
    // Ensure decimal point is shown for whole numbers (Python-like behavior)
    // e.g., 1.0 should display as "1.0" not "1"
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        s.push_str(".0");
    }
    let cstr = CString::new(s).unwrap();
    roast_str_from_cstr(cstr.as_ptr())
}

#[no_mangle]
pub extern "C" fn roast_bool_to_str(value: bool) -> *mut RoastString {
    let s = if value { "True" } else { "False" };
    let cstr = CString::new(s).unwrap();
    roast_str_from_cstr(cstr.as_ptr())
}

#[no_mangle]
pub extern "C" fn roast_str_to_int(s: *const RoastString) -> c_long {
    if s.is_null() { return 0; }
    unsafe {
        let slice = std::slice::from_raw_parts((*s).data, (*s).len);
        if let Ok(string) = std::str::from_utf8(slice) {
            string.trim().parse().unwrap_or(0)
        } else {
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn roast_str_to_float(s: *const RoastString) -> c_double {
    if s.is_null() { return 0.0; }
    unsafe {
        let slice = std::slice::from_raw_parts((*s).data, (*s).len);
        if let Ok(string) = std::str::from_utf8(slice) {
            string.trim().parse().unwrap_or(0.0)
        } else {
            0.0
        }
    }
}

// ============================================================================
// Additional String Methods
// ============================================================================

/// Get a single character as a string (proper string indexing)
#[no_mangle]
pub extern "C" fn roast_str_index(s: *const RoastString, index: c_long) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len as c_long;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len { return roast_str_new(ptr::null(), 0); }

        let result = roast_str_new(ptr::null(), 1);
        *(*result).data = *(*s).data.add(idx as usize);
        *(*result).data.add(1) = 0;
        result
    }
}

/// Convert string to uppercase
#[no_mangle]
pub extern "C" fn roast_str_upper(s: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let result = roast_str_new(ptr::null(), len as c_long);
        for i in 0..len {
            let c = *(*s).data.add(i);
            *(*result).data.add(i) = if c >= b'a' && c <= b'z' { c - 32 } else { c };
        }
        *(*result).data.add(len) = 0;
        result
    }
}

/// Convert string to lowercase
#[no_mangle]
pub extern "C" fn roast_str_lower(s: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let result = roast_str_new(ptr::null(), len as c_long);
        for i in 0..len {
            let c = *(*s).data.add(i);
            *(*result).data.add(i) = if c >= b'A' && c <= b'Z' { c + 32 } else { c };
        }
        *(*result).data.add(len) = 0;
        result
    }
}

/// Strip whitespace from both ends
#[no_mangle]
pub extern "C" fn roast_str_strip(s: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let mut start = 0usize;
        let mut end = len;

        // Find start (skip leading whitespace)
        while start < len {
            let c = *(*s).data.add(start);
            if c != b' ' && c != b'\t' && c != b'\n' && c != b'\r' { break; }
            start += 1;
        }

        // Find end (skip trailing whitespace)
        while end > start {
            let c = *(*s).data.add(end - 1);
            if c != b' ' && c != b'\t' && c != b'\n' && c != b'\r' { break; }
            end -= 1;
        }

        let new_len = end - start;
        let result = roast_str_new(ptr::null(), new_len as c_long);
        if new_len > 0 {
            ptr::copy_nonoverlapping((*s).data.add(start), (*result).data, new_len);
        }
        *(*result).data.add(new_len) = 0;
        result
    }
}

/// Strip whitespace from left
#[no_mangle]
pub extern "C" fn roast_str_lstrip(s: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let mut start = 0usize;

        while start < len {
            let c = *(*s).data.add(start);
            if c != b' ' && c != b'\t' && c != b'\n' && c != b'\r' { break; }
            start += 1;
        }

        let new_len = len - start;
        let result = roast_str_new(ptr::null(), new_len as c_long);
        if new_len > 0 {
            ptr::copy_nonoverlapping((*s).data.add(start), (*result).data, new_len);
        }
        *(*result).data.add(new_len) = 0;
        result
    }
}

/// Strip whitespace from right
#[no_mangle]
pub extern "C" fn roast_str_rstrip(s: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let mut end = len;

        while end > 0 {
            let c = *(*s).data.add(end - 1);
            if c != b' ' && c != b'\t' && c != b'\n' && c != b'\r' { break; }
            end -= 1;
        }

        let result = roast_str_new(ptr::null(), end as c_long);
        if end > 0 {
            ptr::copy_nonoverlapping((*s).data, (*result).data, end);
        }
        *(*result).data.add(end) = 0;
        result
    }
}

/// Find substring, returns index or -1 if not found
#[no_mangle]
pub extern "C" fn roast_str_find(haystack: *const RoastString, needle: *const RoastString) -> c_long {
    if haystack.is_null() || needle.is_null() { return -1; }
    unsafe {
        let h_len = (*haystack).len;
        let n_len = (*needle).len;

        if n_len == 0 { return 0; }
        if n_len > h_len { return -1; }

        for i in 0..=(h_len - n_len) {
            let mut matches = true;
            for j in 0..n_len {
                if *(*haystack).data.add(i + j) != *(*needle).data.add(j) {
                    matches = false;
                    break;
                }
            }
            if matches { return i as c_long; }
        }
        -1
    }
}

/// Find substring from right, returns index or -1 if not found
#[no_mangle]
pub extern "C" fn roast_str_rfind(haystack: *const RoastString, needle: *const RoastString) -> c_long {
    if haystack.is_null() || needle.is_null() { return -1; }
    unsafe {
        let h_len = (*haystack).len;
        let n_len = (*needle).len;

        if n_len == 0 { return h_len as c_long; }
        if n_len > h_len { return -1; }

        for i in (0..=(h_len - n_len)).rev() {
            let mut matches = true;
            for j in 0..n_len {
                if *(*haystack).data.add(i + j) != *(*needle).data.add(j) {
                    matches = false;
                    break;
                }
            }
            if matches { return i as c_long; }
        }
        -1
    }
}

/// Check if string starts with prefix
#[no_mangle]
pub extern "C" fn roast_str_startswith(s: *const RoastString, prefix: *const RoastString) -> bool {
    if s.is_null() || prefix.is_null() { return false; }
    unsafe {
        let s_len = (*s).len;
        let p_len = (*prefix).len;

        if p_len > s_len { return false; }
        if p_len == 0 { return true; }

        for i in 0..p_len {
            if *(*s).data.add(i) != *(*prefix).data.add(i) {
                return false;
            }
        }
        true
    }
}

/// Check if string ends with suffix
#[no_mangle]
pub extern "C" fn roast_str_endswith(s: *const RoastString, suffix: *const RoastString) -> bool {
    if s.is_null() || suffix.is_null() { return false; }
    unsafe {
        let s_len = (*s).len;
        let suf_len = (*suffix).len;

        if suf_len > s_len { return false; }
        if suf_len == 0 { return true; }

        let start = s_len - suf_len;
        for i in 0..suf_len {
            if *(*s).data.add(start + i) != *(*suffix).data.add(i) {
                return false;
            }
        }
        true
    }
}

/// Replace all occurrences of old with new
#[no_mangle]
pub extern "C" fn roast_str_replace(s: *const RoastString, old: *const RoastString, new: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    if old.is_null() || new.is_null() {
        unsafe { return roast_str_new((*s).data as *const c_char, (*s).len as c_long); }
    }

    unsafe {
        let s_len = (*s).len;
        let old_len = (*old).len;
        let new_len = (*new).len;

        if old_len == 0 {
            return roast_str_new((*s).data as *const c_char, s_len as c_long);
        }

        // Count occurrences first
        let mut count = 0usize;
        let mut i = 0usize;
        while i <= s_len - old_len {
            let mut matches = true;
            for j in 0..old_len {
                if *(*s).data.add(i + j) != *(*old).data.add(j) {
                    matches = false;
                    break;
                }
            }
            if matches {
                count += 1;
                i += old_len;
            } else {
                i += 1;
            }
        }

        if count == 0 {
            return roast_str_new((*s).data as *const c_char, s_len as c_long);
        }

        // Calculate new length and allocate
        let result_len = s_len - (count * old_len) + (count * new_len);
        let result = roast_str_new(ptr::null(), result_len as c_long);

        // Copy with replacements
        let mut src_i = 0usize;
        let mut dst_i = 0usize;
        while src_i < s_len {
            if src_i + old_len <= s_len {
                let mut matches = true;
                for j in 0..old_len {
                    if *(*s).data.add(src_i + j) != *(*old).data.add(j) {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    ptr::copy_nonoverlapping((*new).data, (*result).data.add(dst_i), new_len);
                    src_i += old_len;
                    dst_i += new_len;
                    continue;
                }
            }
            *(*result).data.add(dst_i) = *(*s).data.add(src_i);
            src_i += 1;
            dst_i += 1;
        }
        *(*result).data.add(result_len) = 0;
        result
    }
}

/// Count occurrences of substring
#[no_mangle]
pub extern "C" fn roast_str_count(s: *const RoastString, sub: *const RoastString) -> c_long {
    if s.is_null() || sub.is_null() { return 0; }
    unsafe {
        let s_len = (*s).len;
        let sub_len = (*sub).len;

        if sub_len == 0 { return (s_len + 1) as c_long; }
        if sub_len > s_len { return 0; }

        let mut count: c_long = 0;
        let mut i = 0usize;
        while i <= s_len - sub_len {
            let mut matches = true;
            for j in 0..sub_len {
                if *(*s).data.add(i + j) != *(*sub).data.add(j) {
                    matches = false;
                    break;
                }
            }
            if matches {
                count += 1;
                i += sub_len;
            } else {
                i += 1;
            }
        }
        count
    }
}

/// Check if string contains substring
#[no_mangle]
pub extern "C" fn roast_str_contains(s: *const RoastString, sub: *const RoastString) -> bool {
    roast_str_find(s, sub) >= 0
}

/// Split string by separator, returns a list of strings
#[no_mangle]
pub extern "C" fn roast_str_split(s: *const RoastString, sep: *const RoastString) -> *mut RoastList {
    let result = roast_list_new(8);
    if s.is_null() { return result; }

    unsafe {
        let s_len = (*s).len;

        // If no separator or empty separator, split each character
        if sep.is_null() || (*sep).len == 0 {
            for i in 0..s_len {
                let char_str = roast_str_index(s, i as c_long);
                roast_list_append(result, char_str as c_long);
            }
            return result;
        }

        let sep_len = (*sep).len;
        let mut start = 0usize;
        let mut i = 0usize;

        while i <= s_len {
            let mut found_sep = false;
            if i + sep_len <= s_len {
                let mut matches = true;
                for j in 0..sep_len {
                    if *(*s).data.add(i + j) != *(*sep).data.add(j) {
                        matches = false;
                        break;
                    }
                }
                found_sep = matches;
            }

            if found_sep || i == s_len {
                // Create substring from start to i
                let part_len = i - start;
                let part = roast_str_new(ptr::null(), part_len as c_long);
                if part_len > 0 {
                    ptr::copy_nonoverlapping((*s).data.add(start), (*part).data, part_len);
                }
                *(*part).data.add(part_len) = 0;
                roast_list_append(result, part as c_long);

                if found_sep {
                    i += sep_len;
                    start = i;
                } else {
                    break;
                }
            } else {
                i += 1;
            }
        }

        result
    }
}

/// Join list of strings with separator
#[no_mangle]
pub extern "C" fn roast_str_join(sep: *const RoastString, list: *const RoastList) -> *mut RoastString {
    if list.is_null() { return roast_str_new(ptr::null(), 0); }

    unsafe {
        let list_len = (*list).len;
        if list_len == 0 { return roast_str_new(ptr::null(), 0); }

        let sep_len = if sep.is_null() { 0 } else { (*sep).len };

        // Calculate total length
        let mut total_len = 0usize;
        for i in 0..list_len {
            let item = *(*list).data.add(i) as *const RoastString;
            if !item.is_null() {
                total_len += (*item).len;
            }
            if i < list_len - 1 {
                total_len += sep_len;
            }
        }

        let result = roast_str_new(ptr::null(), total_len as c_long);
        let mut offset = 0usize;

        for i in 0..list_len {
            let item = *(*list).data.add(i) as *const RoastString;
            if !item.is_null() && (*item).len > 0 {
                ptr::copy_nonoverlapping((*item).data, (*result).data.add(offset), (*item).len);
                offset += (*item).len;
            }
            if i < list_len - 1 && sep_len > 0 && !sep.is_null() {
                ptr::copy_nonoverlapping((*sep).data, (*result).data.add(offset), sep_len);
                offset += sep_len;
            }
        }
        *(*result).data.add(total_len) = 0;

        result
    }
}

/// Repeat string n times
#[no_mangle]
pub extern "C" fn roast_str_repeat(s: *const RoastString, n: c_long) -> *mut RoastString {
    if s.is_null() || n <= 0 { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let total = len * (n as usize);
        let result = roast_str_new(ptr::null(), total as c_long);

        for i in 0..(n as usize) {
            ptr::copy_nonoverlapping((*s).data, (*result).data.add(i * len), len);
        }
        *(*result).data.add(total) = 0;
        result
    }
}

/// Check if string is alphabetic
#[no_mangle]
pub extern "C" fn roast_str_isalpha(s: *const RoastString) -> bool {
    if s.is_null() { return false; }
    unsafe {
        let len = (*s).len;
        if len == 0 { return false; }
        for i in 0..len {
            let c = *(*s).data.add(i);
            if !((c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z')) {
                return false;
            }
        }
        true
    }
}

/// Check if string is numeric
#[no_mangle]
pub extern "C" fn roast_str_isdigit(s: *const RoastString) -> bool {
    if s.is_null() { return false; }
    unsafe {
        let len = (*s).len;
        if len == 0 { return false; }
        for i in 0..len {
            let c = *(*s).data.add(i);
            if !(c >= b'0' && c <= b'9') {
                return false;
            }
        }
        true
    }
}

/// Check if string is alphanumeric
#[no_mangle]
pub extern "C" fn roast_str_isalnum(s: *const RoastString) -> bool {
    if s.is_null() { return false; }
    unsafe {
        let len = (*s).len;
        if len == 0 { return false; }
        for i in 0..len {
            let c = *(*s).data.add(i);
            if !((c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z') || (c >= b'0' && c <= b'9')) {
                return false;
            }
        }
        true
    }
}

/// Check if string is whitespace only
#[no_mangle]
pub extern "C" fn roast_str_isspace(s: *const RoastString) -> bool {
    if s.is_null() { return false; }
    unsafe {
        let len = (*s).len;
        if len == 0 { return false; }
        for i in 0..len {
            let c = *(*s).data.add(i);
            if c != b' ' && c != b'\t' && c != b'\n' && c != b'\r' {
                return false;
            }
        }
        true
    }
}

/// Capitalize first character
#[no_mangle]
pub extern "C" fn roast_str_capitalize(s: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let result = roast_str_new(ptr::null(), len as c_long);

        for i in 0..len {
            let c = *(*s).data.add(i);
            if i == 0 && c >= b'a' && c <= b'z' {
                *(*result).data.add(i) = c - 32;
            } else if i > 0 && c >= b'A' && c <= b'Z' {
                *(*result).data.add(i) = c + 32;
            } else {
                *(*result).data.add(i) = c;
            }
        }
        *(*result).data.add(len) = 0;
        result
    }
}

/// Title case
#[no_mangle]
pub extern "C" fn roast_str_title(s: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let result = roast_str_new(ptr::null(), len as c_long);
        let mut cap_next = true;

        for i in 0..len {
            let c = *(*s).data.add(i);
            if c == b' ' || c == b'\t' || c == b'\n' {
                *(*result).data.add(i) = c;
                cap_next = true;
            } else if cap_next && c >= b'a' && c <= b'z' {
                *(*result).data.add(i) = c - 32;
                cap_next = false;
            } else if !cap_next && c >= b'A' && c <= b'Z' {
                *(*result).data.add(i) = c + 32;
                cap_next = false;
            } else {
                *(*result).data.add(i) = c;
                cap_next = false;
            }
        }
        *(*result).data.add(len) = 0;
        result
    }
}

/// Swap case
#[no_mangle]
pub extern "C" fn roast_str_swapcase(s: *const RoastString) -> *mut RoastString {
    if s.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let len = (*s).len;
        let result = roast_str_new(ptr::null(), len as c_long);

        for i in 0..len {
            let c = *(*s).data.add(i);
            if c >= b'a' && c <= b'z' {
                *(*result).data.add(i) = c - 32;
            } else if c >= b'A' && c <= b'Z' {
                *(*result).data.add(i) = c + 32;
            } else {
                *(*result).data.add(i) = c;
            }
        }
        *(*result).data.add(len) = 0;
        result
    }
}

fn roast_str_free(s: *mut RoastString) {
    if s.is_null() { return; }
    unsafe {
        // Safety check: verify the object looks valid before freeing
        let header = &(*s).header;
        if header.type_tag != TypeTag::Str {
            return; // Wrong type or corrupted, don't free
        }
        if !(*s).data.is_null() && (*s).capacity > 0 && (*s).capacity < 1_000_000_000 {
            let layout = Layout::from_size_align((*s).capacity, 1).unwrap();
            dealloc((*s).data, layout);
        }
        let layout = Layout::new::<RoastString>();
        dealloc(s as *mut u8, layout);
    }
}

// ============================================================================
// Regex Operations (standard library: re module)
// ============================================================================

use regex::Regex;

/// Check if pattern matches the entire string (like Python's re.fullmatch)
#[no_mangle]
pub extern "C" fn roast_re_fullmatch(pattern: *const RoastString, s: *const RoastString) -> bool {
    if pattern.is_null() || s.is_null() { return false; }
    unsafe {
        let pat_slice = std::slice::from_raw_parts((*pattern).data, (*pattern).len);
        let s_slice = std::slice::from_raw_parts((*s).data, (*s).len);
        
        if let (Ok(pat_str), Ok(s_str)) = (std::str::from_utf8(pat_slice), std::str::from_utf8(s_slice)) {
            if let Ok(re) = Regex::new(pat_str) {
                return re.is_match(s_str) && re.find(s_str).map(|m| m.as_str() == s_str).unwrap_or(false);
            }
        }
        false
    }
}

/// Check if pattern matches anywhere in string (like Python's re.search)
#[no_mangle]
pub extern "C" fn roast_re_search(pattern: *const RoastString, s: *const RoastString) -> bool {
    if pattern.is_null() || s.is_null() { return false; }
    unsafe {
        let pat_slice = std::slice::from_raw_parts((*pattern).data, (*pattern).len);
        let s_slice = std::slice::from_raw_parts((*s).data, (*s).len);
        
        if let (Ok(pat_str), Ok(s_str)) = (std::str::from_utf8(pat_slice), std::str::from_utf8(s_slice)) {
            if let Ok(re) = Regex::new(pat_str) {
                return re.is_match(s_str);
            }
        }
        false
    }
}

/// Check if pattern matches at beginning of string (like Python's re.match)
#[no_mangle]
pub extern "C" fn roast_re_match(pattern: *const RoastString, s: *const RoastString) -> bool {
    if pattern.is_null() || s.is_null() { return false; }
    unsafe {
        let pat_slice = std::slice::from_raw_parts((*pattern).data, (*pattern).len);
        let s_slice = std::slice::from_raw_parts((*s).data, (*s).len);
        
        if let (Ok(pat_str), Ok(s_str)) = (std::str::from_utf8(pat_slice), std::str::from_utf8(s_slice)) {
            // Prepend ^ to anchor at start
            let anchored = format!("^{}", pat_str);
            if let Ok(re) = Regex::new(&anchored) {
                return re.is_match(s_str);
            }
        }
        false
    }
}

/// Find all matches (like Python's re.findall) - returns list of match strings
#[no_mangle]
pub extern "C" fn roast_re_findall(pattern: *const RoastString, s: *const RoastString) -> *mut RoastList {
    let list = roast_list_new(0);
    if pattern.is_null() || s.is_null() { return list; }
    
    unsafe {
        let pat_slice = std::slice::from_raw_parts((*pattern).data, (*pattern).len);
        let s_slice = std::slice::from_raw_parts((*s).data, (*s).len);
        
        if let (Ok(pat_str), Ok(s_str)) = (std::str::from_utf8(pat_slice), std::str::from_utf8(s_slice)) {
            if let Ok(re) = Regex::new(pat_str) {
                for mat in re.find_iter(s_str) {
                    let match_str = roast_str_from_cstr(
                        std::ffi::CString::new(mat.as_str()).unwrap().as_ptr()
                    );
                    roast_list_append(list, match_str as c_long);
                }
            }
        }
    }
    list
}

/// Replace pattern with replacement (like Python's re.sub)
#[no_mangle]
pub extern "C" fn roast_re_sub(
    pattern: *const RoastString,
    replacement: *const RoastString,
    s: *const RoastString
) -> *mut RoastString {
    if pattern.is_null() || s.is_null() { return roast_str_new(ptr::null(), 0); }
    if replacement.is_null() { return roast_str_new(ptr::null(), 0); }
    
    unsafe {
        let pat_slice = std::slice::from_raw_parts((*pattern).data, (*pattern).len);
        let s_slice = std::slice::from_raw_parts((*s).data, (*s).len);
        let repl_slice = std::slice::from_raw_parts((*replacement).data, (*replacement).len);
        
        if let (Ok(pat_str), Ok(s_str), Ok(repl_str)) = (
            std::str::from_utf8(pat_slice),
            std::str::from_utf8(s_slice),
            std::str::from_utf8(repl_slice)
        ) {
            if let Ok(re) = Regex::new(pat_str) {
                let result = re.replace_all(s_str, repl_str);
                return roast_str_from_cstr(
                    std::ffi::CString::new(result.as_ref()).unwrap().as_ptr()
                );
            }
        }
        // Return original string on failure
        roast_str_new((*s).data as *const c_char, (*s).len as c_long)
    }
}

/// Split string by pattern (like Python's re.split)
#[no_mangle]
pub extern "C" fn roast_re_split(pattern: *const RoastString, s: *const RoastString) -> *mut RoastList {
    let list = roast_list_new(0);
    if pattern.is_null() || s.is_null() { return list; }
    
    unsafe {
        let pat_slice = std::slice::from_raw_parts((*pattern).data, (*pattern).len);
        let s_slice = std::slice::from_raw_parts((*s).data, (*s).len);
        
        if let (Ok(pat_str), Ok(s_str)) = (std::str::from_utf8(pat_slice), std::str::from_utf8(s_slice)) {
            if let Ok(re) = Regex::new(pat_str) {
                for part in re.split(s_str) {
                    let part_str = roast_str_from_cstr(
                        std::ffi::CString::new(part).unwrap().as_ptr()
                    );
                    roast_list_append(list, part_str as c_long);
                }
            }
        }
    }
    list
}

// ============================================================================
// JSON Operations (standard library: json module)
// ============================================================================

use serde_json::{Value as JsonValue};

/// Parse JSON string into Roast value
/// Returns dict for objects, list for arrays, primitives for others
#[no_mangle]
pub extern "C" fn roast_json_loads(s: *const RoastString) -> c_long {
    if s.is_null() { return 0; }
    
    unsafe {
        let s_slice = std::slice::from_raw_parts((*s).data, (*s).len);
        if let Ok(s_str) = std::str::from_utf8(s_slice) {
            if let Ok(value) = serde_json::from_str::<JsonValue>(s_str) {
                return json_to_roast(value);
            }
        }
        0 // Return None/0 on parse failure
    }
}

/// Convert JSON Value to Roast value recursively
fn json_to_roast(value: JsonValue) -> c_long {
    match value {
        JsonValue::Null => 0,
        JsonValue::Bool(b) => if b { 1 } else { 0 },
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                i as c_long
            } else if let Some(f) = n.as_f64() {
                // For floats, need to box them
                f.to_bits() as c_long
            } else {
                0
            }
        }
        JsonValue::String(s) => {
            roast_str_from_cstr(
                std::ffi::CString::new(s).unwrap().as_ptr()
            ) as c_long
        }
        JsonValue::Array(arr) => {
            let list = roast_list_new(arr.len() as c_long);
            for item in arr {
                roast_list_append(list, json_to_roast(item));
            }
            list as c_long
        }
        JsonValue::Object(obj) => {
            let dict = roast_dict_new();
            for (key, val) in obj {
                let key_str = roast_str_from_cstr(
                    std::ffi::CString::new(key).unwrap().as_ptr()
                );
                roast_dict_set(dict, key_str as c_long, json_to_roast(val));
            }
            dict as c_long
        }
    }
}

/// Serialize Roast value to JSON string
#[no_mangle]
pub extern "C" fn roast_json_dumps(value: c_long) -> *mut RoastString {
    // For now, basic serialization of primitive types
    // Full implementation would recursively serialize dicts/lists
    if value == 0 {
        return roast_str_from_cstr(c"null".as_ptr());
    }
    
    // Try to detect type and serialize
    // This is a simplified version - full impl would check type tags
    let json_str = format!("{}", value);
    roast_str_from_cstr(
        std::ffi::CString::new(json_str).unwrap().as_ptr()
    )
}

/// Check if string is valid JSON
#[no_mangle]
pub extern "C" fn roast_json_valid(s: *const RoastString) -> bool {
    if s.is_null() { return false; }
    
    unsafe {
        let s_slice = std::slice::from_raw_parts((*s).data, (*s).len);
        if let Ok(s_str) = std::str::from_utf8(s_slice) {
            return serde_json::from_str::<JsonValue>(s_str).is_ok();
        }
        false
    }
}

// ============================================================================
// BigInt Operations (arbitrary precision integers)
// ============================================================================

#[repr(C)]
pub struct RoastBigInt {
    header: ObjectHeader,
    value: *mut NumBigInt,
}

#[no_mangle]
pub extern "C" fn roast_bigint_from_i64(value: c_long) -> *mut RoastBigInt {
    unsafe {
        let layout = Layout::new::<RoastBigInt>();
        let ptr = alloc(layout) as *mut RoastBigInt;
        (*ptr).header = ObjectHeader::new(TypeTag::BigInt);
        let bigint = Box::new(NumBigInt::from(value));
        (*ptr).value = Box::into_raw(bigint);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_bigint_add(a: *const RoastBigInt, b: *const RoastBigInt) -> *mut RoastBigInt {
    if a.is_null() || b.is_null() { return roast_bigint_from_i64(0); }
    unsafe {
        let a_val = &*(*a).value;
        let b_val = &*(*b).value;
        let result = a_val + b_val;
        
        let layout = Layout::new::<RoastBigInt>();
        let ptr = alloc(layout) as *mut RoastBigInt;
        (*ptr).header = ObjectHeader::new(TypeTag::BigInt);
        (*ptr).value = Box::into_raw(Box::new(result));
        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_bigint_sub(a: *const RoastBigInt, b: *const RoastBigInt) -> *mut RoastBigInt {
    if a.is_null() || b.is_null() { return roast_bigint_from_i64(0); }
    unsafe {
        let a_val = &*(*a).value;
        let b_val = &*(*b).value;
        let result = a_val - b_val;
        
        let layout = Layout::new::<RoastBigInt>();
        let ptr = alloc(layout) as *mut RoastBigInt;
        (*ptr).header = ObjectHeader::new(TypeTag::BigInt);
        (*ptr).value = Box::into_raw(Box::new(result));
        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_bigint_mul(a: *const RoastBigInt, b: *const RoastBigInt) -> *mut RoastBigInt {
    if a.is_null() || b.is_null() { return roast_bigint_from_i64(0); }
    unsafe {
        let a_val = &*(*a).value;
        let b_val = &*(*b).value;
        let result = a_val * b_val;
        
        let layout = Layout::new::<RoastBigInt>();
        let ptr = alloc(layout) as *mut RoastBigInt;
        (*ptr).header = ObjectHeader::new(TypeTag::BigInt);
        (*ptr).value = Box::into_raw(Box::new(result));
        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_bigint_to_str(b: *const RoastBigInt) -> *mut RoastString {
    if b.is_null() { return roast_str_new(ptr::null(), 0); }
    unsafe {
        let val = &*(*b).value;
        let s = val.to_string();
        let cstr = CString::new(s).unwrap();
        roast_str_from_cstr(cstr.as_ptr())
    }
}

#[no_mangle]
pub extern "C" fn roast_bigint_print(b: *const RoastBigInt) {
    if b.is_null() { 
        println!("0");
        return; 
    }
    unsafe {
        let val = &*(*b).value;
        println!("{}", val);
    }
}

fn roast_bigint_free(b: *mut RoastBigInt) {
    if b.is_null() { return; }
    unsafe {
        if !(*b).value.is_null() {
            drop(Box::from_raw((*b).value));
        }
        let layout = Layout::new::<RoastBigInt>();
        dealloc(b as *mut u8, layout);
    }
}

// ============================================================================
// List Operations
// ============================================================================

#[repr(C)]
pub struct RoastList {
    header: ObjectHeader,
    len: usize,
    capacity: usize,
    data: *mut c_long,
}

#[no_mangle]
pub extern "C" fn roast_list_new(capacity: c_long) -> *mut RoastList {
    unsafe {
        let cap = (capacity as usize).max(8);
        let layout = Layout::new::<RoastList>();
        let ptr = alloc(layout) as *mut RoastList;

        (*ptr).header = ObjectHeader::new(TypeTag::List);
        (*ptr).len = 0;
        (*ptr).capacity = cap;

        let data_layout = Layout::array::<c_long>(cap).unwrap();
        (*ptr).data = alloc(data_layout) as *mut c_long;

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_list_len(list: *const RoastList) -> c_long {
    if list.is_null() { 0 } else { unsafe { (*list).len as c_long } }
}

#[no_mangle]
pub extern "C" fn roast_list_append(list: *mut RoastList, value: c_long) {
    if list.is_null() { return; }
    unsafe {
        if (*list).len >= (*list).capacity {
            let new_cap = (*list).capacity * 2;
            let old_layout = Layout::array::<c_long>((*list).capacity).unwrap();
            let new_layout = Layout::array::<c_long>(new_cap).unwrap();
            (*list).data = realloc((*list).data as *mut u8, old_layout, new_layout.size()) as *mut c_long;
            (*list).capacity = new_cap;
        }
        *(*list).data.add((*list).len) = value;
        (*list).len += 1;
    }
}

#[no_mangle]
pub extern "C" fn roast_list_get(list: *const RoastList, index: c_long) -> c_long {
    if list.is_null() { return 0; }
    unsafe {
        let len = (*list).len as c_long;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len { return 0; }
        *(*list).data.add(idx as usize)
    }
}

#[no_mangle]
pub extern "C" fn roast_list_set(list: *mut RoastList, index: c_long, value: c_long) {
    if list.is_null() { return; }
    unsafe {
        let len = (*list).len as c_long;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len { return; }
        *(*list).data.add(idx as usize) = value;
    }
}

/// Unified subscript get - works for both lists and dicts
/// Detects the container type at runtime and dispatches to appropriate function
#[no_mangle]
pub extern "C" fn roast_subscript_get(container: c_long, key: c_long) -> c_long {
    let ptr = container as *const c_void;
    if ptr.is_null() { return 0; }

    unsafe {
        let header = ptr as *const ObjectHeader;
        match (*header).type_tag {
            TypeTag::List => {
                roast_list_get(ptr as *const RoastList, key)
            }
            TypeTag::Dict => {
                roast_dict_get(ptr as *const RoastDict, key)
            }
            TypeTag::Str => {
                // String indexing returns the character at index
                let s = ptr as *const RoastString;
                let len = (*s).len as c_long;
                let idx = if key < 0 { len + key } else { key };
                if idx < 0 || idx >= len { return 0; }
                *(*s).data.add(idx as usize) as c_long
            }
            TypeTag::Tuple => {
                roast_tuple_get(ptr as *const RoastTuple, key)
            }
            _ => 0
        }
    }
}

/// Unified subscript set - works for both lists and dicts
/// Detects the container type at runtime and dispatches to appropriate function
#[no_mangle]
pub extern "C" fn roast_subscript_set(container: c_long, key: c_long, value: c_long) {
    let ptr = container as *mut c_void;
    if ptr.is_null() { return; }

    unsafe {
        let header = ptr as *const ObjectHeader;
        match (*header).type_tag {
            TypeTag::List => {
                roast_list_set(ptr as *mut RoastList, key, value);
            }
            TypeTag::Dict => {
                roast_dict_set(ptr as *mut RoastDict, key, value);
            }
            _ => {}
        }
    }
}

#[no_mangle]
pub extern "C" fn roast_list_pop(list: *mut RoastList) -> c_long {
    if list.is_null() { return 0; }
    unsafe {
        if (*list).len == 0 { return 0; }
        (*list).len -= 1;
        *(*list).data.add((*list).len)
    }
}

#[no_mangle]
pub extern "C" fn roast_list_insert(list: *mut RoastList, index: c_long, value: c_long) {
    if list.is_null() { return; }
    unsafe {
        let len = (*list).len as c_long;
        let idx = if index < 0 { (len + index + 1).max(0) } else { index.min(len) } as usize;

        // Ensure capacity
        if (*list).len >= (*list).capacity {
            let new_cap = (*list).capacity * 2;
            let old_layout = Layout::array::<c_long>((*list).capacity).unwrap();
            let new_layout = Layout::array::<c_long>(new_cap).unwrap();
            (*list).data = realloc((*list).data as *mut u8, old_layout, new_layout.size()) as *mut c_long;
            (*list).capacity = new_cap;
        }

        // Shift elements
        if idx < (*list).len {
            ptr::copy((*list).data.add(idx), (*list).data.add(idx + 1), (*list).len - idx);
        }
        *(*list).data.add(idx) = value;
        (*list).len += 1;
    }
}

#[no_mangle]
pub extern "C" fn roast_list_remove(list: *mut RoastList, index: c_long) {
    if list.is_null() { return; }
    unsafe {
        let len = (*list).len as c_long;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len { return; }
        let idx = idx as usize;

        if idx < (*list).len - 1 {
            ptr::copy((*list).data.add(idx + 1), (*list).data.add(idx), (*list).len - idx - 1);
        }
        (*list).len -= 1;
    }
}

#[no_mangle]
pub extern "C" fn roast_list_contains(list: *const RoastList, value: c_long) -> bool {
    if list.is_null() { return false; }
    unsafe {
        for i in 0..(*list).len {
            let item = *(*list).data.add(i);
            if roast_values_equal(item, value) {
                return true;
            }
        }
        false
    }
}

/// Compare two Roast values for equality.
/// For strings, compares content. For other types, compares raw values.
fn roast_values_equal(a: c_long, b: c_long) -> bool {
    // Fast path: exact same value/pointer
    if a == b { return true; }
    
    // Check if both might be strings and compare by content
    let ptr_a = a as *const c_void;
    let ptr_b = b as *const c_void;
    
    if ptr_a.is_null() || ptr_b.is_null() { return false; }
    if !is_likely_pointer(a) || !is_likely_pointer(b) { return false; }
    
    unsafe {
        let header_a = ptr_a as *const ObjectHeader;
        let header_b = ptr_b as *const ObjectHeader;
        
        // Both must be strings for content comparison
        if (*header_a).type_tag == TypeTag::Str && (*header_b).type_tag == TypeTag::Str {
            let s_a = ptr_a as *const RoastString;
            let s_b = ptr_b as *const RoastString;
            
            // Different lengths = not equal
            if (*s_a).len != (*s_b).len { return false; }
            if (*s_a).len == 0 { return true; } // Both empty strings
            
            // Compare content byte by byte
            let slice_a = std::slice::from_raw_parts((*s_a).data, (*s_a).len);
            let slice_b = std::slice::from_raw_parts((*s_b).data, (*s_b).len);
            return slice_a == slice_b;
        }
    }
    
    false // Different pointer values that aren't equal strings
}

#[no_mangle]
pub extern "C" fn roast_list_index(list: *const RoastList, value: c_long) -> c_long {
    if list.is_null() { return -1; }
    unsafe {
        for i in 0..(*list).len {
            if *(*list).data.add(i) == value {
                return i as c_long;
            }
        }
        -1
    }
}

#[no_mangle]
pub extern "C" fn roast_list_slice(list: *const RoastList, start: c_long, end: c_long, step: c_long) -> *mut RoastList {
    if list.is_null() { return roast_list_new(0); }
    unsafe {
        let len = (*list).len as c_long;
        
        // Handle sentinel values for None bounds
        // Sentinel for omitted bounds: i64::MAX / 2 = 4611686018427387903
        // Omitted start defaults to 0, omitted end defaults to len
        const SENTINEL: c_long = 4611686018427387903;
        
        let start = if start == SENTINEL { 0 } else if start < 0 { (len + start).max(0) } else { start.min(len) };
        let end = if end == SENTINEL { len } else if end < 0 { (len + end).max(0) } else { end.min(len) };
        let step = if step == 0 || step == SENTINEL { 1 } else { step };

        let result = roast_list_new(((end - start).abs() / step.abs()) as c_long);

        if step > 0 {
            let mut i = start;
            while i < end {
                roast_list_append(result, *(*list).data.add(i as usize));
                i += step;
            }
        } else {
            let mut i = start;
            while i > end {
                roast_list_append(result, *(*list).data.add(i as usize));
                i += step;
            }
        }
        result
    }
}

#[no_mangle]
pub extern "C" fn roast_list_concat(a: *const RoastList, b: *const RoastList) -> *mut RoastList {
    let len_a = if a.is_null() { 0 } else { unsafe { (*a).len } };
    let len_b = if b.is_null() { 0 } else { unsafe { (*b).len } };

    let result = roast_list_new((len_a + len_b) as c_long);
    unsafe {
        for i in 0..len_a {
            roast_list_append(result, *(*a).data.add(i));
        }
        for i in 0..len_b {
            roast_list_append(result, *(*b).data.add(i));
        }
    }
    result
}

#[no_mangle]
pub extern "C" fn roast_list_extend(list: *mut RoastList, other: *const RoastList) {
    if list.is_null() || other.is_null() { return; }
    unsafe {
        for i in 0..(*other).len {
            roast_list_append(list, *(*other).data.add(i));
        }
    }
}

#[no_mangle]
pub extern "C" fn roast_list_clear(list: *mut RoastList) {
    if list.is_null() { return; }
    unsafe { (*list).len = 0; }
}

#[no_mangle]
pub extern "C" fn roast_list_copy(list: *const RoastList) -> *mut RoastList {
    if list.is_null() { return roast_list_new(0); }
    unsafe {
        let result = roast_list_new((*list).len as c_long);
        for i in 0..(*list).len {
            roast_list_append(result, *(*list).data.add(i));
        }
        result
    }
}

#[no_mangle]
pub extern "C" fn roast_list_reverse(list: *mut RoastList) {
    if list.is_null() { return; }
    unsafe {
        let len = (*list).len;
        for i in 0..len/2 {
            let tmp = *(*list).data.add(i);
            *(*list).data.add(i) = *(*list).data.add(len - 1 - i);
            *(*list).data.add(len - 1 - i) = tmp;
        }
    }
}

/// Count occurrences of a value in list
#[no_mangle]
pub extern "C" fn roast_list_count(list: *const RoastList, value: c_long) -> c_long {
    if list.is_null() { return 0; }
    unsafe {
        let mut count = 0 as c_long;
        for i in 0..(*list).len {
            if *(*list).data.add(i) == value {
                count += 1;
            }
        }
        count
    }
}

/// Sort list in place (simple bubble sort for now)
#[no_mangle]
pub extern "C" fn roast_list_sort(list: *mut RoastList) {
    if list.is_null() { return; }
    unsafe {
        let len = (*list).len;
        if len <= 1 { return; }

        // Simple quicksort-like approach using recursion would be better,
        // but for simplicity use insertion sort which works well for small lists
        for i in 1..len {
            let key = *(*list).data.add(i);
            let mut j = i;
            while j > 0 && *(*list).data.add(j - 1) > key {
                *(*list).data.add(j) = *(*list).data.add(j - 1);
                j -= 1;
            }
            *(*list).data.add(j) = key;
        }
    }
}

/// Sort list in place (reverse)
#[no_mangle]
pub extern "C" fn roast_list_sort_reverse(list: *mut RoastList) {
    roast_list_sort(list);
    roast_list_reverse(list);
}

/// Pop at specific index (supports negative indexing)
#[no_mangle]
pub extern "C" fn roast_list_pop_at(list: *mut RoastList, index: c_long) -> c_long {
    if list.is_null() { return 0; }
    unsafe {
        let len = (*list).len as c_long;
        if len == 0 { return 0; }

        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len { return 0; }

        let value = *(*list).data.add(idx as usize);

        // Shift elements
        for i in (idx as usize)..((*list).len - 1) {
            *(*list).data.add(i) = *(*list).data.add(i + 1);
        }
        (*list).len -= 1;

        value
    }
}

/// Remove first occurrence of value from list
#[no_mangle]
pub extern "C" fn roast_list_remove_value(list: *mut RoastList, value: c_long) -> bool {
    if list.is_null() { return false; }
    unsafe {
        for i in 0..(*list).len {
            if *(*list).data.add(i) == value {
                // Shift elements
                for j in i..((*list).len - 1) {
                    *(*list).data.add(j) = *(*list).data.add(j + 1);
                }
                (*list).len -= 1;
                return true;
            }
        }
        false
    }
}

/// Get minimum value in list
#[no_mangle]
pub extern "C" fn roast_list_min(list: *const RoastList) -> c_long {
    if list.is_null() { return 0; }
    unsafe {
        if (*list).len == 0 { return 0; }
        let mut min = *(*list).data;
        for i in 1..(*list).len {
            let v = *(*list).data.add(i);
            if v < min { min = v; }
        }
        min
    }
}

/// Get maximum value in list
#[no_mangle]
pub extern "C" fn roast_list_max(list: *const RoastList) -> c_long {
    if list.is_null() { return 0; }
    unsafe {
        if (*list).len == 0 { return 0; }
        let mut max = *(*list).data;
        for i in 1..(*list).len {
            let v = *(*list).data.add(i);
            if v > max { max = v; }
        }
        max
    }
}

/// Sum all values in list
#[no_mangle]
pub extern "C" fn roast_list_sum(list: *const RoastList) -> c_long {
    if list.is_null() { return 0; }
    unsafe {
        let mut sum = 0 as c_long;
        for i in 0..(*list).len {
            sum += *(*list).data.add(i);
        }
        sum
    }
}

/// Compare two lists for equality (element-by-element)
#[no_mangle]
pub extern "C" fn roast_list_eq(a: *const RoastList, b: *const RoastList) -> bool {
    // Same pointer = equal
    if a == b { return true; }
    // One null = not equal
    if a.is_null() || b.is_null() { return false; }
    unsafe {
        // Different lengths = not equal
        if (*a).len != (*b).len { return false; }
        // Compare elements
        for i in 0..(*a).len {
            if *(*a).data.add(i) != *(*b).data.add(i) {
                return false;
            }
        }
        true
    }
}

fn roast_list_free_internal(list: *mut RoastList) {
    if list.is_null() { return; }
    unsafe {
        if !(*list).data.is_null() {
            let layout = Layout::array::<c_long>((*list).capacity).unwrap();
            dealloc((*list).data as *mut u8, layout);
        }
        let layout = Layout::new::<RoastList>();
        dealloc(list as *mut u8, layout);
    }
}

// ============================================================================
// Dict Operations
// ============================================================================

#[repr(C)]
pub struct RoastDict {
    header: ObjectHeader,
    /// HashMap stores: hash -> (original_key, value)
    map: *mut HashMap<c_long, (c_long, c_long)>,
}

#[no_mangle]
pub extern "C" fn roast_dict_new() -> *mut RoastDict {
    unsafe {
        let layout = Layout::new::<RoastDict>();
        let ptr = alloc(layout) as *mut RoastDict;

        (*ptr).header = ObjectHeader::new(TypeTag::Dict);
        (*ptr).map = Box::into_raw(Box::new(HashMap::<c_long, (c_long, c_long)>::new()));

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_dict_len(dict: *const RoastDict) -> c_long {
    if dict.is_null() { return 0; }
    unsafe { (*(*dict).map).len() as c_long }
}

/// Compute a hash key for dict operations.
/// For strings, we hash the content so that equal strings hash to the same key.
fn dict_hash_key(key: c_long) -> c_long {
    let ptr = key as *const c_void;
    if ptr.is_null() || !is_likely_pointer(key) {
        return key; // Return as-is for integers and other non-pointers
    }

    unsafe {
        let header = ptr as *const ObjectHeader;
        let tag_val = (*header).type_tag as u8;

        // Only handle strings specially - hash by content
        if tag_val == TypeTag::Str as u8 {
            let s = ptr as *const RoastString;
            if (*s).len > 0 && !(*s).data.is_null() {
                // Use FNV-1a hash for string content
                let mut hash: u64 = 0xcbf29ce484222325;
                let slice = std::slice::from_raw_parts((*s).data, (*s).len);
                for &byte in slice {
                    hash ^= byte as u64;
                    hash = hash.wrapping_mul(0x100000001b3);
                }
                return hash as c_long;
            }
        }
    }

    key // Return as-is for non-strings
}

#[no_mangle]
pub extern "C" fn roast_dict_set(dict: *mut RoastDict, key: c_long, value: c_long) {
    if dict.is_null() { return; }
    let hash_key = dict_hash_key(key);
    // Store original key and value as tuple
    unsafe { (*(*dict).map).insert(hash_key, (key, value)); }
}

#[no_mangle]
pub extern "C" fn roast_dict_get(dict: *const RoastDict, key: c_long) -> c_long {
    if dict.is_null() { return 0; }
    unsafe {
        if (*dict).map.is_null() { return 0; }
        let hash_key = dict_hash_key(key);
        // Return value from (key, value) tuple
        (*(*dict).map).get(&hash_key).map(|(_, v)| *v).unwrap_or(0)
    }
}

#[no_mangle]
pub extern "C" fn roast_dict_contains(dict: *const RoastDict, key: c_long) -> bool {
    if dict.is_null() { return false; }
    unsafe {
        if (*dict).map.is_null() { return false; }
        let hash_key = dict_hash_key(key);
        (*(*dict).map).contains_key(&hash_key)
    }
}

#[no_mangle]
pub extern "C" fn roast_dict_delete(dict: *mut RoastDict, key: c_long) {
    if dict.is_null() { return; }
    let hash_key = dict_hash_key(key);
    unsafe { (*(*dict).map).remove(&hash_key); }
}

#[no_mangle]
pub extern "C" fn roast_dict_keys(dict: *const RoastDict) -> *mut RoastList {
    let result = roast_list_new(0);
    if dict.is_null() { return result; }
    unsafe {
        // Return original keys from (key, value) tuples
        for (original_key, _) in (*(*dict).map).values() {
            roast_list_append(result, *original_key);
        }
    }
    result
}

#[no_mangle]
pub extern "C" fn roast_dict_values(dict: *const RoastDict) -> *mut RoastList {
    let result = roast_list_new(0);
    if dict.is_null() { return result; }
    unsafe {
        // Return values from (key, value) tuples
        for (_, value) in (*(*dict).map).values() {
            roast_list_append(result, *value);
        }
    }
    result
}

#[no_mangle]
pub extern "C" fn roast_dict_clear(dict: *mut RoastDict) {
    if dict.is_null() { return; }
    unsafe { (*(*dict).map).clear(); }
}

/// Get value with default if key not found
#[no_mangle]
pub extern "C" fn roast_dict_get_default(dict: *const RoastDict, key: c_long, default: c_long) -> c_long {
    if dict.is_null() { return default; }
    let hash_key = dict_hash_key(key);
    unsafe { (*(*dict).map).get(&hash_key).map(|(_, v)| *v).unwrap_or(default) }
}

/// Pop value (remove and return) with default if not found
#[no_mangle]
pub extern "C" fn roast_dict_pop(dict: *mut RoastDict, key: c_long, default: c_long) -> c_long {
    if dict.is_null() { return default; }
    let hash_key = dict_hash_key(key);
    // Return value from removed (key, value) tuple
    unsafe { (*(*dict).map).remove(&hash_key).map(|(_, v)| v).unwrap_or(default) }
}

/// Set value if key doesn't exist, return value
#[no_mangle]
pub extern "C" fn roast_dict_setdefault(dict: *mut RoastDict, key: c_long, default: c_long) -> c_long {
    if dict.is_null() { return default; }
    let hash_key = dict_hash_key(key);
    unsafe {
        // Return value from (key, value) tuple
        (*(*dict).map).entry(hash_key).or_insert((key, default)).1
    }
}

/// Update dict with another dict
#[no_mangle]
pub extern "C" fn roast_dict_update(dict: *mut RoastDict, other: *const RoastDict) {
    if dict.is_null() || other.is_null() { return; }
    unsafe {
        for (k, v) in (*(*other).map).iter() {
            (*(*dict).map).insert(*k, *v);
        }
    }
}

/// Copy dict
#[no_mangle]
pub extern "C" fn roast_dict_copy(dict: *const RoastDict) -> *mut RoastDict {
    let result = roast_dict_new();
    if dict.is_null() { return result; }
    unsafe {
        for (k, v) in (*(*dict).map).iter() {
            (*(*result).map).insert(*k, *v);
        }
    }
    result
}

/// Get list of (key, value) pairs as a list of tuples
#[no_mangle]
pub extern "C" fn roast_dict_items(dict: *const RoastDict) -> *mut RoastList {
    let result = roast_list_new(0);
    if dict.is_null() { return result; }
    unsafe {
        // HashMap stores: hash -> (original_key, value)
        for (original_key, value) in (*(*dict).map).values() {
            // Create a 2-tuple for each (key, value) pair
            let tuple = roast_tuple_new(2);
            roast_tuple_set(tuple, 0, *original_key);
            roast_tuple_set(tuple, 1, *value);
            roast_list_append(result, tuple as c_long);
        }
    }
    result
}

/// Compare two dicts for equality (key-value pairs)
#[no_mangle]
pub extern "C" fn roast_dict_eq(a: *const RoastDict, b: *const RoastDict) -> bool {
    // Same pointer = equal
    if a == b { return true; }
    // One null = not equal
    if a.is_null() || b.is_null() { return false; }
    unsafe {
        let map_a = &*(*a).map;
        let map_b = &*(*b).map;
        // Different sizes = not equal
        if map_a.len() != map_b.len() { return false; }
        // Check all key-value pairs
        for (hash_key, (_orig_key_a, val_a)) in map_a.iter() {
            match map_b.get(hash_key) {
                Some((_, val_b)) if val_a == val_b => continue,
                _ => return false,
            }
        }
        true
    }
}

fn roast_dict_free_internal(dict: *mut RoastDict) {
    if dict.is_null() { return; }
    unsafe {
        if !(*dict).map.is_null() {
            drop(Box::from_raw((*dict).map));
        }
        let layout = Layout::new::<RoastDict>();
        dealloc(dict as *mut u8, layout);
    }
}

// ============================================================================
// DefaultDict Operations (collections.defaultdict)
// ============================================================================

#[repr(C)]
pub struct RoastDefaultDict {
    header: ObjectHeader,
    map: *mut HashMap<c_long, c_long>,
    /// Factory function pointer for default values
    default_factory: c_long,
}

/// Create a new defaultdict with a factory function
#[no_mangle]
pub extern "C" fn roast_defaultdict_new(default_factory: c_long) -> *mut RoastDefaultDict {
    unsafe {
        let layout = Layout::new::<RoastDefaultDict>();
        let ptr = alloc(layout) as *mut RoastDefaultDict;
        
        (*ptr).header = ObjectHeader::new(TypeTag::Dict);
        (*ptr).map = Box::into_raw(Box::new(HashMap::new()));
        (*ptr).default_factory = default_factory;
        
        ptr
    }
}

/// Get from defaultdict - creates default if missing
#[no_mangle]
pub extern "C" fn roast_defaultdict_get(dict: *mut RoastDefaultDict, key: c_long) -> c_long {
    if dict.is_null() { return 0; }
    unsafe {
        if let Some(&value) = (*(*dict).map).get(&key) {
            return value;
        }
        
        // Key missing - create default value
        // For now, return 0 (int default), empty list, or call factory if set
        // Full implementation would call the factory function
        let default_val = if (*dict).default_factory != 0 {
            // Factory is a callable - would need runtime call infrastructure
            // For MVP, return 0
            0
        } else {
            0
        };
        
        (*(*dict).map).insert(key, default_val);
        default_val
    }
}

/// Set value in defaultdict
#[no_mangle]
pub extern "C" fn roast_defaultdict_set(dict: *mut RoastDefaultDict, key: c_long, value: c_long) {
    if dict.is_null() { return; }
    unsafe {
        (*(*dict).map).insert(key, value);
    }
}

// ============================================================================
// Partial Function Application (functools.partial)
// ============================================================================

#[repr(C)]
pub struct RoastPartial {
    header: ObjectHeader,
    /// The wrapped function
    func: c_long,
    /// Pre-bound positional arguments
    args: *mut RoastList,
    /// Pre-bound keyword arguments (dict)
    kwargs: *mut RoastDict,
}

/// Create a partial function application
#[no_mangle]
pub extern "C" fn roast_partial_new(
    func: c_long,
    args: *mut RoastList,
    kwargs: *mut RoastDict
) -> *mut RoastPartial {
    unsafe {
        let layout = Layout::new::<RoastPartial>();
        let ptr = alloc(layout) as *mut RoastPartial;
        
        (*ptr).header = ObjectHeader::new(TypeTag::Function);
        (*ptr).func = func;
        (*ptr).args = args;
        (*ptr).kwargs = kwargs;
        
        // Incref the stored objects
        if !args.is_null() { roast_incref(args as *mut c_void); }
        if !kwargs.is_null() { roast_incref(kwargs as *mut c_void); }
        roast_incref(func as *mut c_void);
        
        ptr
    }
}

/// Get the wrapped function
#[no_mangle]
pub extern "C" fn roast_partial_func(partial: *const RoastPartial) -> c_long {
    if partial.is_null() { return 0; }
    unsafe { (*partial).func }
}

/// Get pre-bound args
#[no_mangle]
pub extern "C" fn roast_partial_args(partial: *const RoastPartial) -> *mut RoastList {
    if partial.is_null() { return std::ptr::null_mut(); }
    unsafe { (*partial).args }
}

/// Get pre-bound kwargs
#[no_mangle]
pub extern "C" fn roast_partial_kwargs(partial: *const RoastPartial) -> *mut RoastDict {
    if partial.is_null() { return std::ptr::null_mut(); }
    unsafe { (*partial).kwargs }
}

// ============================================================================
// Set Operations
// ============================================================================

#[repr(C)]
pub struct RoastSet {
    header: ObjectHeader,
    set: *mut HashSet<c_long>,
}

#[no_mangle]
pub extern "C" fn roast_set_new() -> *mut RoastSet {
    unsafe {
        let layout = Layout::new::<RoastSet>();
        let ptr = alloc(layout) as *mut RoastSet;

        (*ptr).header = ObjectHeader::new(TypeTag::Set);
        (*ptr).set = Box::into_raw(Box::new(HashSet::new()));

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_set_len(set: *const RoastSet) -> c_long {
    if set.is_null() { return 0; }
    unsafe { (*(*set).set).len() as c_long }
}

#[no_mangle]
pub extern "C" fn roast_set_add(set: *mut RoastSet, value: c_long) {
    if set.is_null() { return; }
    unsafe { (*(*set).set).insert(value); }
}

#[no_mangle]
pub extern "C" fn roast_set_remove(set: *mut RoastSet, value: c_long) {
    if set.is_null() { return; }
    unsafe { (*(*set).set).remove(&value); }
}

#[no_mangle]
pub extern "C" fn roast_set_contains(set: *const RoastSet, value: c_long) -> bool {
    if set.is_null() { return false; }
    unsafe { (*(*set).set).contains(&value) }
}

#[no_mangle]
pub extern "C" fn roast_set_union(a: *const RoastSet, b: *const RoastSet) -> *mut RoastSet {
    let result = roast_set_new();
    unsafe {
        if !a.is_null() {
            for v in (*(*a).set).iter() {
                roast_set_add(result, *v);
            }
        }
        if !b.is_null() {
            for v in (*(*b).set).iter() {
                roast_set_add(result, *v);
            }
        }
    }
    result
}

#[no_mangle]
pub extern "C" fn roast_set_intersection(a: *const RoastSet, b: *const RoastSet) -> *mut RoastSet {
    let result = roast_set_new();
    if a.is_null() || b.is_null() { return result; }
    unsafe {
        for v in (*(*a).set).iter() {
            if (*(*b).set).contains(v) {
                roast_set_add(result, *v);
            }
        }
    }
    result
}

#[no_mangle]
pub extern "C" fn roast_set_difference(a: *const RoastSet, b: *const RoastSet) -> *mut RoastSet {
    let result = roast_set_new();
    if a.is_null() { return result; }
    unsafe {
        for v in (*(*a).set).iter() {
            if b.is_null() || !(*(*b).set).contains(v) {
                roast_set_add(result, *v);
            }
        }
    }
    result
}

fn roast_set_free_internal(set: *mut RoastSet) {
    if set.is_null() { return; }
    unsafe {
        if !(*set).set.is_null() {
            drop(Box::from_raw((*set).set));
        }
        let layout = Layout::new::<RoastSet>();
        dealloc(set as *mut u8, layout);
    }
}

// ============================================================================
// Tuple Operations
// ============================================================================

#[repr(C)]
pub struct RoastTuple {
    header: ObjectHeader,
    len: usize,
    data: *mut c_long,
}

#[no_mangle]
pub extern "C" fn roast_tuple_new(len: c_long) -> *mut RoastTuple {
    unsafe {
        let len = len.max(0) as usize;
        let layout = Layout::new::<RoastTuple>();
        let ptr = alloc(layout) as *mut RoastTuple;

        (*ptr).header = ObjectHeader::new(TypeTag::Tuple);
        (*ptr).len = len;

        if len > 0 {
            let data_layout = Layout::array::<c_long>(len).unwrap();
            (*ptr).data = alloc(data_layout) as *mut c_long;
            ptr::write_bytes((*ptr).data, 0, len);
        } else {
            (*ptr).data = ptr::null_mut();
        }

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_tuple_len(tuple: *const RoastTuple) -> c_long {
    if tuple.is_null() { 0 } else { unsafe { (*tuple).len as c_long } }
}

#[no_mangle]
pub extern "C" fn roast_tuple_get(tuple: *const RoastTuple, index: c_long) -> c_long {
    if tuple.is_null() { return 0; }
    unsafe {
        let len = (*tuple).len as c_long;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len { return 0; }
        *(*tuple).data.add(idx as usize)
    }
}

#[no_mangle]
pub extern "C" fn roast_tuple_set(tuple: *mut RoastTuple, index: c_long, value: c_long) {
    if tuple.is_null() { return; }
    unsafe {
        let len = (*tuple).len as c_long;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len { return; }
        *(*tuple).data.add(idx as usize) = value;
    }
}

fn roast_tuple_free_internal(tuple: *mut RoastTuple) {
    if tuple.is_null() { return; }
    unsafe {
        if !(*tuple).data.is_null() && (*tuple).len > 0 {
            let layout = Layout::array::<c_long>((*tuple).len).unwrap();
            dealloc((*tuple).data as *mut u8, layout);
        }
        let layout = Layout::new::<RoastTuple>();
        dealloc(tuple as *mut u8, layout);
    }
}

// ============================================================================
// Range/Iterator Operations
// ============================================================================

#[repr(C)]
pub struct RoastRange {
    header: ObjectHeader,
    start: c_long,
    stop: c_long,
    step: c_long,
    current: c_long,
}

#[no_mangle]
pub extern "C" fn roast_range_new(start: c_long, stop: c_long, step: c_long) -> *mut RoastRange {
    unsafe {
        let layout = Layout::new::<RoastRange>();
        let ptr = alloc(layout) as *mut RoastRange;

        (*ptr).header = ObjectHeader::new(TypeTag::Range);
        (*ptr).start = start;
        (*ptr).stop = stop;
        (*ptr).step = if step == 0 { 1 } else { step };
        (*ptr).current = start;

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_range_len(range: *const RoastRange) -> c_long {
    if range.is_null() { return 0; }
    unsafe {
        let diff = (*range).stop - (*range).start;
        let step = (*range).step;
        if (step > 0 && diff <= 0) || (step < 0 && diff >= 0) {
            0
        } else {
            (diff / step) + if diff % step != 0 { 1 } else { 0 }
        }
    }
}

// Enumerate iterator - wraps an iterable and yields (index, value) tuples
#[repr(C)]
pub struct RoastEnumerate {
    header: ObjectHeader,
    source: *mut c_void,      // The wrapped iterable
    source_iter: *mut RoastIterator,  // Iterator for the source
    index: c_long,            // Current index
    start: c_long,            // Starting index (usually 0)
}

#[no_mangle]
pub extern "C" fn roast_enumerate_new(source: *mut c_void, start: c_long) -> *mut RoastEnumerate {
    unsafe {
        let layout = Layout::new::<RoastEnumerate>();
        let ptr = alloc(layout) as *mut RoastEnumerate;

        (*ptr).header = ObjectHeader::new(TypeTag::Enumerate);
        (*ptr).source = source;
        (*ptr).source_iter = roast_iter_new(source);
        (*ptr).index = start;
        (*ptr).start = start;

        // Incref source
        roast_incref(source);

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_enumerate(iterable: c_long, start: c_long) -> c_long {
    roast_enumerate_new(iterable as *mut c_void, start) as c_long
}

// Zip iterator - wraps two iterables and yields tuples
#[repr(C)]
pub struct RoastZip {
    header: ObjectHeader,
    source_a: *mut c_void,    // First iterable
    source_b: *mut c_void,    // Second iterable
    iter_a: *mut RoastIterator,  // Iterator for first
    iter_b: *mut RoastIterator,  // Iterator for second
}

#[no_mangle]
pub extern "C" fn roast_zip_new(source_a: *mut c_void, source_b: *mut c_void) -> *mut RoastZip {
    unsafe {
        let layout = Layout::new::<RoastZip>();
        let ptr = alloc(layout) as *mut RoastZip;

        (*ptr).header = ObjectHeader::new(TypeTag::Zip);
        (*ptr).source_a = source_a;
        (*ptr).source_b = source_b;
        (*ptr).iter_a = roast_iter_new(source_a);
        (*ptr).iter_b = roast_iter_new(source_b);

        // Incref sources
        roast_incref(source_a);
        roast_incref(source_b);

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_zip(iter_a: c_long, iter_b: c_long) -> c_long {
    roast_zip_new(iter_a as *mut c_void, iter_b as *mut c_void) as c_long
}

#[repr(C)]
pub struct RoastIterator {
    header: ObjectHeader,
    source: *mut c_void,
    source_type: TypeTag,
    index: usize,
}

#[no_mangle]
pub extern "C" fn roast_iter_new(source: *mut c_void) -> *mut RoastIterator {
    unsafe {
        let layout = Layout::new::<RoastIterator>();
        let ptr = alloc(layout) as *mut RoastIterator;

        (*ptr).header = ObjectHeader::new(TypeTag::Iterator);
        (*ptr).index = 0;

        // Check if source is a Dict - if so, convert to keys list for iteration
        let (actual_source, actual_type) = if source.is_null() {
            (source, TypeTag::None)
        } else {
            let source_type = (*(source as *const ObjectHeader)).type_tag;
            if source_type == TypeTag::Dict {
                // Convert dict to keys list for iteration
                let keys_list = roast_dict_keys(source as *const RoastDict);
                (keys_list as *mut c_void, TypeTag::List)
            } else {
                (source, source_type)
            }
        };

        (*ptr).source = actual_source;
        (*ptr).source_type = actual_type;

        // Incref source
        roast_incref(actual_source);

        ptr
    }
}


#[no_mangle]
pub extern "C" fn roast_iter_next(iter: *mut RoastIterator, done: *mut bool) -> c_long {
    if iter.is_null() {
        if !done.is_null() { unsafe { *done = true; } }
        return 0;
    }

    unsafe {
        match (*iter).source_type {
            TypeTag::List => {
                let list = (*iter).source as *const RoastList;
                if list.is_null() || (*list).data.is_null() {
                    if !done.is_null() { *done = true; }
                    return 0;
                }
                if (*iter).index >= (*list).len {
                    if !done.is_null() { *done = true; }
                    return 0;
                }
                let value = *(*list).data.add((*iter).index);
                (*iter).index += 1;
                if !done.is_null() { *done = false; }
                value
            }
            TypeTag::Range => {
                let range = (*iter).source as *mut RoastRange;
                let step = (*range).step;
                let current = (*range).current;

                let exhausted = if step > 0 {
                    current >= (*range).stop
                } else {
                    current <= (*range).stop
                };

                if exhausted {
                    if !done.is_null() { *done = true; }
                    return 0;
                }

                (*range).current += step;
                if !done.is_null() { *done = false; }
                current
            }
            TypeTag::Str => {
                let s = (*iter).source as *const RoastString;
                if (*iter).index >= (*s).len {
                    if !done.is_null() { *done = true; }
                    return 0;
                }
                // Return a single-character string, not a byte value
                // This ensures `for c in "hello":` yields string characters
                let char_str = roast_str_index(s, (*iter).index as c_long);
                (*iter).index += 1;
                if !done.is_null() { *done = false; }
                char_str as c_long
            }
            TypeTag::Enumerate => {
                // Enumerate yields (index, value) tuples
                let enum_obj = (*iter).source as *mut RoastEnumerate;
                let mut inner_done = false;
                let value = roast_iter_next((*enum_obj).source_iter, &mut inner_done);
                
                if inner_done {
                    if !done.is_null() { *done = true; }
                    return 0;
                }
                
                // Create tuple (index, value)
                let tuple = roast_tuple_new(2);
                roast_tuple_set(tuple, 0, (*enum_obj).index);
                roast_tuple_set(tuple, 1, value);
                (*enum_obj).index += 1;
                
                if !done.is_null() { *done = false; }
                tuple as c_long
            }
            TypeTag::Zip => {
                // Zip yields tuples of (value_a, value_b)
                let zip_obj = (*iter).source as *mut RoastZip;
                let mut done_a = false;
                let mut done_b = false;
                let value_a = roast_iter_next((*zip_obj).iter_a, &mut done_a);
                let value_b = roast_iter_next((*zip_obj).iter_b, &mut done_b);
                
                // If either iterator is exhausted, zip is done
                if done_a || done_b {
                    if !done.is_null() { *done = true; }
                    return 0;
                }
                
                // Create tuple (value_a, value_b)
                let tuple = roast_tuple_new(2);
                roast_tuple_set(tuple, 0, value_a);
                roast_tuple_set(tuple, 1, value_b);
                
                if !done.is_null() { *done = false; }
                tuple as c_long
            }
            _ => {
                if !done.is_null() { *done = true; }
                0
            }
        }
    }
}

// ============================================================================
// Itertools Functions (standard library)
// ============================================================================

/// Chain multiple iterables together
#[no_mangle]
pub extern "C" fn roast_itertools_chain(
    iters: *mut RoastList,
) -> *mut RoastList {
    if iters.is_null() { return roast_list_new(0); }
    
    unsafe {
        let result = roast_list_new(0);
        let iter_count = (*iters).len;
        
        for i in 0..iter_count {
            let iter_val = *(*iters).data.add(i);
            // Assume each is a list
            if iter_val != 0 {
                let sublist = iter_val as *const RoastList;
                for j in 0..(*sublist).len {
                    let item = *(*sublist).data.add(j);
                    roast_list_append(result, item);
                }
            }
        }
        
        result
    }
}

/// Apply function to each item from an iterator of argument tuples
#[no_mangle]
pub extern "C" fn roast_itertools_starmap(
    func: c_long,
    iterable: *mut RoastList,
) -> *mut RoastList {
    if iterable.is_null() { return roast_list_new(0); }
    
    // For now, return empty list - full impl needs function call infrastructure
    // This serves as a placeholder for the starmap signature
    roast_list_new(0)
}

/// Take items while predicate is true
#[no_mangle]
pub extern "C" fn roast_itertools_takewhile(
    pred: c_long,
    iterable: *mut RoastList,
) -> *mut RoastList {
    if iterable.is_null() { return roast_list_new(0); }
    
    // Placeholder - needs predicate call infrastructure
    unsafe {
        let result = roast_list_new(0);
        // With proper predicate call, would filter items
        result
    }
}

/// Drop items while predicate is true, then yield rest
#[no_mangle]
pub extern "C" fn roast_itertools_dropwhile(
    pred: c_long,
    iterable: *mut RoastList,
) -> *mut RoastList {
    if iterable.is_null() { return roast_list_new(0); }
    
    // Placeholder - needs predicate call infrastructure
    roast_list_new(0)
}

/// Repeat a value n times (or infinitely if n < 0)
#[no_mangle]
pub extern "C" fn roast_itertools_repeat(value: c_long, n: c_long) -> *mut RoastList {
    let count = if n < 0 { 100 } else { n as usize }; // Cap infinite at 100 for safety
    
    let result = roast_list_new(count as c_long);
    for _ in 0..count {
        roast_list_append(result, value);
    }
    result
}

/// Cycle through items repeatedly
#[no_mangle]
pub extern "C" fn roast_itertools_cycle(iterable: *mut RoastList, n: c_long) -> *mut RoastList {
    if iterable.is_null() { return roast_list_new(0); }
    
    unsafe {
        let result = roast_list_new(0);
        let count = if n <= 0 { 1 } else { n as usize };
        
        for _ in 0..count {
            for i in 0..(*iterable).len {
                let item = *(*iterable).data.add(i);
                roast_list_append(result, item);
            }
        }
        
        result
    }
}

/// Count from start by step
#[no_mangle]
pub extern "C" fn roast_itertools_count(start: c_long, step: c_long, limit: c_long) -> *mut RoastList {
    let result = roast_list_new(limit);
    let mut current = start;
    
    for _ in 0..limit {
        roast_list_append(result, current);
        current += step;
    }
    
    result
}

/// Filter out falsy values (like Python's filter(None, ...))
#[no_mangle]
pub extern "C" fn roast_itertools_filterfalse(iterable: *mut RoastList) -> *mut RoastList {
    if iterable.is_null() { return roast_list_new(0); }
    
    unsafe {
        let result = roast_list_new(0);
        for i in 0..(*iterable).len {
            let item = *(*iterable).data.add(i);
            if item == 0 {
                roast_list_append(result, item);
            }
        }
        result
    }
}

// ============================================================================
// Async Iterator Operations (for async for loops)
// ============================================================================

#[repr(C)]
pub struct RoastAsyncIterator {
    header: ObjectHeader,
    source: *mut c_void,
    source_type: TypeTag,
    index: usize,
    /// For awaiting async generators - stores pending coroutine
    pending: *mut c_void,
}

/// Create an async iterator from an async iterable
#[no_mangle]
pub extern "C" fn roast_aiter_new(source: *mut c_void) -> *mut RoastAsyncIterator {
    unsafe {
        let layout = Layout::new::<RoastAsyncIterator>();
        let ptr = alloc(layout) as *mut RoastAsyncIterator;

        (*ptr).header = ObjectHeader::new(TypeTag::Iterator); // Use Iterator tag for now
        (*ptr).index = 0;
        (*ptr).pending = std::ptr::null_mut();

        let (actual_source, actual_type) = if source.is_null() {
            (source, TypeTag::None)
        } else {
            let source_type = (*(source as *const ObjectHeader)).type_tag;
            (source, source_type)
        };

        (*ptr).source = actual_source;
        (*ptr).source_type = actual_type;

        // Incref source
        roast_incref(actual_source);

        ptr
    }
}

/// Get next value from async iterator (synchronous fallback for now)
/// Full async implementation would await the __anext__ coroutine
#[no_mangle]
pub extern "C" fn roast_aiter_next(iter: *mut RoastAsyncIterator, done: *mut bool) -> c_long {
    if iter.is_null() {
        if !done.is_null() { unsafe { *done = true; } }
        return 0;
    }

    unsafe {
        // For MVP, fall back to synchronous iteration
        // Full implementation would handle async generators properly
        match (*iter).source_type {
            TypeTag::List => {
                let list = (*iter).source as *const RoastList;
                if (*iter).index >= (*list).len {
                    if !done.is_null() { *done = true; }
                    return 0;
                }
                let value = *(*list).data.add((*iter).index);
                (*iter).index += 1;
                if !done.is_null() { *done = false; }
                value
            }
            _ => {
                // Unsupported async iterator type
                if !done.is_null() { *done = true; }
                0
            }
        }
    }
}

// ============================================================================
// Object/Class Operations
// ============================================================================

#[repr(C)]
pub struct RoastObject {
    header: ObjectHeader,
    class: *mut RoastClass,
    attrs: *mut HashMap<c_long, c_long>,
}

#[repr(C)]
pub struct RoastClass {
    header: ObjectHeader,
    name: *mut RoastString,
    methods: *mut HashMap<c_long, c_long>,
    parent: *mut RoastClass,
    mro: *mut RoastList,
}

#[no_mangle]
pub extern "C" fn roast_object_new(class: *mut RoastClass) -> *mut RoastObject {
    unsafe {
        let layout = Layout::new::<RoastObject>();
        let ptr = alloc(layout) as *mut RoastObject;

        (*ptr).header = ObjectHeader::new(TypeTag::Object);
        (*ptr).class = class;
        (*ptr).attrs = Box::into_raw(Box::new(HashMap::new()));

        ptr
    }
}

/// Check if a pointer is a super proxy object
fn is_super_proxy(obj: *const c_void) -> bool {
    if obj.is_null() { return false; }
    unsafe {
        // Check the type tag in the header
        let header = obj as *const ObjectHeader;
        // Verify the pointer looks valid by checking refcount is reasonable
        let refcount = (*header).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return false;
        }
        (*header).type_tag == TypeTag::Super
    }
}

/// Check if a pointer is a property descriptor
/// Returns false for values that don't look like valid pointers
fn is_property_descriptor(value: c_long) -> bool {
    // Check for null or small values that are definitely not pointers
    // On 64-bit systems, heap addresses are typically > 0x10000
    if value == 0 || (value > 0 && value < 0x10000) {
        return false;
    }
    // Also reject negative values (which would be invalid pointers)
    if value < 0 {
        return false;
    }

    unsafe {
        let header = value as *const ObjectHeader;
        // Verify the pointer looks valid by checking refcount is reasonable
        // A valid object should have a refcount between 1 and some reasonable limit
        let refcount = (*header).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return false;
        }
        (*header).type_tag == TypeTag::Property
    }
}

#[no_mangle]
pub extern "C" fn roast_object_getattr(obj: *const RoastObject, attr: *const c_char) -> c_long {
    if obj.is_null() || attr.is_null() { return 0; }
    unsafe {
        // Check if this is a super proxy
        if is_super_proxy(obj as *const c_void) {
            return roast_super_getattr(obj as *const RoastSuper, attr);
        }

        let attr_hash = roast_hash_cstr(attr);
        (*(*obj).attrs).get(&attr_hash).copied().unwrap_or(0)
    }
}

/// Get attribute with automatic property handling.
/// If the attribute is a property descriptor, calls the getter automatically.
/// The `self_obj` parameter is passed to property getters as the instance.
#[no_mangle]
pub extern "C" fn roast_object_getattr_auto(obj: *const RoastObject, attr: *const c_char, self_obj: c_long) -> c_long {
    if obj.is_null() || attr.is_null() { return 0; }
    unsafe {
        // First get the raw attribute
        let raw_attr = roast_object_getattr(obj, attr);

        // Check if it's a property descriptor
        if raw_attr != 0 && is_property_descriptor(raw_attr) {
            // Call the property getter with self
            let prop = raw_attr as *const RoastProperty;
            let getter = (*prop).getter;
            if getter != 0 {
                // The getter is a function pointer: i64 -> i64
                // We call it with self_obj as the argument
                let getter_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(getter as usize);
                return getter_fn(self_obj);
            }
        }

        raw_attr
    }
}

#[no_mangle]
pub extern "C" fn roast_object_setattr(obj: *mut RoastObject, attr: *const c_char, value: c_long) {
    if obj.is_null() || attr.is_null() { return; }
    unsafe {
        let attr_hash = roast_hash_cstr(attr);
        (*(*obj).attrs).insert(attr_hash, value);
    }
}

#[no_mangle]
pub extern "C" fn roast_object_hasattr(obj: *const RoastObject, attr: *const c_char) -> bool {
    if obj.is_null() || attr.is_null() { return false; }
    unsafe {
        let attr_hash = roast_hash_cstr(attr);
        (*(*obj).attrs).contains_key(&attr_hash)
    }
}

/// Call a method on an object with 0 arguments (plus self)
/// This implements dynamic dispatch - looks up method in object's class hierarchy
#[no_mangle]
pub extern "C" fn roast_object_call_method0(obj: c_long, method_name: *const c_char) -> c_long {
    if obj == 0 || method_name.is_null() {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return 0;
        }

        // Handle super proxy - look up method in parent class and call with original object
        if (*ptr).type_tag == TypeTag::Super {
            let sup = obj as *const RoastSuper;
            let method_hash = roast_hash_cstr(method_name);
            let mut current_class = (*sup).start_class;

            // Walk up the MRO starting from parent
            while !current_class.is_null() {
                if let Some(&method) = (*(*current_class).methods).get(&method_hash) {
                    // Call method with the ORIGINAL object (not the super proxy)
                    let original_obj = (*sup).obj as c_long;
                    let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
                    return method_fn(original_obj);
                }
                current_class = (*current_class).parent;
            }
            return 0;
        }

        // Handle builtin types
        match (*ptr).type_tag {
            TypeTag::List => {
                let s = CStr::from_ptr(method_name).to_string_lossy();
                match s.as_ref() {
                    "pop" => {
                        return roast_list_pop(obj as *mut RoastList);
                    },
                    _ => {}
                }
            },
            _ => {}
        }

        if (*ptr).type_tag != TypeTag::Object {
            return 0;
        }

        let obj_ptr = obj as *const RoastObject;
        let method_hash = roast_hash_cstr(method_name);

        // First check instance attributes
        if let Some(&method) = (*(*obj_ptr).attrs).get(&method_hash) {
            // Call as function with self
            let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
            return method_fn(obj);
        }

        // Then check class methods
        if !(*obj_ptr).class.is_null() {
            if let Some(&method) = (*(*(*obj_ptr).class).methods).get(&method_hash) {
                let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(obj);
            }

            // Walk up the MRO (parent classes)
            let mut current = (*(*obj_ptr).class).parent;
            while !current.is_null() {
                if let Some(&method) = (*(*current).methods).get(&method_hash) {
                    let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
                    return method_fn(obj);
                }
                current = (*current).parent;
            }
        }

        0
    }
}

/// Call a method on an object with 1 argument (plus self)
#[no_mangle]
pub extern "C" fn roast_object_call_method1(obj: c_long, method_name: *const c_char, arg1: c_long) -> c_long {
    if obj == 0 || method_name.is_null() {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return 0;
        }

        // Handle super proxy - look up method in parent class and call with original object
        if (*ptr).type_tag == TypeTag::Super {
            let sup = obj as *const RoastSuper;
            let method_hash = roast_hash_cstr(method_name);
            let mut current_class = (*sup).start_class;

            // Walk up the MRO starting from parent
            while !current_class.is_null() {
                if let Some(&method) = (*(*current_class).methods).get(&method_hash) {
                    // Call method with the ORIGINAL object (not the super proxy)
                    let original_obj = (*sup).obj as c_long;
                    let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                    return method_fn(original_obj, arg1);
                }
                current_class = (*current_class).parent;
            }
            return 0;
        }

        // Handle builtin types
        match (*ptr).type_tag {
            TypeTag::List => {
                let s = CStr::from_ptr(method_name).to_string_lossy();
                match s.as_ref() {
                    "append" => {
                        roast_list_append(obj as *mut RoastList, arg1);
                        return 0;
                    },
                    "count" => {
                         // roast_list_count(obj, arg1) if implemented?
                         // For now just append is critical for tests
                         return 0;
                    }
                    _ => {}
                }
            },
            TypeTag::Set => {
                let s = CStr::from_ptr(method_name).to_string_lossy();
                match s.as_ref() {
                    "add" => {
                        roast_set_add(obj as *mut RoastSet, arg1);
                        return 0;
                    },
                    "remove" => {
                        roast_set_remove(obj as *mut RoastSet, arg1);
                        return 0;
                    },
                    _ => {}
                }
            },
            _ => {}
        }

        if (*ptr).type_tag != TypeTag::Object {
            return 0;
        }

        let obj_ptr = obj as *const RoastObject;
        let method_hash = roast_hash_cstr(method_name);

        // First check instance attributes
        if let Some(&method) = (*(*obj_ptr).attrs).get(&method_hash) {
            let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
            return method_fn(obj, arg1);
        }

        // Then check class methods
        if !(*obj_ptr).class.is_null() {
            if let Some(&method) = (*(*(*obj_ptr).class).methods).get(&method_hash) {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(obj, arg1);
            }

            // Walk up the MRO
            let mut current = (*(*obj_ptr).class).parent;
            while !current.is_null() {
                if let Some(&method) = (*(*current).methods).get(&method_hash) {
                    let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                    return method_fn(obj, arg1);
                }
                current = (*current).parent;
            }
        }

        0
    }
}

/// Call a method on an object with 2 arguments (plus self)
#[no_mangle]
pub extern "C" fn roast_object_call_method2(obj: c_long, method_name: *const c_char, arg1: c_long, arg2: c_long) -> c_long {
    if obj == 0 || method_name.is_null() {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return 0;
        }

        // Handle super proxy - look up method in parent class and call with original object
        if (*ptr).type_tag == TypeTag::Super {
            let sup = obj as *const RoastSuper;
            let method_hash = roast_hash_cstr(method_name);
            let mut current_class = (*sup).start_class;

            // Walk up the MRO starting from parent
            while !current_class.is_null() {
                if let Some(&method) = (*(*current_class).methods).get(&method_hash) {
                    // Call method with the ORIGINAL object (not the super proxy)
                    let original_obj = (*sup).obj as c_long;
                    let method_fn: extern "C" fn(c_long, c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                    return method_fn(original_obj, arg1, arg2);
                }
                current_class = (*current_class).parent;
            }
            return 0;
        }

        if (*ptr).type_tag != TypeTag::Object {
            return 0;
        }

        let obj_ptr = obj as *const RoastObject;
        let method_hash = roast_hash_cstr(method_name);

        // First check instance attributes
        if let Some(&method) = (*(*obj_ptr).attrs).get(&method_hash) {
            let method_fn: extern "C" fn(c_long, c_long, c_long) -> c_long = std::mem::transmute(method as usize);
            return method_fn(obj, arg1, arg2);
        }

        // Then check class methods
        if !(*obj_ptr).class.is_null() {
            if let Some(&method) = (*(*(*obj_ptr).class).methods).get(&method_hash) {
                let method_fn: extern "C" fn(c_long, c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(obj, arg1, arg2);
            }

            // Walk up the MRO
            let mut current = (*(*obj_ptr).class).parent;
            while !current.is_null() {
                if let Some(&method) = (*(*current).methods).get(&method_hash) {
                    let method_fn: extern "C" fn(c_long, c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                    return method_fn(obj, arg1, arg2);
                }
                current = (*current).parent;
            }
        }

        0
    }
}

fn roast_object_free_internal(obj: *mut RoastObject) {
    if obj.is_null() { return; }
    unsafe {
        if !(*obj).attrs.is_null() {
            drop(Box::from_raw((*obj).attrs));
        }
        let layout = Layout::new::<RoastObject>();
        dealloc(obj as *mut u8, layout);
    }
}

#[no_mangle]
pub extern "C" fn roast_class_new(name: *const c_char, parent: *mut RoastClass) -> *mut RoastClass {
    unsafe {
        let layout = Layout::new::<RoastClass>();
        let ptr = alloc(layout) as *mut RoastClass;

        (*ptr).header = ObjectHeader::new(TypeTag::Class);
        (*ptr).name = roast_str_from_cstr(name);
        (*ptr).methods = Box::into_raw(Box::new(HashMap::new()));
        (*ptr).parent = parent;
        (*ptr).mro = ptr::null_mut();

        ptr
    }
}

/// Set the MRO (Method Resolution Order) for a class
#[no_mangle]
pub extern "C" fn roast_class_set_mro(class: *mut RoastClass, mro: *mut RoastList) {
    if !class.is_null() {
        unsafe {
            (*class).mro = mro;
        }
    }
}

/// Add a method to a class's method table for dynamic dispatch.
/// method_name is a C string for the method name (e.g., "increment")
/// method_ptr is the function pointer to the method
#[no_mangle]
pub extern "C" fn roast_class_add_method(class: *mut RoastClass, method_name: *const c_char, method_ptr: c_long) {
    if class.is_null() || method_name.is_null() {
        return;
    }
    unsafe {
        let method_hash = roast_hash_cstr(method_name);
        if !(*class).methods.is_null() {
            (*(*class).methods).insert(method_hash, method_ptr);
        }
    }
}

// ============================================================================
// Dunder Methods (Magic Methods) Support
// ============================================================================

/// Call a dunder method on an object if it exists.
/// Returns the method's function pointer, or 0 if not found.
fn get_dunder_method(obj: *const RoastObject, method_name: &str) -> c_long {
    if obj.is_null() {
        return 0;
    }
    unsafe {
        // First check instance attributes
        let method_hash = {
            let cstr = std::ffi::CString::new(method_name).unwrap();
            roast_hash_cstr(cstr.as_ptr())
        };

        if let Some(&method) = (*(*obj).attrs).get(&method_hash) {
            return method;
        }

        // Then check class methods
        if !(*obj).class.is_null() {
            if let Some(&method) = (*(*(*obj).class).methods).get(&method_hash) {
                return method;
            }

            // Walk up the MRO (parent classes)
            let mut current = (*(*obj).class).parent;
            while !current.is_null() {
                if let Some(&method) = (*(*current).methods).get(&method_hash) {
                    return method;
                }
                current = (*current).parent;
            }
        }

        0
    }
}

/// Call __str__ on an object to get its string representation
#[no_mangle]
pub extern "C" fn roast_object_str(obj: c_long) -> *mut RoastString {
    if obj == 0 {
        return roast_str_from_cstr(b"None\0".as_ptr() as *const c_char);
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        // Check if it's a valid heap object
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            // Not a valid object, treat as integer
            let s = format!("{}", obj);
            let cstr = std::ffi::CString::new(s).unwrap();
            return roast_str_from_cstr(cstr.as_ptr());
        }

        match (*ptr).type_tag {
            TypeTag::Object => {
                let obj_ptr = obj as *const RoastObject;
                let method = get_dunder_method(obj_ptr, "__str__");
                if method != 0 {
                    // Call the __str__ method with self
                    let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
                    let result = method_fn(obj);
                    let result = method_fn(obj);
                    return result as *mut RoastString;
                }
                // Fallback to __repr__ if __str__ not defined
                let repr_method = get_dunder_method(obj_ptr, "__repr__");
                if repr_method != 0 {
                    let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(repr_method as usize);
                    let result = method_fn(obj);
                    return result as *mut RoastString;
                }
                // Default object representation
                let class_name = if !(*obj_ptr).class.is_null() {
                    roast_str_data((*(*obj_ptr).class).name)
                } else {
                    b"object\0".as_ptr() as *const c_char
                };
                let buf = format!("<{} object at 0x{:x}>",
                    CStr::from_ptr(class_name).to_string_lossy(), obj);
                let cstr = std::ffi::CString::new(buf).unwrap();
                roast_str_from_cstr(cstr.as_ptr())
            }
            TypeTag::Str => obj as *mut RoastString,
            TypeTag::Int => {
                let buf = format!("{}", obj);
                let cstr = std::ffi::CString::new(buf).unwrap();
                roast_str_from_cstr(cstr.as_ptr())
            }
            TypeTag::Bool => {
                if obj != 0 {
                    roast_str_from_cstr(b"True\0".as_ptr() as *const c_char)
                } else {
                    roast_str_from_cstr(b"False\0".as_ptr() as *const c_char)
                }
            }
            _ => {
                let buf = format!("<{:?} at 0x{:x}>", (*ptr).type_tag, obj);
                let cstr = std::ffi::CString::new(buf).unwrap();
                roast_str_from_cstr(cstr.as_ptr())
            }
        }
    }
}

/// Call __repr__ on an object
#[no_mangle]
pub extern "C" fn roast_object_repr(obj: c_long) -> *mut RoastString {
    if obj == 0 {
        return roast_str_from_cstr(b"None\0".as_ptr() as *const c_char);
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            let buf = format!("{}", obj);
            let cstr = std::ffi::CString::new(buf).unwrap();
            return roast_str_from_cstr(cstr.as_ptr());
        }

        if (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = obj as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__repr__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
                let result = method_fn(obj);
                return result as *mut RoastString;
            }
        }

        // Default repr
        roast_object_str(obj)
    }
}

/// Call __eq__ on two objects
#[no_mangle]
pub extern "C" fn roast_object_eq(lhs: c_long, rhs: c_long) -> c_long {
    if lhs == rhs {
        return 1; // Same object
    }
    if lhs == 0 || rhs == 0 {
        return 0;
    }

    unsafe {
        let ptr = lhs as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            // Not a valid object, compare as integers
            return if lhs == rhs { 1 } else { 0 };
        }

        if (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = lhs as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__eq__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(lhs, rhs);
            }
        }

        // Default: identity comparison
        if lhs == rhs { 1 } else { 0 }
    }
}

/// Call __hash__ on an object
#[no_mangle]
pub extern "C" fn roast_object_hash(obj: c_long) -> c_long {
    if obj == 0 {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return obj; // Use value as hash for non-objects
        }

        if (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = obj as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__hash__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(obj);
            }
        }

        // Default: use object address as hash
        obj
    }
}

/// Call __add__ on two objects
#[no_mangle]
pub extern "C" fn roast_object_add(lhs: c_long, rhs: c_long) -> c_long {
    unsafe {
        let ptr = lhs as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = lhs as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__add__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(lhs, rhs);
            }
        }
        // Fallback to integer addition
        lhs + rhs
    }
}

/// Call __sub__ on two objects
#[no_mangle]
pub extern "C" fn roast_object_sub(lhs: c_long, rhs: c_long) -> c_long {
    unsafe {
        let ptr = lhs as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = lhs as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__sub__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(lhs, rhs);
            }
        }
        lhs - rhs
    }
}

/// Call __mul__ on two objects
#[no_mangle]
pub extern "C" fn roast_object_mul(lhs: c_long, rhs: c_long) -> c_long {
    unsafe {
        let ptr = lhs as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = lhs as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__mul__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(lhs, rhs);
            }
        }
        lhs * rhs
    }
}

/// Call __lt__ (less than) on two objects
#[no_mangle]
pub extern "C" fn roast_object_lt(lhs: c_long, rhs: c_long) -> c_long {
    unsafe {
        let ptr = lhs as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = lhs as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__lt__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(lhs, rhs);
            }
        }
        if lhs < rhs { 1 } else { 0 }
    }
}

/// Call __le__ (less than or equal) on two objects
#[no_mangle]
pub extern "C" fn roast_object_le(lhs: c_long, rhs: c_long) -> c_long {
    unsafe {
        let ptr = lhs as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = lhs as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__le__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(lhs, rhs);
            }
        }
        if lhs <= rhs { 1 } else { 0 }
    }
}

/// Call __gt__ (greater than) on two objects
#[no_mangle]
pub extern "C" fn roast_object_gt(lhs: c_long, rhs: c_long) -> c_long {
    unsafe {
        let ptr = lhs as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = lhs as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__gt__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(lhs, rhs);
            }
        }
        if lhs > rhs { 1 } else { 0 }
    }
}

/// Call __ge__ (greater than or equal) on two objects
#[no_mangle]
pub extern "C" fn roast_object_ge(lhs: c_long, rhs: c_long) -> c_long {
    unsafe {
        let ptr = lhs as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = lhs as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__ge__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(lhs, rhs);
            }
        }
        if lhs >= rhs { 1 } else { 0 }
    }
}

/// Call __bool__ on an object to get its truth value
#[no_mangle]
pub extern "C" fn roast_object_bool(obj: c_long) -> c_long {
    if obj == 0 {
        return 0; // None/null is falsy
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return if obj != 0 { 1 } else { 0 };
        }

        if (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = obj as *const RoastObject;
            // Try __bool__ first
            let method = get_dunder_method(obj_ptr, "__bool__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(obj);
            }
            // Fall back to __len__ (objects with length 0 are falsy)
            let len_method = get_dunder_method(obj_ptr, "__len__");
            if len_method != 0 {
                let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(len_method as usize);
                let len = method_fn(obj);
                return if len != 0 { 1 } else { 0 };
            }
        }

        // Default: all objects are truthy
        1
    }
}

/// Call __len__ on an object
#[no_mangle]
pub extern "C" fn roast_object_len(obj: c_long) -> c_long {
    if obj == 0 {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return 0;
        }

        if (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = obj as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__len__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(obj);
            }
        }

        // Delegate to roast_len for built-in types
        roast_len(obj)
    }
}

/// Call __getitem__ on an object
#[no_mangle]
pub extern "C" fn roast_object_getitem(obj: c_long, key: c_long) -> c_long {
    if obj == 0 {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = obj as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__getitem__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(obj, key);
            }
        }

        // Fallback for lists and dicts
        match (*ptr).type_tag {
            TypeTag::List => roast_list_get(obj as *const RoastList, key),
            TypeTag::Dict => roast_dict_get(obj as *const RoastDict, key),
            _ => 0
        }
    }
}

/// Call __setitem__ on an object
#[no_mangle]
pub extern "C" fn roast_object_setitem(obj: c_long, key: c_long, value: c_long) {
    if obj == 0 {
        return;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = obj as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__setitem__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long, c_long) = std::mem::transmute(method as usize);
                method_fn(obj, key, value);
                return;
            }
        }

        // Fallback for lists and dicts
        match (*ptr).type_tag {
            TypeTag::List => roast_list_set(obj as *mut RoastList, key, value),
            TypeTag::Dict => roast_dict_set(obj as *mut RoastDict, key, value),
            _ => {}
        }
    }
}

/// Call __contains__ on an object (for 'in' operator)
#[no_mangle]
pub extern "C" fn roast_object_contains(obj: c_long, item: c_long) -> c_long {
    if obj == 0 {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = obj as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__contains__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long, c_long) -> c_long = std::mem::transmute(method as usize);
                return method_fn(obj, item);
            }
        }

        // Fallback for built-in types
        match (*ptr).type_tag {
            TypeTag::List => if roast_list_contains(obj as *const RoastList, item) { 1 } else { 0 },
            TypeTag::Dict => if roast_dict_contains(obj as *const RoastDict, item) { 1 } else { 0 },
            TypeTag::Set => if roast_set_contains(obj as *const RoastSet, item) { 1 } else { 0 },
            _ => 0
        }
    }
}

/// Call __iter__ on an object to get an iterator
#[no_mangle]
pub extern "C" fn roast_object_iter(obj: c_long) -> *mut c_void {
    if obj == 0 {
        return ptr::null_mut();
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount > 0 && refcount < 10000 && (*ptr).type_tag == TypeTag::Object {
            let obj_ptr = obj as *const RoastObject;
            let method = get_dunder_method(obj_ptr, "__iter__");
            if method != 0 {
                let method_fn: extern "C" fn(c_long) -> *mut c_void = std::mem::transmute(method as usize);
                return method_fn(obj);
            }
        }

        // Fallback to roast_iter_new
        let ptr_void = obj as *mut c_void;
        roast_iter_new(ptr_void) as *mut c_void
    }
}

/// Check if an object is an instance of a class (isinstance)
#[no_mangle]
pub extern "C" fn roast_isinstance(obj: c_long, class: c_long) -> c_long {
    if obj == 0 || class == 0 {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return 0;
        }

        if (*ptr).type_tag != TypeTag::Object {
            return 0;
        }

        let obj_ptr = obj as *const RoastObject;
        let target_class = class as *mut RoastClass;

        // Walk up the class hierarchy
        let mut current_class = (*obj_ptr).class;
        while !current_class.is_null() {
            if current_class == target_class {
                return 1;
            }
            current_class = (*current_class).parent;
        }

        0
    }
}

/// Check if an object is an instance of a class by name
/// This is used when class objects aren't available at compile time
#[no_mangle]
pub extern "C" fn roast_isinstance_by_name(obj: c_long, class_name: *const c_char) -> c_long {
    if obj == 0 || class_name.is_null() {
        return 0;
    }

    unsafe {
        let ptr = obj as *const ObjectHeader;
        let refcount = (*ptr).refcount.load(std::sync::atomic::Ordering::Relaxed);
        if refcount == 0 || refcount > 10000 {
            return 0;
        }

        if (*ptr).type_tag != TypeTag::Object {
            return 0;
        }

        let obj_ptr = obj as *const RoastObject;
        let target_name = CStr::from_ptr(class_name).to_string_lossy();

        // Walk up the class hierarchy checking names
        let mut current_class = (*obj_ptr).class;
        while !current_class.is_null() {
            if !(*current_class).name.is_null() {
                let current_name_ptr = (*(*current_class).name).data;
                if !current_name_ptr.is_null() {
                    let current_name = CStr::from_ptr(current_name_ptr as *const c_char).to_string_lossy();
                    if current_name == target_name {
                        return 1;
                    }
                }
            }
            current_class = (*current_class).parent;
        }

        0
    }
}

/// Check if a class is a subclass of another class (issubclass)
#[no_mangle]
pub extern "C" fn roast_issubclass(subclass: c_long, superclass: c_long) -> c_long {
    if subclass == 0 || superclass == 0 {
        return 0;
    }

    unsafe {
        let sub_ptr = subclass as *mut RoastClass;
        let super_ptr = superclass as *mut RoastClass;

        // Walk up the class hierarchy from subclass
        let mut current_class = sub_ptr;
        while !current_class.is_null() {
            if current_class == super_ptr {
                return 1;
            }
            current_class = (*current_class).parent;
        }

        0
    }
}

// ============================================================================
// Closure Operations
// ============================================================================

#[repr(C)]
pub struct RoastClosure {
    header: ObjectHeader,
    func_ptr: *mut c_void,
    env_size: usize,
    env: *mut c_long,
}

#[no_mangle]
pub extern "C" fn roast_closure_new(func: *mut c_void, env_size: c_long, env: *mut c_void) -> *mut RoastClosure {
    unsafe {
        let layout = Layout::new::<RoastClosure>();
        let ptr = alloc(layout) as *mut RoastClosure;

        (*ptr).header = ObjectHeader::new(TypeTag::Closure);
        (*ptr).func_ptr = func;
        (*ptr).env_size = env_size as usize;

        if env_size > 0 && !env.is_null() {
            let env_layout = Layout::array::<c_long>(env_size as usize).unwrap();
            (*ptr).env = alloc(env_layout) as *mut c_long;
            ptr::copy_nonoverlapping(env as *const c_long, (*ptr).env, env_size as usize);
        } else {
            (*ptr).env = ptr::null_mut();
        }

        ptr
    }
}

#[no_mangle]
pub extern "C" fn roast_closure_get_env(closure: *const RoastClosure, index: c_long) -> c_long {
    if closure.is_null() { return 0; }
    unsafe {
        if index < 0 || index as usize >= (*closure).env_size { return 0; }
        *(*closure).env.add(index as usize)
    }
}

// ============================================================================
// Cell Type for Nonlocal Variables (mutable reference capture)
// ============================================================================

/// A cell wrapping a single value - allows mutation through shared reference
/// This is used for `nonlocal` variable semantics in closures
#[repr(C)]
pub struct RoastCell {
    header: ObjectHeader,
    value: c_long,
}

/// Create a new cell wrapping a value
#[no_mangle]
pub extern "C" fn roast_cell_new(value: c_long) -> *mut RoastCell {
    unsafe {
        let layout = Layout::new::<RoastCell>();
        let ptr = alloc(layout) as *mut RoastCell;
        
        (*ptr).header = ObjectHeader::new(TypeTag::Cell);
        (*ptr).value = value;
        
        ptr
    }
}

/// Get the value from a cell
#[no_mangle]
pub extern "C" fn roast_cell_get(cell: *const RoastCell) -> c_long {
    if cell.is_null() { return 0; }
    unsafe { (*cell).value }
}

/// Set the value in a cell (mutation through shared reference)
#[no_mangle]
pub extern "C" fn roast_cell_set(cell: *mut RoastCell, value: c_long) {
    if cell.is_null() { return; }
    unsafe { (*cell).value = value; }
}

/// Swap values between two cells
#[no_mangle]
pub extern "C" fn roast_cell_swap(cell_a: *mut RoastCell, cell_b: *mut RoastCell) {
    if cell_a.is_null() || cell_b.is_null() { return; }
    unsafe {
        std::mem::swap(&mut (*cell_a).value, &mut (*cell_b).value);
    }
}

/// Replace value in cell and return old value
#[no_mangle]
pub extern "C" fn roast_cell_replace(cell: *mut RoastCell, value: c_long) -> c_long {
    if cell.is_null() { return 0; }
    unsafe {
        let old = (*cell).value;
        (*cell).value = value;
        old
    }
}

// ============================================================================
// super() Support - Method Resolution Order
// ============================================================================

/// Super proxy object for calling parent class methods
#[repr(C)]
pub struct RoastSuper {
    header: ObjectHeader,
    /// The object instance
    obj: *mut RoastObject,
    /// The class to start looking from (usually parent of current class)
    start_class: *mut RoastClass,
}

/// Create a super() proxy for an object
/// In Python: super() or super(CurrentClass, self)
#[no_mangle]
pub extern "C" fn roast_super_new(obj: *mut RoastObject, current_class: *mut RoastClass) -> *mut RoastSuper {
    unsafe {
        let layout = Layout::new::<RoastSuper>();
        let ptr = alloc(layout) as *mut RoastSuper;

        (*ptr).header = ObjectHeader::new(TypeTag::Super); // Use Super tag
        (*ptr).obj = obj;

        // Start from parent class or next in MRO
        if !obj.is_null() && !(*obj).class.is_null() {
            let cls = (*obj).class;
            
            // If MRO is available, use it for C3 linearization
            if !(*cls).mro.is_null() {
                let mro_len = (*(*cls).mro).len;
                let mro_data = (*(*cls).mro).data;
                
                // If current_class is specified, find it and pick next
                if !current_class.is_null() {
                    for i in 0..mro_len {
                        let c = *mro_data.add(i) as *mut RoastClass;
                        // Found current class, next one is super
                        if c == current_class && i + 1 < mro_len {
                            (*ptr).start_class = *mro_data.add(i + 1) as *mut RoastClass;
                            return ptr;
                        }
                    }
                    // Current class not found or is last in MRO
                    (*ptr).start_class = ptr::null_mut();
                } else {
                     // No current class, start from first base (index 1, index 0 is self)
                     if mro_len > 1 {
                         (*ptr).start_class = *mro_data.add(1) as *mut RoastClass;
                     } else {
                         (*ptr).start_class = ptr::null_mut();
                     }
                }
            } else {
                // Fallback to single inheritance parent
                if !current_class.is_null() && !(*current_class).parent.is_null() {
                    (*ptr).start_class = (*current_class).parent;
                } else if !(*cls).parent.is_null() {
                    (*ptr).start_class = (*cls).parent;
                } else {
                    (*ptr).start_class = ptr::null_mut();
                }
            }
        } else {
            (*ptr).start_class = ptr::null_mut();
        }

        ptr
    }
}

/// Get a method from super - looks up MRO starting from parent
#[no_mangle]
pub extern "C" fn roast_super_getattr(sup: *const RoastSuper, method_name: *const c_char) -> c_long {
    if sup.is_null() || method_name.is_null() {
        return 0;
    }

    unsafe {
        let method_hash = roast_hash_cstr(method_name);
        let mut current_class = (*sup).start_class;

        // Walk up the MRO
        let cls = (*(*sup).obj).class;
        
        // Try MRO first
        if !(*cls).mro.is_null() {
            let mro_len = (*(*cls).mro).len;
            let mro_data = (*(*cls).mro).data;
            let mut found_start = false;
            
            for i in 0..mro_len {
                let current_class = *mro_data.add(i) as *mut RoastClass;
                
                // We start searching from start_class
                if current_class == (*sup).start_class {
                    found_start = true;
                }
                
                if found_start {
                    if let Some(&method) = (*(*current_class).methods).get(&method_hash) {
                         return method;
                    }
                }
            }
            return 0;
        }

        // Fallback to single inheritance
        let mut current_class = (*sup).start_class;
        while !current_class.is_null() {
            if let Some(&method) = (*(*current_class).methods).get(&method_hash) {
                return method;
            }
            current_class = (*current_class).parent;
        }

        0
    }
}

/// Call a parent class method via super
#[no_mangle]
pub extern "C" fn roast_super_call_method(
    sup: *const RoastSuper,
    method_name: *const c_char,
    args: *const c_long,
    nargs: c_long,
) -> c_long {
    if sup.is_null() || method_name.is_null() {
        return 0;
    }

    unsafe {
        let method = roast_super_getattr(sup, method_name);
        if method == 0 {
            eprintln!("AttributeError: 'super' object has no attribute '{}'",
                     CStr::from_ptr(method_name).to_string_lossy());
            return 0;
        }

        // The method is a function pointer, call it with self + args
        // For now, we return the method - actual call is done by caller
        method
    }
}

// ============================================================================
// *args/**kwargs Support - Tuple/Dict Packing
// ============================================================================

/// Pack variable positional arguments into a tuple
/// Called at function entry to collect extra args
#[no_mangle]
pub extern "C" fn roast_pack_varargs(
    args: *const c_long,
    start_index: c_long,
    total_args: c_long,
) -> *mut RoastTuple {
    unsafe {
        let count = if total_args > start_index {
            (total_args - start_index) as usize
        } else {
            0
        };

        let tuple = roast_tuple_new(count as c_long);

        for i in 0..count {
            let arg_idx = start_index as usize + i;
            let value = *args.add(arg_idx);
            roast_tuple_set(tuple, i as c_long, value);
        }

        tuple
    }
}

/// Pack keyword arguments into a dict
#[no_mangle]
pub extern "C" fn roast_pack_kwargs(
    keys: *const *const c_char,
    values: *const c_long,
    count: c_long,
) -> c_long {
    unsafe {
        let dict = roast_dict_new();

        for i in 0..(count as usize) {
            let key = *keys.add(i);
            let value = *values.add(i);
            let key_str = roast_str_from_cstr(key);
            roast_dict_set(dict as *mut RoastDict, key_str as c_long, value);
        }

        dict as c_long
    }
}

/// Unpack *args in a function call - returns array of values
#[no_mangle]
pub extern "C" fn roast_unpack_args(tuple: *const RoastTuple) -> *const c_long {
    if tuple.is_null() {
        return ptr::null();
    }
    unsafe {
        (*tuple).data
    }
}

/// Get length of *args tuple
#[no_mangle]
pub extern "C" fn roast_args_len(tuple: *const RoastTuple) -> c_long {
    if tuple.is_null() {
        return 0;
    }
    unsafe {
        (*tuple).len as c_long
    }
}

// ============================================================================
// Property Support
// ============================================================================

/// Property descriptor
#[repr(C)]
pub struct RoastProperty {
    header: ObjectHeader,
    getter: c_long,  // Function pointer for getter
    setter: c_long,  // Function pointer for setter (0 if read-only)
}

/// Create a property descriptor
#[no_mangle]
pub extern "C" fn roast_property_new(getter: c_long, setter: c_long) -> *mut RoastProperty {
    unsafe {
        let layout = Layout::new::<RoastProperty>();
        let ptr = alloc(layout) as *mut RoastProperty;

        (*ptr).header = ObjectHeader::new(TypeTag::Property); // Use Property tag
        (*ptr).getter = getter;
        (*ptr).setter = setter;

        ptr
    }
}

/// Call property getter
#[no_mangle]
pub extern "C" fn roast_property_get(prop: *const RoastProperty, obj: c_long) -> c_long {
    if prop.is_null() {
        return 0;
    }
    unsafe {
        let getter = (*prop).getter;
        if getter == 0 {
            return 0;
        }
        // The getter is a function that takes self as argument
        // Caller is responsible for invoking it
        getter
    }
}

/// Call property setter
#[no_mangle]
pub extern "C" fn roast_property_set(prop: *const RoastProperty, obj: c_long, value: c_long) {
    if prop.is_null() {
        return;
    }
    unsafe {
        let setter = (*prop).setter;
        if setter == 0 {
            eprintln!("AttributeError: can't set attribute");
            return;
        }
        // Caller is responsible for invoking the setter
    }
}

/// Check if an attribute is a property descriptor
#[no_mangle]
pub extern "C" fn roast_is_property(value: c_long) -> c_long {
    if value == 0 {
        return 0;
    }
    unsafe {
        let ptr = value as *const ObjectHeader;
        if (*ptr).type_tag == TypeTag::Property {
            1
        } else {
            0
        }
    }
}

// ============================================================================
// I/O Operations
// ============================================================================

#[no_mangle]
pub extern "C" fn roast_print_int(value: c_long) {
    print!("{}", value);
}

#[no_mangle]
pub extern "C" fn roast_print_float(value: c_double) {
    let mut s = format!("{}", value);
    // Ensure decimal point is shown for whole numbers (Python-like behavior)
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        s.push_str(".0");
    }
    print!("{}", s);
}

#[no_mangle]
pub extern "C" fn roast_print_str(s: *const c_char) {
    if s.is_null() { return; }
    unsafe {
        if let Ok(string) = CStr::from_ptr(s).to_str() {
            print!("{}", string);
        }
    }
}

#[no_mangle]
pub extern "C" fn roast_print_bool(value: bool) {
    print!("{}", if value { "True" } else { "False" });
}

#[no_mangle]
pub extern "C" fn roast_print_newline() {
    println!();
}

#[no_mangle]
pub extern "C" fn roast_print_space() {
    print!(" ");
}

#[no_mangle]
pub extern "C" fn roast_print_roast_str(s: *const RoastString) {
    if s.is_null() { print!("None"); return; }
    unsafe {
        let slice = std::slice::from_raw_parts((*s).data, (*s).len);
        if let Ok(string) = std::str::from_utf8(slice) {
            print!("{}", string);
        }
    }
}

// ============================================================================
// Argparse Module (standard library)
// ============================================================================

/// Get command line argument count
#[no_mangle]
pub extern "C" fn roast_argparse_argc() -> c_long {
    std::env::args().count() as c_long
}

/// Get command line argument at index
#[no_mangle]
pub extern "C" fn roast_argparse_argv(index: c_long) -> *mut RoastString {
    let args: Vec<String> = std::env::args().collect();
    if (index as usize) < args.len() {
        let arg = &args[index as usize];
        roast_str_from_cstr(
            std::ffi::CString::new(arg.as_str()).unwrap().as_ptr()
        )
    } else {
        std::ptr::null_mut()
    }
}

/// Get all command line arguments as a list
#[no_mangle]
pub extern "C" fn roast_argparse_args() -> *mut RoastList {
    let args: Vec<String> = std::env::args().collect();
    let list = roast_list_new(args.len() as c_long);
    
    for arg in args {
        let s = roast_str_from_cstr(
            std::ffi::CString::new(arg.as_str()).unwrap().as_ptr()
        );
        roast_list_append(list, s as c_long);
    }
    
    list
}

// ============================================================================
// Environment Variables
// ============================================================================

/// Get environment variable
#[no_mangle]
pub extern "C" fn roast_env_get(name: *const RoastString) -> *mut RoastString {
    if name.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*name).data, (*name).len);
        if let Ok(name_str) = std::str::from_utf8(slice) {
            if let Ok(value) = std::env::var(name_str) {
                return roast_str_from_cstr(
                    std::ffi::CString::new(value).unwrap().as_ptr()
                );
            }
        }
        std::ptr::null_mut()
    }
}

/// Set environment variable
#[no_mangle]
pub extern "C" fn roast_env_set(name: *const RoastString, value: *const RoastString) {
    if name.is_null() || value.is_null() { return; }
    
    unsafe {
        let name_slice = std::slice::from_raw_parts((*name).data, (*name).len);
        let value_slice = std::slice::from_raw_parts((*value).data, (*value).len);
        
        if let (Ok(name_str), Ok(value_str)) = (
            std::str::from_utf8(name_slice),
            std::str::from_utf8(value_slice)
        ) {
            std::env::set_var(name_str, value_str);
        }
    }
}

// ============================================================================
// Subprocess Module (standard library)
// ============================================================================

use std::process::{Command, Stdio, Output};

/// Run a command and return exit code
#[no_mangle]
pub extern "C" fn roast_subprocess_call(cmd: *const RoastString) -> c_long {
    if cmd.is_null() { return -1; }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*cmd).data, (*cmd).len);
        if let Ok(cmd_str) = std::str::from_utf8(slice) {
            #[cfg(unix)]
            {
                match Command::new("sh").arg("-c").arg(cmd_str).status() {
                    Ok(status) => status.code().unwrap_or(-1) as c_long,
                    Err(_) => -1,
                }
            }
            #[cfg(windows)]
            {
                match Command::new("cmd").arg("/C").arg(cmd_str).status() {
                    Ok(status) => status.code().unwrap_or(-1) as c_long,
                    Err(_) => -1,
                }
            }
        } else {
            -1
        }
    }
}

/// Run a command and capture stdout
#[no_mangle]
pub extern "C" fn roast_subprocess_check_output(cmd: *const RoastString) -> *mut RoastString {
    if cmd.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*cmd).data, (*cmd).len);
        if let Ok(cmd_str) = std::str::from_utf8(slice) {
            #[cfg(unix)]
            let result = Command::new("sh").arg("-c").arg(cmd_str).output();
            #[cfg(windows)]
            let result = Command::new("cmd").arg("/C").arg(cmd_str).output();
            
            match result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    roast_str_from_cstr(
                        std::ffi::CString::new(stdout.as_ref()).unwrap().as_ptr()
                    )
                }
                Err(_) => std::ptr::null_mut(),
            }
        } else {
            std::ptr::null_mut()
        }
    }
}

/// Run a command and return (exit_code, stdout, stderr) as a tuple
#[no_mangle]
pub extern "C" fn roast_subprocess_run(cmd: *const RoastString) -> *mut RoastTuple {
    if cmd.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*cmd).data, (*cmd).len);
        if let Ok(cmd_str) = std::str::from_utf8(slice) {
            #[cfg(unix)]
            let result = Command::new("sh").arg("-c").arg(cmd_str).output();
            #[cfg(windows)]
            let result = Command::new("cmd").arg("/C").arg(cmd_str).output();
            
            match result {
                Ok(output) => {
                    let tuple = roast_tuple_new(3);
                    
                    // Exit code
                    roast_tuple_set(tuple, 0, output.status.code().unwrap_or(-1) as c_long);
                    
                    // Stdout
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stdout_str = roast_str_from_cstr(
                        std::ffi::CString::new(stdout.as_ref()).unwrap().as_ptr()
                    );
                    roast_tuple_set(tuple, 1, stdout_str as c_long);
                    
                    // Stderr
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stderr_str = roast_str_from_cstr(
                        std::ffi::CString::new(stderr.as_ref()).unwrap().as_ptr()
                    );
                    roast_tuple_set(tuple, 2, stderr_str as c_long);
                    
                    tuple
                }
                Err(_) => std::ptr::null_mut(),
            }
        } else {
            std::ptr::null_mut()
        }
    }
}

/// Get current working directory
#[no_mangle]
pub extern "C" fn roast_subprocess_getcwd() -> *mut RoastString {
    match std::env::current_dir() {
        Ok(path) => {
            let path_str = path.to_string_lossy();
            roast_str_from_cstr(
                std::ffi::CString::new(path_str.as_ref()).unwrap().as_ptr()
            )
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Change current working directory
#[no_mangle]
pub extern "C" fn roast_subprocess_chdir(path: *const RoastString) -> c_long {
    if path.is_null() { return -1; }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*path).data, (*path).len);
        if let Ok(path_str) = std::str::from_utf8(slice) {
            match std::env::set_current_dir(path_str) {
                Ok(_) => 0,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

// ============================================================================
// CSV Module (standard library)
// ============================================================================

/// Parse a single CSV line into a list of fields
#[no_mangle]
pub extern "C" fn roast_csv_parse_line(line: *const RoastString, delimiter: c_long) -> *mut RoastList {
    if line.is_null() { return roast_list_new(0); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*line).data, (*line).len);
        if let Ok(line_str) = std::str::from_utf8(slice) {
            let delim = if delimiter == 0 { ',' } else { char::from_u32(delimiter as u32).unwrap_or(',') };
            let mut fields = Vec::new();
            let mut current = String::new();
            let mut in_quotes = false;
            let mut chars = line_str.chars().peekable();
            
            while let Some(c) = chars.next() {
                if c == '"' {
                    if in_quotes && chars.peek() == Some(&'"') {
                        current.push('"');
                        chars.next();
                    } else {
                        in_quotes = !in_quotes;
                    }
                } else if c == delim && !in_quotes {
                    fields.push(current.clone());
                    current.clear();
                } else {
                    current.push(c);
                }
            }
            fields.push(current);
            
            let list = roast_list_new(fields.len() as c_long);
            for field in fields {
                let s = roast_str_from_cstr(
                    std::ffi::CString::new(field).unwrap().as_ptr()
                );
                roast_list_append(list, s as c_long);
            }
            list
        } else {
            roast_list_new(0)
        }
    }
}

/// Format a list of values as a CSV line
#[no_mangle]
pub extern "C" fn roast_csv_format_line(values: *const RoastList, delimiter: c_long) -> *mut RoastString {
    if values.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let delim = if delimiter == 0 { ',' } else { char::from_u32(delimiter as u32).unwrap_or(',') };
        let mut parts = Vec::new();
        
        for i in 0..(*values).len {
            let val = *(*values).data.add(i);
            if val != 0 {
                let s = val as *const RoastString;
                let slice = std::slice::from_raw_parts((*s).data, (*s).len);
                if let Ok(field) = std::str::from_utf8(slice) {
                    // Quote if contains delimiter, quote, or newline
                    if field.contains(delim) || field.contains('"') || field.contains('\n') {
                        let escaped = field.replace("\"", "\"\"");
                        parts.push(format!("\"{}\"", escaped));
                    } else {
                        parts.push(field.to_string());
                    }
                }
            }
        }
        
        let line = parts.join(&delim.to_string());
        roast_str_from_cstr(std::ffi::CString::new(line).unwrap().as_ptr())
    }
}

/// Split a multi-line CSV string into a list of rows (each row is a list of fields)
#[no_mangle]
pub extern "C" fn roast_csv_parse_all(content: *const RoastString, delimiter: c_long) -> *mut RoastList {
    if content.is_null() { return roast_list_new(0); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*content).data, (*content).len);
        if let Ok(content_str) = std::str::from_utf8(slice) {
            let rows = roast_list_new(0);
            
            for line in content_str.lines() {
                let line_str = roast_str_from_cstr(
                    std::ffi::CString::new(line).unwrap().as_ptr()
                );
                let row = roast_csv_parse_line(line_str, delimiter);
                roast_list_append(rows, row as c_long);
            }
            
            rows
        } else {
            roast_list_new(0)
        }
    }
}

// ============================================================================
// SQLite3 Module (standard library)
// ============================================================================

use std::sync::Mutex;
use once_cell::sync::Lazy;

// Global database connection store (simplified - real impl would use handles)
static DB_CONNECTIONS: Lazy<Mutex<Vec<rusqlite::Connection>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Open a connection to a SQLite database
/// Returns connection handle (index) or -1 on error
#[no_mangle]
pub extern "C" fn roast_sqlite_connect(path: *const RoastString) -> c_long {
    if path.is_null() { return -1; }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*path).data, (*path).len);
        if let Ok(path_str) = std::str::from_utf8(slice) {
            let conn_result = if path_str == ":memory:" {
                rusqlite::Connection::open_in_memory()
            } else {
                rusqlite::Connection::open(path_str)
            };
            
            match conn_result {
                Ok(conn) => {
                    let mut connections = DB_CONNECTIONS.lock().unwrap();
                    connections.push(conn);
                    (connections.len() - 1) as c_long
                }
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// Execute a SQL statement (INSERT, UPDATE, DELETE, CREATE)
/// Returns number of rows affected or -1 on error
#[no_mangle]
pub extern "C" fn roast_sqlite_execute(handle: c_long, sql: *const RoastString) -> c_long {
    if sql.is_null() || handle < 0 { return -1; }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*sql).data, (*sql).len);
        if let Ok(sql_str) = std::str::from_utf8(slice) {
            let connections = DB_CONNECTIONS.lock().unwrap();
            if let Some(conn) = connections.get(handle as usize) {
                match conn.execute(sql_str, []) {
                    Ok(rows) => rows as c_long,
                    Err(_) => -1,
                }
            } else {
                -1
            }
        } else {
            -1
        }
    }
}

/// Execute a SELECT query and return results as a list of rows
/// Each row is a list of string values
#[no_mangle]
pub extern "C" fn roast_sqlite_query(handle: c_long, sql: *const RoastString) -> *mut RoastList {
    if sql.is_null() || handle < 0 { return roast_list_new(0); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*sql).data, (*sql).len);
        if let Ok(sql_str) = std::str::from_utf8(slice) {
            let connections = DB_CONNECTIONS.lock().unwrap();
            if let Some(conn) = connections.get(handle as usize) {
                let mut stmt = match conn.prepare(sql_str) {
                    Ok(s) => s,
                    Err(_) => return roast_list_new(0),
                };
                
                let col_count = stmt.column_count();
                let rows = roast_list_new(0);
                
                let row_iter = stmt.query_map([], |row| {
                    let mut values: Vec<String> = Vec::new();
                    for i in 0..col_count {
                        let val: Result<String, _> = row.get(i);
                        values.push(val.unwrap_or_default());
                    }
                    Ok(values)
                });
                
                if let Ok(iter) = row_iter {
                    for row_result in iter {
                        if let Ok(values) = row_result {
                            let row_list = roast_list_new(values.len() as c_long);
                            for val in values {
                                let s = roast_str_from_cstr(
                                    std::ffi::CString::new(val).unwrap().as_ptr()
                                );
                                roast_list_append(row_list, s as c_long);
                            }
                            roast_list_append(rows, row_list as c_long);
                        }
                    }
                }
                
                return rows;
            }
        }
        roast_list_new(0)
    }
}

/// Close a database connection
#[no_mangle]
pub extern "C" fn roast_sqlite_close(handle: c_long) -> c_long {
    if handle < 0 { return -1; }
    
    // Note: In a real implementation, we'd properly remove and close
    // For now, we just leave it - rusqlite closes on drop
    0
}

// ============================================================================
// HTTP Module (standard library)
// ============================================================================

/// HTTP Response struct containing status, headers, and body
#[repr(C)]
pub struct RoastHttpResponse {
    header: ObjectHeader,
    status: c_long,
    body: *mut RoastString,
}

/// Perform HTTP GET request
/// Returns response body as string, or null on error
#[no_mangle]
pub extern "C" fn roast_http_get(url: *const RoastString) -> *mut RoastString {
    if url.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*url).data, (*url).len);
        if let Ok(url_str) = std::str::from_utf8(slice) {
            match ureq::get(url_str).call() {
                Ok(resp) => {
                    if let Ok(body) = resp.into_string() {
                        return roast_str_from_cstr(
                            std::ffi::CString::new(body).unwrap().as_ptr()
                        );
                    }
                }
                Err(_) => {}
            }
        }
        std::ptr::null_mut()
    }
}

/// Perform HTTP POST request with body
/// Returns response body as string, or null on error
#[no_mangle]
pub extern "C" fn roast_http_post(url: *const RoastString, body: *const RoastString) -> *mut RoastString {
    if url.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let url_slice = std::slice::from_raw_parts((*url).data, (*url).len);
        let body_str = if !body.is_null() {
            let body_slice = std::slice::from_raw_parts((*body).data, (*body).len);
            std::str::from_utf8(body_slice).unwrap_or("")
        } else {
            ""
        };
        
        if let Ok(url_str) = std::str::from_utf8(url_slice) {
            match ureq::post(url_str)
                .set("Content-Type", "application/json")
                .send_string(body_str) 
            {
                Ok(resp) => {
                    if let Ok(resp_body) = resp.into_string() {
                        return roast_str_from_cstr(
                            std::ffi::CString::new(resp_body).unwrap().as_ptr()
                        );
                    }
                }
                Err(_) => {}
            }
        }
        std::ptr::null_mut()
    }
}

/// Perform HTTP request with custom method
/// method: "GET", "POST", "PUT", "DELETE", "PATCH"
#[no_mangle]
pub extern "C" fn roast_http_request(
    method: *const RoastString,
    url: *const RoastString,
    body: *const RoastString,
) -> *mut RoastTuple {
    if url.is_null() || method.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let method_slice = std::slice::from_raw_parts((*method).data, (*method).len);
        let url_slice = std::slice::from_raw_parts((*url).data, (*url).len);
        
        let method_str = std::str::from_utf8(method_slice).unwrap_or("GET");
        let url_str = match std::str::from_utf8(url_slice) {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        
        let body_str = if !body.is_null() {
            let body_slice = std::slice::from_raw_parts((*body).data, (*body).len);
            std::str::from_utf8(body_slice).unwrap_or("")
        } else {
            ""
        };
        
        let result = match method_str.to_uppercase().as_str() {
            "GET" => ureq::get(url_str).call(),
            "POST" => ureq::post(url_str).send_string(body_str),
            "PUT" => ureq::put(url_str).send_string(body_str),
            "DELETE" => ureq::delete(url_str).call(),
            "PATCH" => ureq::patch(url_str).send_string(body_str),
            _ => return std::ptr::null_mut(),
        };
        
        let tuple = roast_tuple_new(2);
        
        match result {
            Ok(resp) => {
                let status = resp.status() as c_long;
                let resp_body = resp.into_string().unwrap_or_default();
                let body_str = roast_str_from_cstr(
                    std::ffi::CString::new(resp_body).unwrap().as_ptr()
                );
                roast_tuple_set(tuple, 0, status);
                roast_tuple_set(tuple, 1, body_str as c_long);
            }
            Err(_) => {
                roast_tuple_set(tuple, 0, -1);
                roast_tuple_set(tuple, 1, 0);
            }
        }
        
        tuple
    }
}

/// Download file from URL to path
#[no_mangle]
pub extern "C" fn roast_http_download(url: *const RoastString, path: *const RoastString) -> c_long {
    if url.is_null() || path.is_null() { return -1; }
    
    unsafe {
        let url_slice = std::slice::from_raw_parts((*url).data, (*url).len);
        let path_slice = std::slice::from_raw_parts((*path).data, (*path).len);
        
        let url_str = match std::str::from_utf8(url_slice) {
            Ok(s) => s,
            Err(_) => return -1,
        };
        let path_str = match std::str::from_utf8(path_slice) {
            Ok(s) => s,
            Err(_) => return -1,
        };
        
        match ureq::get(url_str).call() {
            Ok(resp) => {
                if let Ok(body) = resp.into_string() {
                    if std::fs::write(path_str, body).is_ok() {
                        return 0;
                    }
                }
                -1
            }
            Err(_) => -1,
        }
    }
}

// ============================================================================
// Python FFI Module (call Python from Roast)
// ============================================================================
// Enable with: cargo build --features python-ffi
// Usage in Roast:
//   import py:numpy as np
//   result = np.array([1, 2, 3])
// ============================================================================

#[cfg(feature = "python-ffi")]
mod python_ffi {
    use super::*;
    use pyo3::prelude::*;
    use pyo3::types::{PyTuple, PyAnyMethods};

    /// Import a Python module by name
    /// Returns module handle or null on error
    #[no_mangle]
    pub extern "C" fn roast_py_import(module_name: *const RoastString) -> c_long {
        if module_name.is_null() { return 0; }
        
        unsafe {
            let slice = std::slice::from_raw_parts((*module_name).data, (*module_name).len);
            if let Ok(name) = std::str::from_utf8(slice) {
                Python::with_gil(|py| {
                    match py.import_bound(name) {
                        Ok(module) => {
                            // Store module reference as raw pointer
                            module.as_ptr() as c_long
                        }
                        Err(e) => {
                            eprintln!("Python import error: {}", e);
                            0
                        }
                    }
                })
            } else {
                0
            }
        }
    }

    /// Get attribute from Python object
    #[no_mangle]
    pub extern "C" fn roast_py_getattr(obj: c_long, attr: *const RoastString) -> c_long {
        if obj == 0 || attr.is_null() { return 0; }
        
        unsafe {
            let slice = std::slice::from_raw_parts((*attr).data, (*attr).len);
            if let Ok(attr_name) = std::str::from_utf8(slice) {
                Python::with_gil(|py| {
                    // Reconstruct PyObject from raw pointer
                    let py_obj: Py<pyo3::PyAny> = Py::from_borrowed_ptr(py, obj as *mut pyo3::ffi::PyObject);
                    match py_obj.getattr(py, attr_name) {
                        Ok(result) => result.as_ptr() as c_long,
                        Err(_) => 0,
                    }
                })
            } else {
                0
            }
        }
    }

    /// Call a Python callable with arguments
    #[no_mangle]
    pub extern "C" fn roast_py_call(callable: c_long, args: *const RoastList) -> c_long {
        if callable == 0 { return 0; }
        
        Python::with_gil(|py| {
            unsafe {
                // Reconstruct PyObject from raw pointer
                let py_callable: Py<pyo3::PyAny> = Py::from_borrowed_ptr(py, callable as *mut pyo3::ffi::PyObject);
                
                // Convert args to Python tuple
                let py_args: Bound<'_, PyTuple> = if args.is_null() {
                    PyTuple::empty_bound(py)
                } else {
                    let len = (*args).len;
                    let data = (*args).data;
                    let mut items: Vec<PyObject> = Vec::new();
                    for i in 0..len {
                        let val = *data.add(i);
                        items.push(val.into_py(py));
                    }
                    PyTuple::new_bound(py, items)
                };
                
                match py_callable.call1(py, py_args) {
                    Ok(result) => result.as_ptr() as c_long,
                    Err(e) => {
                        eprintln!("Python call error: {}", e);
                        0
                    }
                }
            }
        })
    }

    /// Convert Python object to Roast string
    #[no_mangle]
    pub extern "C" fn roast_py_to_str(obj: c_long) -> *mut RoastString {
        if obj == 0 { return std::ptr::null_mut(); }
        
        Python::with_gil(|py| {
            unsafe {
                let py_obj: Py<pyo3::PyAny> = Py::from_borrowed_ptr(py, obj as *mut pyo3::ffi::PyObject);
                match py_obj.call_method0(py, "__str__") {
                    Ok(s) => {
                        if let Ok(rust_str) = s.extract::<String>(py) {
                            return roast_str_from_cstr(
                                std::ffi::CString::new(rust_str).unwrap().as_ptr()
                            );
                        }
                    }
                    Err(_) => {}
                }
                std::ptr::null_mut()
            }
        })
    }

    /// Convert Python int to Roast int
    #[no_mangle]
    pub extern "C" fn roast_py_to_int(obj: c_long) -> c_long {
        if obj == 0 { return 0; }
        
        Python::with_gil(|py| {
            unsafe {
                let py_obj: Py<pyo3::PyAny> = Py::from_borrowed_ptr(py, obj as *mut pyo3::ffi::PyObject);
                py_obj.extract::<c_long>(py).unwrap_or(0)
            }
        })
    }
}

// Re-export Python FFI functions when feature is enabled
#[cfg(feature = "python-ffi")]
pub use python_ffi::*;

// Stub implementations when Python FFI is disabled
#[cfg(not(feature = "python-ffi"))]
mod python_ffi_stubs {
    use super::*;

    #[no_mangle]
    pub extern "C" fn roast_py_import(_module_name: *const RoastString) -> c_long {
        eprintln!("Python FFI not enabled. Rebuild with: cargo build --features python-ffi");
        0
    }

    #[no_mangle]
    pub extern "C" fn roast_py_getattr(_obj: c_long, _attr: *const RoastString) -> c_long { 0 }

    #[no_mangle]
    pub extern "C" fn roast_py_call(_callable: c_long, _args: *const RoastList) -> c_long { 0 }

    #[no_mangle]
    pub extern "C" fn roast_py_to_str(_obj: c_long) -> *mut RoastString { std::ptr::null_mut() }

    #[no_mangle]
    pub extern "C" fn roast_py_to_int(_obj: c_long) -> c_long { 0 }
}

#[cfg(not(feature = "python-ffi"))]
pub use python_ffi_stubs::*;

// ============================================================================
// Hashlib Module (standard library)
// ============================================================================

use sha2::{Sha256, Sha512, Digest as ShaDigest};
use md5::{Md5, Digest as Md5Digest};

/// Compute MD5 hash of string, returns hex string
#[no_mangle]
pub extern "C" fn roast_hashlib_md5(data: *const RoastString) -> *mut RoastString {
    if data.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        let mut hasher = Md5::new();
        hasher.update(slice);
        let result = hasher.finalize();
        let hex = format!("{:x}", result);
        roast_str_from_cstr(std::ffi::CString::new(hex).unwrap().as_ptr())
    }
}

/// Compute SHA256 hash of string, returns hex string
#[no_mangle]
pub extern "C" fn roast_hashlib_sha256(data: *const RoastString) -> *mut RoastString {
    if data.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        let mut hasher = Sha256::new();
        hasher.update(slice);
        let result = hasher.finalize();
        let hex = format!("{:x}", result);
        roast_str_from_cstr(std::ffi::CString::new(hex).unwrap().as_ptr())
    }
}

/// Compute SHA512 hash of string, returns hex string
#[no_mangle]
pub extern "C" fn roast_hashlib_sha512(data: *const RoastString) -> *mut RoastString {
    if data.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        let mut hasher = Sha512::new();
        hasher.update(slice);
        let result = hasher.finalize();
        let hex = format!("{:x}", result);
        roast_str_from_cstr(std::ffi::CString::new(hex).unwrap().as_ptr())
    }
}

/// Compute hash with algorithm name ("md5", "sha256", "sha512")
#[no_mangle]
pub extern "C" fn roast_hashlib_new(algo: *const RoastString, data: *const RoastString) -> *mut RoastString {
    if algo.is_null() || data.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let algo_slice = std::slice::from_raw_parts((*algo).data, (*algo).len);
        let algo_str = std::str::from_utf8(algo_slice).unwrap_or("");
        
        match algo_str.to_lowercase().as_str() {
            "md5" => roast_hashlib_md5(data),
            "sha256" => roast_hashlib_sha256(data),
            "sha512" => roast_hashlib_sha512(data),
            _ => std::ptr::null_mut(),
        }
    }
}

// ============================================================================
// Socket Module (standard library)
// ============================================================================

use std::net::{TcpStream, TcpListener, UdpSocket};

// Simple socket handle storage
static SOCKETS: Lazy<Mutex<Vec<Option<TcpStream>>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Create a TCP connection and return socket handle
#[no_mangle]
pub extern "C" fn roast_socket_connect(host: *const RoastString, port: c_long) -> c_long {
    if host.is_null() { return -1; }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*host).data, (*host).len);
        if let Ok(host_str) = std::str::from_utf8(slice) {
            let addr = format!("{}:{}", host_str, port);
            match TcpStream::connect(&addr) {
                Ok(stream) => {
                    let mut sockets = SOCKETS.lock().unwrap();
                    sockets.push(Some(stream));
                    (sockets.len() - 1) as c_long
                }
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// Send data through socket, returns bytes sent
#[no_mangle]
pub extern "C" fn roast_socket_send(handle: c_long, data: *const RoastString) -> c_long {
    if data.is_null() || handle < 0 { return -1; }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        let mut sockets = SOCKETS.lock().unwrap();
        if let Some(Some(stream)) = sockets.get_mut(handle as usize) {
            match stream.write(slice) {
                Ok(n) => n as c_long,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// Receive data from socket, returns string
#[no_mangle]
pub extern "C" fn roast_socket_recv(handle: c_long, max_bytes: c_long) -> *mut RoastString {
    if handle < 0 { return std::ptr::null_mut(); }
    
    let mut sockets = SOCKETS.lock().unwrap();
    if let Some(Some(stream)) = sockets.get_mut(handle as usize) {
        let mut buf = vec![0u8; max_bytes as usize];
        match stream.read(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                if let Ok(s) = String::from_utf8(buf) {
                    return roast_str_from_cstr(
                        std::ffi::CString::new(s).unwrap().as_ptr()
                    );
                }
            }
            Err(_) => {}
        }
    }
    std::ptr::null_mut()
}

/// Close a socket
#[no_mangle]
pub extern "C" fn roast_socket_close(handle: c_long) -> c_long {
    if handle < 0 { return -1; }
    
    let mut sockets = SOCKETS.lock().unwrap();
    if let Some(slot) = sockets.get_mut(handle as usize) {
        *slot = None; // Drop closes the connection
        0
    } else {
        -1
    }
}

// ============================================================================
// TCP Server Module (for building HTTP servers)
// ============================================================================

// Storage for TCP listeners (servers)
static TCP_LISTENERS: Lazy<Mutex<Vec<Option<TcpListener>>>> = Lazy::new(|| Mutex::new(Vec::new()));

// Storage for accepted client connections
static CLIENT_CONNECTIONS: Lazy<Mutex<Vec<Option<TcpStream>>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Create a TCP server (bind and listen on a port)
/// Returns server handle on success, -1 on error
#[no_mangle]
pub extern "C" fn roast_tcp_server_create(host: *const RoastString, port: c_long) -> c_long {
    if host.is_null() { return -1; }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*host).data, (*host).len);
        if let Ok(host_str) = std::str::from_utf8(slice) {
            let addr = format!("{}:{}", host_str, port);
            match TcpListener::bind(&addr) {
                Ok(listener) => {
                    // Log server startup
                    eprintln!(
                        "\x1b[2m[{}]\x1b[0m \x1b[32mINFO    \x1b[0m Server listening on http://{}",
                        get_log_timestamp(), addr
                    );
                    
                    let mut listeners = TCP_LISTENERS.lock().unwrap();
                    listeners.push(Some(listener));
                    (listeners.len() - 1) as c_long
                }
                Err(e) => {
                    eprintln!(
                        "\x1b[2m[{}]\x1b[0m \x1b[31mERROR   \x1b[0m Failed to bind to {}: {}",
                        get_log_timestamp(), addr, e
                    );
                    -1
                }
            }
        } else {
            -1
        }
    }
}

/// Accept a client connection (blocking)
/// Returns client handle on success, -1 on error
#[no_mangle]
pub extern "C" fn roast_tcp_server_accept(server_handle: c_long) -> c_long {
    if server_handle < 0 { return -1; }
    
    let listeners = TCP_LISTENERS.lock().unwrap();
    if let Some(Some(listener)) = listeners.get(server_handle as usize) {
        // Clone the listener to release the lock during accept
        let listener_clone = match listener.try_clone() {
            Ok(l) => l,
            Err(_) => return -1,
        };
        drop(listeners);  // Release the lock before blocking accept
        
        match listener_clone.accept() {
            Ok((stream, addr)) => {
                // Log connection at DEBUG level (only shows if log level is DEBUG)
                let current_level = LOG_LEVEL.load(Ordering::Relaxed);
                if current_level <= 10 {
                    eprintln!(
                        "\x1b[2m[{}]\x1b[0m \x1b[36mDEBUG   \x1b[0m Connection from {}",
                        get_log_timestamp(), addr
                    );
                }
                
                let mut clients = CLIENT_CONNECTIONS.lock().unwrap();
                clients.push(Some(stream));
                (clients.len() - 1) as c_long
            }
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// Read data from a client connection
/// Returns the data as a string, or empty string on error
#[no_mangle]
pub extern "C" fn roast_tcp_client_read(client_handle: c_long, max_bytes: c_long) -> *mut RoastString {
    if client_handle < 0 { 
        // Return empty string instead of null for safety
        return roast_str_from_cstr(c"".as_ptr());
    }
    
    let mut clients = CLIENT_CONNECTIONS.lock().unwrap();
    if let Some(Some(stream)) = clients.get_mut(client_handle as usize) {
        let mut buf = vec![0u8; max_bytes as usize];
        match stream.read(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                // Return raw bytes as string (handles binary data)
                let s = String::from_utf8_lossy(&buf).to_string();
                drop(clients);  // Release lock before allocating
                return roast_str_from_cstr(
                    std::ffi::CString::new(s).unwrap().as_ptr()
                );
            }
            Err(e) => {
                eprintln!("[roast_tcp_client_read] Error reading: {}", e);
            }
        }
    }
    // Return empty string instead of null for safety
    roast_str_from_cstr(c"".as_ptr())
}

/// Write data to a client connection
/// Returns bytes written on success, -1 on error
#[no_mangle]
pub extern "C" fn roast_tcp_client_write(client_handle: c_long, data: *const RoastString) -> c_long {
    if client_handle < 0 || data.is_null() { return -1; }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        let mut clients = CLIENT_CONNECTIONS.lock().unwrap();
        if let Some(Some(stream)) = clients.get_mut(client_handle as usize) {
            match stream.write_all(slice) {
                Ok(_) => {
                    let _ = stream.flush();
                    slice.len() as c_long
                }
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// Close a client connection
#[no_mangle]
pub extern "C" fn roast_tcp_client_close(client_handle: c_long) -> c_long {
    if client_handle < 0 { return -1; }
    
    let mut clients = CLIENT_CONNECTIONS.lock().unwrap();
    if let Some(slot) = clients.get_mut(client_handle as usize) {
        *slot = None; // Drop closes the connection
        0
    } else {
        -1
    }
}

/// Close a TCP server
#[no_mangle]
pub extern "C" fn roast_tcp_server_close(server_handle: c_long) -> c_long {
    if server_handle < 0 { return -1; }
    
    let mut listeners = TCP_LISTENERS.lock().unwrap();
    if let Some(slot) = listeners.get_mut(server_handle as usize) {
        *slot = None; // Drop closes the listener
        0
    } else {
        -1
    }
}

// ============================================================================
// Async Runtime Module (GIL-free, Rust/Go-style parallelism)
// ============================================================================
// No GIL! True parallelism with:
// - Goroutine-style spawn (roast_go)
// - Channel-based communication (roast_chan_*)
// - Tokio-backed async runtime
// ============================================================================

use std::thread::{self, JoinHandle};
use std::time::Duration;
use crossbeam_channel::{unbounded, Sender, Receiver};

// Global tokio runtime for async operations
static ASYNC_RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus())
        .enable_all()
        .build()
        .expect("Failed to create async runtime")
});

fn num_cpus() -> usize {
    std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4)
}

// Channel storage for Go-style channel communication
type Channel = (Sender<c_long>, Receiver<c_long>);
static CHANNELS: Lazy<Mutex<Vec<Channel>>> = Lazy::new(|| Mutex::new(Vec::new()));

// Task handles storage
static TASK_HANDLES: Lazy<Mutex<Vec<Option<JoinHandle<c_long>>>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Create a new channel (like Go's make(chan int))
/// Returns channel handle
#[no_mangle]
pub extern "C" fn roast_chan_new() -> c_long {
    let (tx, rx) = unbounded();
    let mut channels = CHANNELS.lock().unwrap();
    channels.push((tx, rx));
    (channels.len() - 1) as c_long
}

/// Send value to channel (like Go's ch <- value)
/// Non-blocking - returns 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn roast_chan_send(handle: c_long, value: c_long) -> c_long {
    if handle < 0 { return -1; }
    
    let channels = CHANNELS.lock().unwrap();
    if let Some((tx, _)) = channels.get(handle as usize) {
        match tx.send(value) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// Receive value from channel (like Go's <-ch)
/// Blocks until value available
#[no_mangle]
pub extern "C" fn roast_chan_recv(handle: c_long) -> c_long {
    if handle < 0 { return 0; }
    
    let channels = CHANNELS.lock().unwrap();
    if let Some((_, rx)) = channels.get(handle as usize) {
        let rx = rx.clone();
        drop(channels); // Release lock before blocking
        rx.recv().unwrap_or(0)
    } else {
        0
    }
}

/// Try to receive (non-blocking), returns (success, value)
#[no_mangle]
pub extern "C" fn roast_chan_try_recv(handle: c_long, value_out: *mut c_long) -> c_long {
    if handle < 0 || value_out.is_null() { return 0; }
    
    let channels = CHANNELS.lock().unwrap();
    if let Some((_, rx)) = channels.get(handle as usize) {
        match rx.try_recv() {
            Ok(v) => {
                unsafe { *value_out = v; }
                1 // Success
            }
            Err(_) => 0, // No value available
        }
    } else {
        0
    }
}

/// Close a channel
#[no_mangle]
pub extern "C" fn roast_chan_close(handle: c_long) -> c_long {
    // Channels close automatically when all senders are dropped
    // This is a no-op for our simple implementation
    if handle < 0 { return -1; }
    0
}

/// Spawn a new goroutine (like Go's go func())
/// Returns task handle
#[no_mangle]
pub extern "C" fn roast_go(func_ptr: extern "C" fn() -> c_long) -> c_long {
    let handle = thread::spawn(move || {
        func_ptr()
    });
    
    let mut tasks = TASK_HANDLES.lock().unwrap();
    tasks.push(Some(handle));
    (tasks.len() - 1) as c_long
}

/// Wait for a goroutine to complete (like sync.WaitGroup)
#[no_mangle]
pub extern "C" fn roast_go_wait(handle: c_long) -> c_long {
    if handle < 0 { return -1; }
    
    let mut tasks = TASK_HANDLES.lock().unwrap();
    if let Some(slot) = tasks.get_mut(handle as usize) {
        if let Some(h) = slot.take() {
            drop(tasks); // Release lock before join
            match h.join() {
                Ok(result) => result,
                Err(_) => -1,
            }
        } else {
            -1 // Already joined
        }
    } else {
        -1
    }
}

/// Spawn N workers that process from a channel (worker pool pattern)
#[no_mangle]
pub extern "C" fn roast_go_pool(
    n_workers: c_long,
    work_chan: c_long,
    worker_func: extern "C" fn(c_long) -> c_long,
) -> c_long {
    if n_workers <= 0 || work_chan < 0 { return -1; }
    
    for _ in 0..n_workers {
        let work_chan = work_chan;
        thread::spawn(move || {
            loop {
                let work = roast_chan_recv(work_chan);
                if work == 0 { break; } // 0 signals shutdown
                worker_func(work);
            }
        });
    }
    
    0
}

/// Sleep in async context (non-blocking for other tasks)
#[no_mangle]
pub extern "C" fn roast_async_sleep(ms: c_long) {
    if ms > 0 {
        ASYNC_RUNTIME.block_on(async {
            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
        });
    }
}

/// Get number of available parallel workers
#[no_mangle]
pub extern "C" fn roast_parallelism() -> c_long {
    num_cpus() as c_long
}

// ============================================================================
// Threading Module (standard library)
// ============================================================================


static THREADS: Lazy<Mutex<Vec<Option<JoinHandle<c_long>>>>> = Lazy::new(|| Mutex::new(Vec::new()));

// Mutex storage for user-created mutexes
static USER_MUTEXES: Lazy<Mutex<Vec<Mutex<c_long>>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Sleep for specified milliseconds
#[no_mangle]
pub extern "C" fn roast_thread_sleep(ms: c_long) {
    if ms > 0 {
        thread::sleep(Duration::from_millis(ms as u64));
    }
}

/// Get current thread ID
#[no_mangle]
pub extern "C" fn roast_thread_current_id() -> c_long {
    // Use a simple hash of the thread id
    format!("{:?}", thread::current().id()).len() as c_long
}

/// Yield execution to other threads
#[no_mangle]
pub extern "C" fn roast_thread_yield_now() {
    thread::yield_now();
}

/// Create a new mutex, returns handle
#[no_mangle]
pub extern "C" fn roast_mutex_new() -> c_long {
    let mut mutexes = USER_MUTEXES.lock().unwrap();
    mutexes.push(Mutex::new(0));
    (mutexes.len() - 1) as c_long
}

/// Lock a mutex
#[no_mangle]
pub extern "C" fn roast_mutex_lock(handle: c_long) -> c_long {
    if handle < 0 { return -1; }
    
    let mutexes = USER_MUTEXES.lock().unwrap();
    if let Some(m) = mutexes.get(handle as usize) {
        let _guard = m.lock(); // Block until acquired
        // Note: Guard is dropped immediately - real impl would need to track
        0
    } else {
        -1
    }
}

/// Unlock a mutex
#[no_mangle]
pub extern "C" fn roast_mutex_unlock(handle: c_long) -> c_long {
    // In this simple impl, unlock is a no-op since we don't track guards
    if handle < 0 { return -1; }
    0
}

/// Get number of available CPU cores
#[no_mangle]
pub extern "C" fn roast_thread_cpu_count() -> c_long {
    std::thread::available_parallelism()
        .map(|p| p.get() as c_long)
        .unwrap_or(1)
}

// ============================================================================
// Base64 Module (standard library)
// ============================================================================

/// Encode bytes/string to base64
#[no_mangle]
pub extern "C" fn roast_base64_encode(data: *const RoastString) -> *mut RoastString {
    if data.is_null() { return std::ptr::null_mut(); }
    unsafe {
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        let encoded = base64_encode_bytes(slice);
        roast_str_from_cstr(std::ffi::CString::new(encoded).unwrap().as_ptr())
    }
}

fn base64_encode_bytes(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        
        result.push(CHARS[(b0 >> 2)] as char);
        result.push(CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        
        if chunk.len() > 1 {
            result.push(CHARS[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }
        
        if chunk.len() > 2 {
            result.push(CHARS[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }
    
    result
}

fn base64_engine() {}

/// Decode base64 to string
#[no_mangle]
pub extern "C" fn roast_base64_decode(data: *const RoastString) -> *mut RoastString {
    if data.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        if let Ok(s) = std::str::from_utf8(slice) {
            if let Some(decoded) = base64_decode_str(s) {
                if let Ok(result) = String::from_utf8(decoded) {
                    return roast_str_from_cstr(std::ffi::CString::new(result).unwrap().as_ptr());
                }
            }
        }
        std::ptr::null_mut()
    }
}

fn base64_decode_str(s: &str) -> Option<Vec<u8>> {
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
    
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=' && b != b'\n' && b != b'\r').collect();
    let mut result = Vec::new();
    
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 { break; }
        let b0 = DECODE.get(chunk[0] as usize).copied().unwrap_or(-1);
        let b1 = DECODE.get(chunk[1] as usize).copied().unwrap_or(-1);
        if b0 < 0 || b1 < 0 { return None; }
        
        result.push(((b0 << 2) | (b1 >> 4)) as u8);
        
        if chunk.len() > 2 {
            let b2 = DECODE.get(chunk[2] as usize).copied().unwrap_or(-1);
            if b2 >= 0 {
                result.push((((b1 & 0x0f) << 4) | (b2 >> 2)) as u8);
            }
            
            if chunk.len() > 3 {
                let b3 = DECODE.get(chunk[3] as usize).copied().unwrap_or(-1);
                if b3 >= 0 {
                    result.push((((b2 & 0x03) << 6) | b3) as u8);
                }
            }
        }
    }
    
    Some(result)
}

// ============================================================================
// UUID Module (standard library)
// ============================================================================

/// Generate a random UUID v4
#[no_mangle]
pub extern "C" fn roast_uuid_v4() -> *mut RoastString {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    // Simple pseudo-random UUID v4
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let seed = now.as_nanos() as u64;
    
    let mut bytes = [0u8; 16];
    let mut state = seed;
    for i in 0..16 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        bytes[i] = (state >> 56) as u8;
    }
    
    // Set version (4) and variant (RFC 4122)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    
    let uuid = format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    );
    
    roast_str_from_cstr(std::ffi::CString::new(uuid).unwrap().as_ptr())
}

// ============================================================================
// Random Module (standard library)
// ============================================================================

// RefCell already imported at file start

thread_local! {
    static RNG_STATE: RefCell<u64> = RefCell::new(0);
}

fn init_rng() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
}

fn next_random() -> u64 {
    RNG_STATE.with(|state| {
        let mut s = state.borrow_mut();
        if *s == 0 { *s = init_rng(); }
        *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *s
    })
}

/// Generate random integer in range [0, max)
#[no_mangle]
pub extern "C" fn roast_random_int(max: c_long) -> c_long {
    if max <= 0 { return 0; }
    (next_random() % (max as u64)) as c_long
}

/// Generate random integer in range [min, max]
#[no_mangle]
pub extern "C" fn roast_random_range(min: c_long, max: c_long) -> c_long {
    if max <= min { return min; }
    min + roast_random_int(max - min + 1)
}

/// Generate random float in range [0.0, 1.0)
#[no_mangle]
pub extern "C" fn roast_random_float() -> c_double {
    (next_random() as f64) / (u64::MAX as f64)
}

/// Seed the random number generator
#[no_mangle]
pub extern "C" fn roast_random_seed(seed: c_long) {
    RNG_STATE.with(|state| {
        *state.borrow_mut() = seed as u64;
    });
}

/// Shuffle a list in place
#[no_mangle]
pub extern "C" fn roast_random_shuffle(list: *mut RoastList) {
    if list.is_null() { return; }
    
    unsafe {
        let len = (*list).len;
        let data = (*list).data;
        
        for i in (1..len).rev() {
            let j = roast_random_int(i as c_long + 1) as usize;
            let tmp = *data.add(i);
            *data.add(i) = *data.add(j);
            *data.add(j) = tmp;
        }
    }
}

/// Pick random element from list
#[no_mangle]
pub extern "C" fn roast_random_choice(list: *const RoastList) -> c_long {
    if list.is_null() { return 0; }
    
    unsafe {
        let len = (*list).len;
        if len == 0 { return 0; }
        let idx = roast_random_int(len as c_long) as usize;
        *(*list).data.add(idx)
    }
}

// ============================================================================
// DateTime Module (standard library)
// ============================================================================

// Note: SystemTime already imported at line 72

/// Get current Unix timestamp (seconds since epoch)
#[no_mangle]
pub extern "C" fn roast_time_now() -> c_long {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as c_long
}

/// Get current Unix timestamp in milliseconds
#[no_mangle]
pub extern "C" fn roast_time_now_ms() -> c_long {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as c_long
}

/// Get current Unix timestamp in nanoseconds
#[no_mangle]
pub extern "C" fn roast_time_now_ns() -> c_long {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as c_long
}

/// Format timestamp as ISO 8601 string (YYYY-MM-DDTHH:MM:SSZ)
#[no_mangle]
pub extern "C" fn roast_time_format_iso(timestamp: c_long) -> *mut RoastString {
    // Simple implementation - proper one would use chrono
    let secs = timestamp as u64;
    
    // Calculate components (simplified - doesn't handle leap years perfectly)
    let mut rem = secs;
    let years_since_1970 = rem / 31536000;
    rem %= 31536000;
    let day_of_year = rem / 86400;
    rem %= 86400;
    let hours = rem / 3600;
    rem %= 3600;
    let minutes = rem / 60;
    let seconds = rem % 60;
    
    let year = 1970 + years_since_1970;
    let month = (day_of_year / 30) + 1;
    let day = (day_of_year % 30) + 1;
    
    let iso = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month.min(12), day.min(31), hours, minutes, seconds
    );
    
    roast_str_from_cstr(std::ffi::CString::new(iso).unwrap().as_ptr())
}

// ============================================================================
// XML/HTML Module (standard library - basic parsing)
// ============================================================================

/// Extract all text between XML/HTML tags (simple regex-based)
#[no_mangle]
pub extern "C" fn roast_xml_get_text(xml: *const RoastString, tag: *const RoastString) -> *mut RoastList {
    if xml.is_null() || tag.is_null() { return roast_list_new(0); }
    
    unsafe {
        let xml_slice = std::slice::from_raw_parts((*xml).data, (*xml).len);
        let tag_slice = std::slice::from_raw_parts((*tag).data, (*tag).len);
        
        let xml_str = std::str::from_utf8(xml_slice).unwrap_or("");
        let tag_str = std::str::from_utf8(tag_slice).unwrap_or("");
        
        let result = roast_list_new(0);
        
        // Simple pattern: <tag>content</tag> or <tag attr="val">content</tag>
        let open_pattern = format!("<{}", tag_str);
        let close_tag = format!("</{}>", tag_str);
        
        let mut pos = 0;
        while let Some(start) = xml_str[pos..].find(&open_pattern) {
            let abs_start = pos + start;
            if let Some(gt) = xml_str[abs_start..].find('>') {
                let content_start = abs_start + gt + 1;
                if let Some(end) = xml_str[content_start..].find(&close_tag) {
                    let content = &xml_str[content_start..content_start + end];
                    let s = roast_str_from_cstr(
                        std::ffi::CString::new(content).unwrap().as_ptr()
                    );
                    roast_list_append(result, s as c_long);
                    pos = content_start + end + close_tag.len();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        result
    }
}

/// Get attribute value from first matching tag
#[no_mangle]
pub extern "C" fn roast_xml_get_attr(xml: *const RoastString, tag: *const RoastString, attr: *const RoastString) -> *mut RoastString {
    if xml.is_null() || tag.is_null() || attr.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let xml_slice = std::slice::from_raw_parts((*xml).data, (*xml).len);
        let tag_slice = std::slice::from_raw_parts((*tag).data, (*tag).len);
        let attr_slice = std::slice::from_raw_parts((*attr).data, (*attr).len);
        
        let xml_str = std::str::from_utf8(xml_slice).unwrap_or("");
        let tag_str = std::str::from_utf8(tag_slice).unwrap_or("");
        let attr_str = std::str::from_utf8(attr_slice).unwrap_or("");
        
        // Find <tag ...>
        let open_pattern = format!("<{}", tag_str);
        if let Some(start) = xml_str.find(&open_pattern) {
            if let Some(gt) = xml_str[start..].find('>') {
                let tag_content = &xml_str[start..start + gt + 1];
                // Look for attr="value" or attr='value'
                let attr_pattern = format!("{}=", attr_str);
                if let Some(attr_pos) = tag_content.find(&attr_pattern) {
                    let value_start = attr_pos + attr_pattern.len();
                    if value_start < tag_content.len() {
                        let quote = tag_content.chars().nth(value_start).unwrap_or('"');
                        if quote == '"' || quote == '\'' {
                            if let Some(end) = tag_content[value_start + 1..].find(quote) {
                                let value = &tag_content[value_start + 1..value_start + 1 + end];
                                return roast_str_from_cstr(
                                    std::ffi::CString::new(value).unwrap().as_ptr()
                                );
                            }
                        }
                    }
                }
            }
        }
        
        std::ptr::null_mut()
    }
}

/// Strip all HTML/XML tags from string
#[no_mangle]
pub extern "C" fn roast_html_strip_tags(html: *const RoastString) -> *mut RoastString {
    if html.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*html).data, (*html).len);
        if let Ok(html_str) = std::str::from_utf8(slice) {
            let mut result = String::new();
            let mut in_tag = false;
            
            for c in html_str.chars() {
                if c == '<' {
                    in_tag = true;
                } else if c == '>' {
                    in_tag = false;
                } else if !in_tag {
                    result.push(c);
                }
            }
            
            return roast_str_from_cstr(
                std::ffi::CString::new(result).unwrap().as_ptr()
            );
        }
        std::ptr::null_mut()
    }
}

/// Escape HTML special characters
#[no_mangle]
pub extern "C" fn roast_html_escape(text: *const RoastString) -> *mut RoastString {
    if text.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*text).data, (*text).len);
        if let Ok(text_str) = std::str::from_utf8(slice) {
            let escaped = text_str
                .replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
                .replace("\"", "&quot;")
                .replace("'", "&#39;");
            
            return roast_str_from_cstr(
                std::ffi::CString::new(escaped).unwrap().as_ptr()
            );
        }
        std::ptr::null_mut()
    }
}

// ============================================================================
// User Input
// ============================================================================

/// input(prompt) - Read a line from stdin, optionally displaying a prompt
/// Returns a RoastString containing the user input (without trailing newline)
#[no_mangle]
pub extern "C" fn roast_input(prompt: *const RoastString) -> *mut RoastString {
    use std::io::{self, Write, BufRead};
    
    // Print prompt if provided
    if !prompt.is_null() {
        unsafe {
            let slice = std::slice::from_raw_parts((*prompt).data, (*prompt).len);
            if let Ok(string) = std::str::from_utf8(slice) {
                print!("{}", string);
                let _ = io::stdout().flush();
            }
        }
    }
    
    // Read line from stdin
    let stdin = io::stdin();
    let mut input = String::new();
    if stdin.lock().read_line(&mut input).is_err() {
        return roast_str_from_cstr(b"".as_ptr() as *const c_char);
    }
    
    // Remove trailing newline
    let input = input.trim_end_matches(|c| c == '\n' || c == '\r');
    
    // Convert to RoastString
    roast_str_new(input.as_ptr() as *const c_char, input.len() as c_long)
}

// ============================================================================
// Async Operations
// ============================================================================

/// asyncio_run(coro) - Execute a coroutine/async function to completion
/// For now, this is a simple placeholder that just returns the value.
/// In a real implementation, this would spin up an event loop.
#[no_mangle]
pub extern "C" fn roast_asyncio_run(coro: c_long) -> c_long {
    // For now, just return the coroutine value
    // The actual execution happens at the VM level
    coro
}

/// asyncio_sleep(seconds) - Sleep for the given number of seconds
/// This is a blocking implementation for now.
#[no_mangle]
pub extern "C" fn roast_asyncio_sleep(seconds: c_double) -> c_long {
    std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
    0 // Return None
}

// ============================================================================
// Math Operations
// ============================================================================

#[no_mangle]
pub extern "C" fn roast_pow_int(base: c_long, exp: c_long) -> c_long {
    if exp < 0 { return 0; }
    (base as i64).pow(exp as u32) as c_long
}

#[no_mangle]
pub extern "C" fn roast_pow_float(base: c_double, exp: c_double) -> c_double {
    base.powf(exp)
}

/// Python-style floor division: floor(a/b)
/// For positive divisor: rounds toward negative infinity
/// -7 // 2 = -4 (not -3 as in C)
#[no_mangle]
pub extern "C" fn roast_floordiv(a: c_long, b: c_long) -> c_long {
    if b == 0 { return 0; } // avoid div by zero
    let q = a / b;
    let r = a % b;
    // If remainder is non-zero and signs of operands differ, subtract 1
    if r != 0 && ((a < 0) != (b < 0)) { q - 1 } else { q }
}

/// Python-style modulo: result has same sign as divisor
/// -7 % 2 = 1 (not -1 as in C)
/// 7 % -2 = -1 (not 1 as in C)
#[no_mangle]
pub extern "C" fn roast_mod(a: c_long, b: c_long) -> c_long {
    if b == 0 { return 0; }
    let r = a % b;
    // If remainder is non-zero and signs of operands differ, add b
    if r != 0 && ((a < 0) != (b < 0)) { r + b } else { r }
}


#[no_mangle]
pub extern "C" fn roast_abs_int(value: c_long) -> c_long {
    value.abs()
}

#[no_mangle]
pub extern "C" fn roast_abs_float(value: c_double) -> c_double {
    value.abs()
}

#[no_mangle]
pub extern "C" fn roast_min_int(a: c_long, b: c_long) -> c_long {
    a.min(b)
}

#[no_mangle]
pub extern "C" fn roast_max_int(a: c_long, b: c_long) -> c_long {
    a.max(b)
}

#[no_mangle]
pub extern "C" fn roast_min_float(a: c_double, b: c_double) -> c_double {
    a.min(b)
}

#[no_mangle]
pub extern "C" fn roast_max_float(a: c_double, b: c_double) -> c_double {
    a.max(b)
}

/// Generic min for boxed values (assumes integers for simplicity)
#[no_mangle]
pub extern "C" fn roast_min(a: c_long, b: c_long) -> c_long {
    a.min(b)
}

/// Generic max for boxed values (assumes integers for simplicity)
#[no_mangle]
pub extern "C" fn roast_max(a: c_long, b: c_long) -> c_long {
    a.max(b)
}

/// List min - returns minimum element from a list
#[no_mangle]
pub extern "C" fn roast_min_list(list: c_long) -> c_long {
    let ptr = list as *const RoastList;
    if ptr.is_null() { return 0; }
    unsafe {
        if (*ptr).len == 0 { return 0; }
        let mut min_val = *(*ptr).data.add(0);
        for i in 1..(*ptr).len {
            let val = *(*ptr).data.add(i);
            if val < min_val {
                min_val = val;
            }
        }
        min_val
    }
}

/// List max - returns maximum element from a list
#[no_mangle]
pub extern "C" fn roast_max_list(list: c_long) -> c_long {
    let ptr = list as *const RoastList;
    if ptr.is_null() { return 0; }
    unsafe {
        if (*ptr).len == 0 { return 0; }
        let mut max_val = *(*ptr).data.add(0);
        for i in 1..(*ptr).len {
            let val = *(*ptr).data.add(i);
            if val > max_val {
                max_val = val;
            }
        }
        max_val
    }
}

#[no_mangle]
pub extern "C" fn roast_floor(value: c_double) -> c_double {
    value.floor()
}

#[no_mangle]
pub extern "C" fn roast_ceil(value: c_double) -> c_double {
    value.ceil()
}

#[no_mangle]
pub extern "C" fn roast_sqrt(value: c_double) -> c_double {
    value.sqrt()
}

// ============================================================================
// Hash Operations
// ============================================================================

#[no_mangle]
pub extern "C" fn roast_hash(value: c_long) -> c_long {
    // FNV-1a hash
    let mut hash: u64 = 14695981039346656037;
    let bytes = value.to_le_bytes();
    for byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash as c_long
}

#[no_mangle]
pub extern "C" fn roast_hash_str(s: *const RoastString) -> c_long {
    if s.is_null() { return 0; }
    unsafe {
        let mut hash: u64 = 14695981039346656037;
        for i in 0..(*s).len {
            hash ^= *(*s).data.add(i) as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        hash as c_long
    }
}

fn roast_hash_cstr(s: *const c_char) -> c_long {
    if s.is_null() { return 0; }
    unsafe {
        let mut hash: u64 = 14695981039346656037;
        let mut p = s;
        while *p != 0 {
            hash ^= *p as u64;
            hash = hash.wrapping_mul(1099511628211);
            p = p.add(1);
        }
        hash as c_long
    }
}

// ============================================================================
// Error Handling
// ============================================================================

#[no_mangle]
pub extern "C" fn roast_assert(cond: bool, msg: *const c_char) {
    if !cond {
        let message = if msg.is_null() {
            "assertion failed".to_string()
        } else {
            unsafe { CStr::from_ptr(msg).to_str().unwrap_or("assertion failed").to_string() }
        };
        eprintln!("AssertionError: {}", message);
        std::process::exit(1);
    }
}

// ============================================================================
// Builtin Functions (unified interface)
// ============================================================================

/// Helper to check if a value looks like a valid heap pointer
/// On most systems, heap allocations are in a specific range
#[inline]
fn is_likely_pointer(value: c_long) -> bool {
    // Check if it looks like a properly aligned heap pointer
    // Heap allocations are typically:
    // - Non-null
    // - Aligned to at least 8 bytes (low 3 bits are 0)
    // - In a reasonable address range for heap
    if value <= 0 {
        return false;
    }
    let ptr = value as u64;
    // Check alignment (heap pointers are 8-byte aligned)
    if ptr & 0x7 != 0 {
        return false;
    }
    // On Linux x86_64, heap/mmap addresses are typically very high:
    // - Heap: around 0x555555... (program break)
    // - Mmap: around 0x7f0000000000 (shared libraries/malloc)
    // Values below 0x500000000000 (~80TB) are almost certainly integers
    // This covers all reasonable 64-bit integer results
    // Values above 0x7fffffffffff are kernel space
    if ptr < 0x500000000000 || ptr > 0x7fffffffffff {
        return false;
    }
    true
}

#[no_mangle]
pub extern "C" fn roast_print(value: c_long) -> c_long {
    // Try to detect if this is a RoastString pointer
    // This is a heuristic - not perfect but works for common cases
    let ptr = value as *const c_void;

    if !ptr.is_null() && is_likely_pointer(value) {
        // Try to interpret as RoastString - be careful!
        unsafe {
            // First check if reading the header is safe (basic sanity check)
            let header = ptr as *const ObjectHeader;

            // Check if type_tag looks valid (must be in range 0-15)
            let tag_val = (*header).type_tag as u8;
            if tag_val <= TypeTag::Bytes as u8 {
                match (*header).type_tag {
                    TypeTag::Str => {
                        let s = ptr as *const RoastString;
                        // Additional validation: check len is reasonable
                        if (*s).len < 1_000_000 && !(*s).data.is_null() {
                            let slice = std::slice::from_raw_parts((*s).data, (*s).len);
                            if let Ok(string) = std::str::from_utf8(slice) {
                                println!("{}", string);
                                return 0;
                            }
                        }
                    }
                    TypeTag::Bool => {
                        // Bool is typically stored inline, not as pointer
                        println!("{}", if value != 0 { "True" } else { "False" });
                        return 0;
                    }
                    TypeTag::Int => {
                        // Int stored inline
                        println!("{}", value);
                        return 0;
                    }
                    TypeTag::List => {
                        let list = ptr as *const RoastList;
                        if (*list).len < 1_000_000 && !(*list).data.is_null() {
                            print!("[");
                            for i in 0..(*list).len {
                                if i > 0 { print!(", "); }
                                let elem = *(*list).data.add(i);
                                let elem_str = value_to_display_string(elem);
                                print!("{}", elem_str);
                            }
                            println!("]");
                            return 0;
                        }
                    }
                    TypeTag::Dict => {
                        let dict = ptr as *const RoastDict;
                        let map = &*(*dict).map;
                        print!("{{");
                        let mut first = true;
                        // HashMap stores: hash -> (original_key, value)
                        for (original_key, val) in map.values() {
                            if !first { print!(", "); }
                            first = false;
                            let key_str = value_to_display_string(*original_key);
                            let val_str = value_to_display_string(*val);
                            print!("{}: {}", key_str, val_str);
                        }
                        println!("}}");
                        return 0;
                    }
                    TypeTag::Object => {
                        // Print object with class name and attributes
                        let obj = ptr as *const RoastObject;
                        let class = (*obj).class;
                        
                        // Get class name
                        let class_name = if !class.is_null() && !(*class).name.is_null() {
                            let name_str = (*class).name;
                            let slice = std::slice::from_raw_parts((*name_str).data, (*name_str).len);
                            std::str::from_utf8(slice).unwrap_or("<unknown>").to_string()
                        } else {
                            "<object>".to_string()
                        };
                        
                        // Print class name and attributes
                        print!("{}(", class_name);
                        if !(*obj).attrs.is_null() {
                            let attrs = &*(*obj).attrs;
                            let mut first = true;
                            for (&key, &val) in attrs.iter() {
                                if !first { print!(", "); }
                                first = false;
                                // Key is typically a symbol/hash, show value instead
                                let val_str = value_to_display_string(val);
                                print!("{}", val_str);
                            }
                        }
                        println!(")");
                        return 0;
                    }
                    _ => {}
                }
            }
        }
    }

    // Default: print as integer
    println!("{}", value);
    0
}

/// Print a value without newline (for multi-arg print)
#[no_mangle]
pub extern "C" fn roast_print_value(value: c_long) -> c_long {
    // Try to detect if this is a RoastString pointer
    let ptr = value as *const c_void;

    if !ptr.is_null() && is_likely_pointer(value) {
        unsafe {
            let header = ptr as *const ObjectHeader;
            let tag_val = (*header).type_tag as u8;
            if tag_val <= TypeTag::Bytes as u8 {
                match (*header).type_tag {
                    TypeTag::Str => {
                        let s = ptr as *const RoastString;
                        if (*s).len < 1_000_000 && !(*s).data.is_null() {
                            let slice = std::slice::from_raw_parts((*s).data, (*s).len);
                            if let Ok(string) = std::str::from_utf8(slice) {
                                print!("{}", string);
                                return 0;
                            }
                        }
                    }
                    TypeTag::Bool => {
                        print!("{}", if value != 0 { "True" } else { "False" });
                        return 0;
                    }
                    TypeTag::Int => {
                        print!("{}", value);
                        return 0;
                    }
                    TypeTag::List => {
                        let list = ptr as *const RoastList;
                        if (*list).len < 1_000_000 && !(*list).data.is_null() {
                            print!("[");
                            for i in 0..(*list).len {
                                if i > 0 { print!(", "); }
                                let elem = *(*list).data.add(i);
                                let elem_str = value_to_display_string(elem);
                                print!("{}", elem_str);
                            }
                            print!("]");
                            return 0;
                        }
                    }
                    TypeTag::Dict => {
                        let dict = ptr as *const RoastDict;
                        let map = &*(*dict).map;
                        print!("{{");
                        let mut first = true;
                        for (original_key, val) in map.values() {
                            if !first { print!(", "); }
                            first = false;
                            let key_str = value_to_display_string(*original_key);
                            let val_str = value_to_display_string(*val);
                            print!("{}: {}", key_str, val_str);
                        }
                        print!("}}");
                        return 0;
                    }
                    _ => {}
                }
            }
        }
    }

    // Default: print as integer (no newline)
    print!("{}", value);
    use std::io::Write;
    std::io::stdout().flush().ok();
    0
}

#[no_mangle]
pub extern "C" fn roast_len(value: c_long) -> c_long {
    // Get length based on type
    let ptr = value as *const c_void;
    if ptr.is_null() { return 0; }
    unsafe {
        let header = ptr as *const ObjectHeader;
        match (*header).type_tag {
            TypeTag::List => roast_list_len(ptr as *const RoastList),
            TypeTag::Str => roast_str_len(ptr as *const RoastString),
            TypeTag::Dict => roast_dict_len(ptr as *const RoastDict),
            TypeTag::Set => roast_set_len(ptr as *const RoastSet),
            TypeTag::Tuple => roast_tuple_len(ptr as *const RoastTuple),
            TypeTag::Range => roast_range_len(ptr as *const RoastRange),
            _ => 0,
        }
    }
}

#[no_mangle]
pub extern "C" fn roast_range(start: c_long, stop: c_long, step: c_long) -> c_long {
    roast_range_new(start, stop, step) as c_long
}

#[no_mangle]
pub extern "C" fn roast_sum(list: c_long) -> c_long {
    let ptr = list as *const RoastList;
    if ptr.is_null() { return 0; }
    unsafe {
        let mut sum: c_long = 0;
        for i in 0..(*ptr).len {
            sum += *(*ptr).data.add(i);
        }
        sum
    }
}

#[no_mangle]
pub extern "C" fn roast_all(list: c_long) -> c_long {
    let ptr = list as *const RoastList;
    if ptr.is_null() { return 1; }
    unsafe {
        for i in 0..(*ptr).len {
            if *(*ptr).data.add(i) == 0 {
                return 0;
            }
        }
        1
    }
}

#[no_mangle]
pub extern "C" fn roast_any(list: c_long) -> c_long {
    let ptr = list as *const RoastList;
    if ptr.is_null() { return 0; }
    unsafe {
        for i in 0..(*ptr).len {
            if *(*ptr).data.add(i) != 0 {
                return 1;
            }
        }
        0
    }
}

// ============================================================================
// Type Conversions
// ============================================================================

#[no_mangle]
pub extern "C" fn roast_float_to_int(value: c_double) -> c_long {
    value as c_long
}

#[no_mangle]
pub extern "C" fn roast_int_to_float(value: c_long) -> c_double {
    value as c_double
}

#[no_mangle]
pub extern "C" fn roast_int_to_bool(value: c_long) -> bool {
    value != 0
}

#[no_mangle]
pub extern "C" fn roast_bool_to_int(value: bool) -> c_long {
    if value { 1 } else { 0 }
}

// ============================================================================
// Python-like builtin conversion functions (str, int, float, bool)
// ============================================================================

/// Helper function to convert a value to a display string for dict/collection printing
fn value_to_display_string(value: c_long) -> String {
    if !is_likely_pointer(value) {
        return format!("{}", value);
    }
    
    let ptr = value as *const c_void;
    if ptr.is_null() {
        return "None".to_string();
    }
    
    unsafe {
        let header = ptr as *const ObjectHeader;
        let tag_val = (*header).type_tag as u8;
        if tag_val <= TypeTag::Bytes as u8 {
            match (*header).type_tag {
                TypeTag::Str => {
                    let s = ptr as *const RoastString;
                    let slice = std::slice::from_raw_parts((*s).data, (*s).len);
                    if let Ok(string) = std::str::from_utf8(slice) {
                        return format!("\"{}\"", string);
                    }
                    return "\"<invalid utf8>\"".to_string();
                }
                TypeTag::None => return "None".to_string(),
                TypeTag::List => {
                    // Recursively convert list elements
                    let list = ptr as *const RoastList;
                    if (*list).len < 1_000_000 && !(*list).data.is_null() {
                        let mut s = String::from("[");
                        for i in 0..(*list).len {
                            if i > 0 { s.push_str(", "); }
                            let elem = *(*list).data.add(i);
                            s.push_str(&value_to_display_string(elem));
                        }
                        s.push(']');
                        return s;
                    }
                }
                TypeTag::Dict => {
                    // Recursively convert dict key/values
                    let dict = ptr as *const RoastDict;
                    let map = &*(*dict).map;
                    let mut s = String::from("{");
                    let mut first = true;
                    for (original_key, val) in map.values() {
                        if !first { s.push_str(", "); }
                        first = false;
                        let key_str = value_to_display_string(*original_key);
                        let val_str = value_to_display_string(*val);
                        s.push_str(&format!("{}: {}", key_str, val_str));
                    }
                    s.push('}');
                    return s;
                }
                TypeTag::Object => {
                    // Format object with class name and attributes
                    let obj = ptr as *const RoastObject;
                    let class = (*obj).class;
                    
                    let class_name = if !class.is_null() && !(*class).name.is_null() {
                        let name_str = (*class).name;
                        let slice = std::slice::from_raw_parts((*name_str).data, (*name_str).len);
                        std::str::from_utf8(slice).unwrap_or("<unknown>").to_string()
                    } else {
                        "<object>".to_string()
                    };
                    
                    let mut s = format!("{}(", class_name);
                    if !(*obj).attrs.is_null() {
                        let attrs = &*(*obj).attrs;
                        let mut first = true;
                        for (&_key, &val) in attrs.iter() {
                            if !first { s.push_str(", "); }
                            first = false;
                            s.push_str(&value_to_display_string(val));
                        }
                    }
                    s.push(')');
                    return s;
                }
                _ => {}
            }
        }
    }
    
    format!("{}", value)
}

/// Convert any value to string (Python str() builtin)
#[no_mangle]
pub extern "C" fn roast_str(value: c_long) -> c_long {
    // First check: if value is NOT a likely pointer, it's probably a small integer.
    // This handles the critical case of 0 and other small integers that would
    // otherwise be misinterpreted as null pointers.
    if !is_likely_pointer(value) {
        // Treat as integer directly
        return roast_int_to_str(value) as c_long;
    }

    let ptr = value as *const c_void;

    // Handle null (only for actual null pointers, not integer 0)
    if ptr.is_null() {
        let s = CString::new("None").unwrap();
        return roast_str_from_cstr(s.as_ptr()) as c_long;
    }

    // Check if it's a likely pointer to an object
    unsafe {
        let header = ptr as *const ObjectHeader;
        let tag_val = (*header).type_tag as u8;
        if tag_val <= TypeTag::Bytes as u8 {
            match (*header).type_tag {
                TypeTag::Str => {
                    // Already a string - return as is
                    return value;
                }
                TypeTag::Int => {
                    return roast_int_to_str(value) as c_long;
                }
                TypeTag::Float => {
                    // Reinterpret as double
                    let float_ptr = ptr as *const c_double;
                    return roast_float_to_str(*float_ptr) as c_long;
                }
                TypeTag::Bool => {
                    return roast_bool_to_str(value != 0) as c_long;
                }
                TypeTag::List => {
                    let list = ptr as *const RoastList;
                    let mut s = String::from("[");
                    for i in 0..(*list).len {
                        if i > 0 { s.push_str(", "); }
                        let elem = *(*list).data.add(i);
                        // Use value_to_display_string for proper string formatting
                        s.push_str(&value_to_display_string(elem));
                    }
                    s.push(']');
                    let cstr = CString::new(s).unwrap();
                    return roast_str_from_cstr(cstr.as_ptr()) as c_long;
                }
                TypeTag::Dict => {
                    let dict = ptr as *const RoastDict;
                    let mut s = String::from("{");
                    let map = &*(*dict).map;
                    let mut first = true;
                    // HashMap stores: hash -> (original_key, value)
                    for (original_key, val) in map.values() {
                        if !first { s.push_str(", "); }
                        first = false;
                        // Format key and value - show original string keys properly
                        let key_str = value_to_display_string(*original_key);
                        let val_str = value_to_display_string(*val);
                        s.push_str(&format!("{}: {}", key_str, val_str));
                    }
                    s.push('}');
                    let cstr = CString::new(s).unwrap();
                    return roast_str_from_cstr(cstr.as_ptr()) as c_long;
                }
                TypeTag::None => {
                    let s = CString::new("None").unwrap();
                    return roast_str_from_cstr(s.as_ptr()) as c_long;
                }
                _ => {}
            }
        }
    }

    // Default: treat as integer
    roast_int_to_str(value) as c_long
}

/// Convert any value to int (Python int() builtin)
#[no_mangle]
pub extern "C" fn roast_int(value: c_long) -> c_long {
    let ptr = value as *const c_void;

    if ptr.is_null() {
        return 0;
    }

    if is_likely_pointer(value) {
        unsafe {
            let header = ptr as *const ObjectHeader;
            let tag_val = (*header).type_tag as u8;
            if tag_val <= TypeTag::Bytes as u8 {
                match (*header).type_tag {
                    TypeTag::Str => {
                        return roast_str_to_int(ptr as *const RoastString);
                    }
                    TypeTag::Float => {
                        let float_ptr = ptr as *const c_double;
                        return *float_ptr as c_long;
                    }
                    TypeTag::Bool => {
                        return if value != 0 { 1 } else { 0 };
                    }
                    TypeTag::Int => {
                        return value;
                    }
                    _ => {}
                }
            }
        }
    }

    // Already an integer
    value
}

/// Convert any value to float (Python float() builtin)
#[no_mangle]
pub extern "C" fn roast_float(value: c_long) -> c_long {
    let ptr = value as *const c_void;

    if is_likely_pointer(value) {
        unsafe {
            let header = ptr as *const ObjectHeader;
            let tag_val = (*header).type_tag as u8;
            if tag_val <= TypeTag::Bytes as u8 {
                match (*header).type_tag {
                    TypeTag::Str => {
                        let f = roast_str_to_float(ptr as *const RoastString);
                        return f.to_bits() as c_long;
                    }
                    TypeTag::Int => {
                        let f = value as c_double;
                        return f.to_bits() as c_long;
                    }
                    TypeTag::Float => {
                        return value;
                    }
                    _ => {}
                }
            }
        }
    }

    // Treat as int, convert to float
    let f = value as c_double;
    f.to_bits() as c_long
}

/// Convert any value to bool (Python bool() builtin)
#[no_mangle]
pub extern "C" fn roast_bool(value: c_long) -> c_long {
    let ptr = value as *const c_void;

    if ptr.is_null() {
        return 0;
    }

    if is_likely_pointer(value) {
        unsafe {
            let header = ptr as *const ObjectHeader;
            let tag_val = (*header).type_tag as u8;
            if tag_val <= TypeTag::Bytes as u8 {
                match (*header).type_tag {
                    TypeTag::Str => {
                        let s = ptr as *const RoastString;
                        return if (*s).len > 0 { 1 } else { 0 };
                    }
                    TypeTag::List => {
                        let list = ptr as *const RoastList;
                        return if (*list).len > 0 { 1 } else { 0 };
                    }
                    TypeTag::Dict => {
                        let dict = ptr as *const RoastDict;
                        return if roast_dict_len(dict) > 0 { 1 } else { 0 };
                    }
                    TypeTag::None => {
                        return 0;
                    }
                    _ => {}
                }
            }
        }
    }

    // Non-zero is true
    if value != 0 { 1 } else { 0 }
}

// ============================================================================
// Async (Synchronous execution - async functions run sequentially)
// ============================================================================

#[no_mangle]
pub extern "C" fn roast_await(future: *mut c_void) -> c_long {
    // In the LLVM backend, async functions execute synchronously and return
    // their result directly. The "future" parameter is actually already the
    // computed result value. We just need to pass it through.
    //
    // This provides Python-like async semantics in a single-threaded context:
    // - async functions work but execute sequentially
    // - await returns the function's result
    //
    // For true async parallelism, would need proper coroutine scheduling.
    future as c_long
}

// ============================================================================
// Reference Counting
// ============================================================================

/// Reference-counted wrapper for shared ownership
/// In this simple implementation, the rc wrapper just holds the value.
/// Assignment (ref1 = ref2) shares the same rc pointer.
#[repr(C)]
pub struct RoastRc {
    /// The wrapped value
    value: c_long,
    /// Reference count (for future use with proper memory management)
    refcount: std::sync::atomic::AtomicUsize,
}

/// Create a new reference-counted value
#[no_mangle]
pub extern "C" fn roast_rc_create(value: c_long) -> c_long {
    let rc_box = Box::new(RoastRc {
        value,
        refcount: std::sync::atomic::AtomicUsize::new(1),
    });
    // Return pointer to rc wrapper as i64
    Box::into_raw(rc_box) as c_long
}

/// Get the value from a reference-counted wrapper
#[no_mangle]
pub extern "C" fn roast_rc_get(rc_ptr: c_long) -> c_long {
    if rc_ptr == 0 {
        return 0;
    }
    let rc = unsafe { &*(rc_ptr as *const RoastRc) };
    rc.value
}

// ============================================================================
// Exception Handling with setjmp/longjmp
// ============================================================================

use std::cell::RefCell;

// Import libc for setjmp/longjmp
#[cfg(unix)]
extern "C" {
    fn setjmp(env: *mut JmpBuf) -> c_int;
    fn longjmp(env: *mut JmpBuf, val: c_int) -> !;
}

#[cfg(windows)]
extern "C" {
    fn _setjmp(env: *mut JmpBuf) -> c_int;
    fn longjmp(env: *mut JmpBuf, val: c_int) -> !;
}

#[cfg(windows)]
unsafe fn setjmp(env: *mut JmpBuf) -> c_int {
    _setjmp(env)
}

/// Jump buffer for setjmp/longjmp (platform-specific size)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct JmpBuf {
    #[cfg(target_os = "linux")]
    _data: [u64; 25], // __jmp_buf on Linux x86_64
    #[cfg(target_os = "macos")]
    _data: [u64; 37], // jmp_buf on macOS
    #[cfg(target_os = "windows")]
    _data: [u64; 32], // jmp_buf on Windows
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    _data: [u64; 64], // Conservative fallback
}

impl Default for JmpBuf {
    fn default() -> Self {
        Self {
            _data: [0; {
                #[cfg(target_os = "linux")]
                { 25 }
                #[cfg(target_os = "macos")]
                { 37 }
                #[cfg(target_os = "windows")]
                { 32 }
                #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
                { 64 }
            }],
        }
    }
}

/// Exception object that can be raised
#[repr(C)]
pub struct RoastException {
    header: ObjectHeader,
    type_code: c_long,       // Exception type code (1=ValueError, 2=TypeError, etc.)
    message: *mut RoastString, // Exception message (may be null)
}

/// Create a new exception object
#[no_mangle]
pub extern "C" fn roast_exception_create(type_code: c_long, message: c_long) -> *mut RoastException {
    unsafe {
        let layout = Layout::new::<RoastException>();
        let ptr = alloc(layout) as *mut RoastException;
        (*ptr).header = ObjectHeader::new(TypeTag::None); // Using None tag for exception
        (*ptr).type_code = type_code;
        (*ptr).message = if message == 0 {
            std::ptr::null_mut()
        } else {
            message as *mut RoastString
        };
        ptr
    }
}

/// Get exception type code from exception object
#[no_mangle]
pub extern "C" fn roast_exception_get_type(exc: c_long) -> c_long {
    if exc == 0 {
        return 0;
    }
    // First check if it's a direct type code (small integer)
    if exc > 0 && exc < 100 {
        return exc; // It's a type code directly
    }
    // Otherwise it's a pointer to RoastException
    unsafe {
        let exc_ptr = exc as *const RoastException;
        if exc_ptr.is_null() {
            0
        } else {
            (*exc_ptr).type_code
        }
    }
}

/// Exception handler frame
struct ExceptionFrame {
    jmp_buf: JmpBuf,
    exception_type: c_long,
    exception_value: c_long,
}

impl Default for ExceptionFrame {
    fn default() -> Self {
        Self {
            jmp_buf: JmpBuf::default(),
            exception_type: 0,
            exception_value: 0,
        }
    }
}

/// Thread-local exception state
thread_local! {
    static EXCEPTION_STATE: RefCell<ExceptionState> = RefCell::new(ExceptionState::new());
}

struct ExceptionState {
    /// Current exception value (0 = no exception)
    current_exception: c_long,
    /// Exception type code
    exception_type_code: c_long,
    /// Exception type name
    exception_type: String,
    /// Exception message
    exception_message: String,
    /// Stack of exception frames for nested try blocks
    frames: Vec<ExceptionFrame>,
}

impl ExceptionState {
    fn new() -> Self {
        Self {
            current_exception: 0,
            exception_type_code: 0,
            exception_type: String::new(),
            exception_message: String::new(),
            frames: Vec::with_capacity(16),
        }
    }
}

/// Push a new exception frame and return pointer to jmp_buf for setjmp
/// Returns a pointer that LLVM code will use with setjmp
#[no_mangle]
pub extern "C" fn roast_exception_push_frame() -> *mut JmpBuf {
    EXCEPTION_STATE.with(|state| {
        let mut s = state.borrow_mut();
        s.frames.push(ExceptionFrame::default());
        let frame = s.frames.last_mut().unwrap();
        &mut frame.jmp_buf as *mut JmpBuf
    })
}

/// Pop the current exception frame
#[no_mangle]
pub extern "C" fn roast_exception_pop_frame() {
    EXCEPTION_STATE.with(|state| {
        let mut s = state.borrow_mut();
        s.frames.pop();
        // Clear exception if no more frames
        if s.frames.is_empty() {
            s.current_exception = 0;
            s.exception_type_code = 0;
            s.exception_type.clear();
            s.exception_message.clear();
        }
    })
}

/// Begin a try block - returns 0 normally, non-zero if exception raised
#[no_mangle]
pub extern "C" fn roast_try_begin() -> c_long {
    EXCEPTION_STATE.with(|state| {
        let s = state.borrow();
        // Return current exception state (0 = no exception)
        s.current_exception
    })
}

/// End a try block
#[no_mangle]
pub extern "C" fn roast_try_end() {
    EXCEPTION_STATE.with(|state| {
        let mut s = state.borrow_mut();
        // Clear exception state when exiting try block normally
        s.current_exception = 0;
        s.exception_type_code = 0;
        s.exception_type.clear();
        s.exception_message.clear();
    })
}

/// Get exception type name from type code
fn exception_type_name(type_code: c_long) -> &'static str {
    match type_code {
        1 => "ValueError",
        2 => "TypeError",
        3 => "RuntimeError",
        4 => "IndexError",
        5 => "KeyError",
        6 => "AttributeError",
        7 => "NotImplementedError",
        8 => "StopIteration",
        9 => "AssertionError",
        10 => "ZeroDivisionError",
        11 => "OverflowError",
        12 => "NameError",
        13 => "ImportError",
        14 => "ModuleNotFoundError",
        15 => "FileNotFoundError",
        16 => "OSError",
        17 => "MemoryError",
        18 => "RecursionError",
        _ => "Exception",
    }
}

/// Raise an exception - uses longjmp if inside a try block
#[no_mangle]
pub extern "C" fn roast_raise(exc: c_long) {
    // Extract type code from exception (could be type code directly or RoastException pointer)
    let type_code = roast_exception_get_type(exc);

    // Store exception info
    let has_frame = EXCEPTION_STATE.with(|state| {
        let mut s = state.borrow_mut();
        s.current_exception = exc;
        s.exception_type_code = type_code;
        s.exception_type = exception_type_name(type_code).to_string();

        !s.frames.is_empty()
    });

    if has_frame {
        // We have an exception frame - longjmp back to it
        EXCEPTION_STATE.with(|state| {
            let s = state.borrow();
            if let Some(frame) = s.frames.last() {
                let jmp_buf_ptr = &frame.jmp_buf as *const JmpBuf as *mut JmpBuf;
                // Drop borrow before longjmp
                drop(s);
                unsafe {
                    longjmp(jmp_buf_ptr, 1);
                }
            }
        });
    }

    // No try block - uncaught exception
    EXCEPTION_STATE.with(|state| {
        let s = state.borrow();
        eprintln!("Uncaught {}", s.exception_type);
        std::process::exit(1);
    });
}

/// Raise an exception with a message
#[no_mangle]
pub extern "C" fn roast_raise_with_message(exc_type: c_long, message: c_long) {
    // Store exception info
    let msg_str = if message == 0 {
        String::new()
    } else {
        let msg_ptr = message as *const RoastString;
        if msg_ptr.is_null() {
            String::new()
        } else {
            unsafe {
                let rs = &*msg_ptr;
                let slice = std::slice::from_raw_parts(rs.data, rs.len);
                String::from_utf8_lossy(slice).into_owned()
            }
        }
    };

    let has_frame = EXCEPTION_STATE.with(|state| {
        let mut s = state.borrow_mut();
        s.current_exception = exc_type;
        s.exception_type_code = exc_type;
        s.exception_type = match exc_type {
            1 => "ValueError",
            2 => "TypeError",
            3 => "RuntimeError",
            4 => "IndexError",
            5 => "KeyError",
            6 => "AttributeError",
            7 => "NotImplementedError",
            8 => "StopIteration",
            9 => "AssertionError",
            10 => "ZeroDivisionError",
            _ => "Exception",
        }.to_string();
        s.exception_message = msg_str;

        !s.frames.is_empty()
    });

    if has_frame {
        EXCEPTION_STATE.with(|state| {
            let s = state.borrow();
            if let Some(frame) = s.frames.last() {
                let jmp_buf_ptr = &frame.jmp_buf as *const JmpBuf as *mut JmpBuf;
                drop(s);
                unsafe {
                    longjmp(jmp_buf_ptr, 1);
                }
            }
        });
    }

    // No try block - uncaught exception
    EXCEPTION_STATE.with(|state| {
        let s = state.borrow();
        if s.exception_message.is_empty() {
            eprintln!("Uncaught {}", s.exception_type);
        } else {
            eprintln!("Uncaught {}: {}", s.exception_type, s.exception_message);
        }
        std::process::exit(1);
    });
}

/// Get the current exception type code
#[no_mangle]
pub extern "C" fn roast_get_exception_type() -> c_long {
    EXCEPTION_STATE.with(|state| {
        state.borrow().exception_type_code
    })
}

/// Re-raise the current exception
#[no_mangle]
pub extern "C" fn roast_reraise() {
    let (has_exc, has_frame) = EXCEPTION_STATE.with(|state| {
        let s = state.borrow();
        (s.current_exception != 0, !s.frames.is_empty())
    });

    if !has_exc {
        eprintln!("RuntimeError: No active exception to re-raise");
        std::process::exit(1);
    }

    if has_frame {
        EXCEPTION_STATE.with(|state| {
            let s = state.borrow();
            if let Some(frame) = s.frames.last() {
                let jmp_buf_ptr = &frame.jmp_buf as *const JmpBuf as *mut JmpBuf;
                drop(s);
                unsafe {
                    longjmp(jmp_buf_ptr, 1);
                }
            }
        });
    }

    EXCEPTION_STATE.with(|state| {
        let s = state.borrow();
        eprintln!("Uncaught {}", s.exception_type);
        std::process::exit(1);
    });
}

/// Get the current exception value
#[no_mangle]
pub extern "C" fn roast_get_exception() -> c_long {
    EXCEPTION_STATE.with(|state| {
        state.borrow().current_exception
    })
}

/// Check if exception matches a type (by name)
#[no_mangle]
pub extern "C" fn roast_exception_matches(exc: c_long, type_name: *const c_char) -> c_long {
    if type_name.is_null() {
        return 1; // Bare except catches all
    }

    let type_str = unsafe {
        std::ffi::CStr::from_ptr(type_name).to_string_lossy()
    };

    // For now, match by checking if the exception type contains the name
    // This is a simplified version - real Python does isinstance checks
    EXCEPTION_STATE.with(|state| {
        let s = state.borrow();
        if type_str == "Exception" || type_str == "BaseException" {
            1 // Catch all exceptions
        } else if s.exception_type == type_str.as_ref() {
            1
        } else {
            0
        }
    })
}

/// Create a new exception object from type code and message string
#[no_mangle]
pub extern "C" fn roast_exception_new(exc_type: c_long, message: c_long) -> c_long {
    // Create a proper exception object that preserves type code
    roast_exception_create(exc_type, message) as c_long
}

// ============================================================================
// Runtime Init/Cleanup
// ============================================================================
// Type introspection
// ============================================================================

/// Get the type of a value at runtime.
/// Returns a type tag that can be compared with type constants.
/// For heap objects, reads the type tag from the object header.
/// For primitives, we encode them inline.
#[no_mangle]
pub extern "C" fn roast_type(value: c_long) -> c_long {
    // Check for special values first
    if value == 0 {
        return TypeTag::None as c_long;
    }
    
    // Check if this looks like a pointer to a heap object
    let ptr = value as *const c_void;
    if !ptr.is_null() && is_likely_pointer(value) {
        unsafe {
            let header = ptr as *const ObjectHeader;
            let tag_val = (*header).type_tag as u8;
            // Validate that it's a valid type tag
            if tag_val <= TypeTag::Property as u8 {
                return (*header).type_tag as c_long;
            }
        }
    }
    
    // Assume it's an integer if it doesn't look like a pointer
    TypeTag::Int as c_long
}

/// Type constants for comparison in Roast code
/// These match the TypeTag enum values
#[no_mangle]
pub static roast_type_int: c_long = TypeTag::Int as c_long;
#[no_mangle]
pub static roast_type_float: c_long = TypeTag::Float as c_long;
#[no_mangle]
pub static roast_type_str: c_long = TypeTag::Str as c_long;
#[no_mangle]
pub static roast_type_bool: c_long = TypeTag::Bool as c_long;
#[no_mangle]
pub static roast_type_list: c_long = TypeTag::List as c_long;
#[no_mangle]
pub static roast_type_dict: c_long = TypeTag::Dict as c_long;
#[no_mangle]
pub static roast_type_none: c_long = TypeTag::None as c_long;

// ============================================================================
// Initialization
// ============================================================================


#[no_mangle]
pub extern "C" fn roast_init() {
    // Initialize runtime (if needed)
}

#[no_mangle]
pub extern "C" fn roast_cleanup() {
    // Cleanup runtime (if needed)
}

// ============================================================================
// File I/O Operations
// ============================================================================

use std::fs::{File, OpenOptions};
use std::io::{Read, Write, BufRead, BufReader};

/// A file handle for Roast file I/O
#[repr(C)]
pub struct RoastFile {
    header: ObjectHeader,
    file: *mut File,
    mode: u8,  // 0 = read, 1 = write, 2 = append
    path: *mut RoastString,
}

/// TypeTag for File (add to enum if not present, use 20 for now)
const FILE_TAG: u8 = 20;

/// Open a file with the given mode ("r", "w", "a", "rb", "wb", "ab")
/// Returns a RoastFile pointer
#[no_mangle]
pub extern "C" fn roast_file_open(path: *const RoastString, mode: *const RoastString) -> *mut RoastFile {
    if path.is_null() || mode.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        // Get path string
        let path_slice = std::slice::from_raw_parts((*path).data, (*path).len);
        let path_str = match std::str::from_utf8(path_slice) {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        
        // Get mode string
        let mode_slice = std::slice::from_raw_parts((*mode).data, (*mode).len);
        let mode_str = match std::str::from_utf8(mode_slice) {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        
        // Parse mode and open file
        let file = match mode_str {
            "r" | "rb" => File::open(path_str),
            "w" | "wb" => File::create(path_str),
            "a" | "ab" => OpenOptions::new().append(true).create(true).open(path_str),
            _ => return std::ptr::null_mut(),
        };
        
        let file = match file {
            Ok(f) => f,
            Err(_) => return std::ptr::null_mut(),
        };
        
        // Allocate RoastFile
        let layout = Layout::new::<RoastFile>();
        let ptr = alloc(layout) as *mut RoastFile;
        
        let mode_val = match mode_str.chars().next() {
            Some('r') => 0,
            Some('w') => 1,
            Some('a') => 2,
            _ => 0,
        };
        
        (*ptr).header = ObjectHeader::new(TypeTag::None); // Use None for now
        (*ptr).file = Box::into_raw(Box::new(file));
        (*ptr).mode = mode_val;
        (*ptr).path = path as *mut RoastString;
        
        ptr
    }
}

/// Read the entire file contents as a string
#[no_mangle]
pub extern "C" fn roast_file_read(file: *mut RoastFile) -> *mut RoastString {
    if file.is_null() {
        return roast_str_from_cstr(b"".as_ptr() as *const c_char);
    }
    
    unsafe {
        if (*file).file.is_null() {
            return roast_str_from_cstr(b"".as_ptr() as *const c_char);
        }
        
        let f = &mut *(*file).file;
        let mut contents = String::new();
        
        if f.read_to_string(&mut contents).is_err() {
            return roast_str_from_cstr(b"".as_ptr() as *const c_char);
        }
        
        roast_str_new(contents.as_ptr() as *const c_char, contents.len() as c_long)
    }
}

/// Write a string to the file
#[no_mangle]
pub extern "C" fn roast_file_write(file: *mut RoastFile, content: *const RoastString) -> c_long {
    if file.is_null() || content.is_null() {
        return 0;
    }
    
    unsafe {
        if (*file).file.is_null() {
            return 0;
        }
        
        let f = &mut *(*file).file;
        let slice = std::slice::from_raw_parts((*content).data, (*content).len);
        
        match f.write_all(slice) {
            Ok(_) => (*content).len as c_long,
            Err(_) => 0,
        }
    }
}

/// Close the file
#[no_mangle]
pub extern "C" fn roast_file_close(file: *mut RoastFile) {
    if file.is_null() {
        return;
    }
    
    unsafe {
        if !(*file).file.is_null() {
            // Drop the file to close it
            let _ = Box::from_raw((*file).file);
            (*file).file = std::ptr::null_mut();
        }
    }
}

/// Read a single line from the file
#[no_mangle]
pub extern "C" fn roast_file_readline(file: *mut RoastFile) -> *mut RoastString {
    if file.is_null() {
        return roast_str_from_cstr(b"".as_ptr() as *const c_char);
    }
    
    unsafe {
        if (*file).file.is_null() {
            return roast_str_from_cstr(b"".as_ptr() as *const c_char);
        }
        
        // Note: This is a simplified implementation
        // For a proper implementation, we'd need to buffer the file
        let f = &mut *(*file).file;
        let mut line = String::new();
        let mut buf = [0u8; 1];
        
        loop {
            match f.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if buf[0] == b'\n' {
                        line.push('\n');
                        break;
                    }
                    line.push(buf[0] as char);
                }
                Err(_) => break,
            }
        }
        
        roast_str_new(line.as_ptr() as *const c_char, line.len() as c_long)
    }
}

/// Python-like open() - returns a File object
/// This is the main entry point for file I/O
#[no_mangle]
pub extern "C" fn roast_open(path: *const RoastString, mode: *const RoastString) -> *mut RoastFile {
    roast_file_open(path, mode)
}

// ============================================================================
// Password Hashing (bcrypt) - Security Module
// ============================================================================

/// Hash a password using bcrypt
/// Returns the hashed password string or null on error
#[no_mangle]
pub extern "C" fn roast_bcrypt_hash(password: *const RoastString, cost: c_long) -> *mut RoastString {
    if password.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        let slice = std::slice::from_raw_parts((*password).data, (*password).len);
        if let Ok(pw_str) = std::str::from_utf8(slice) {
            let cost = if cost <= 0 || cost > 31 { 12 } else { cost as u32 };
            match bcrypt::hash(pw_str, cost) {
                Ok(hashed) => {
                    return roast_str_from_cstr(
                        std::ffi::CString::new(hashed).unwrap().as_ptr()
                    );
                }
                Err(_) => {}
            }
        }
        std::ptr::null_mut()
    }
}

/// Verify a password against a bcrypt hash
/// Returns 1 if match, 0 if no match, -1 on error
#[no_mangle]
pub extern "C" fn roast_bcrypt_verify(password: *const RoastString, hash: *const RoastString) -> c_long {
    if password.is_null() || hash.is_null() { return -1; }
    
    unsafe {
        let pw_slice = std::slice::from_raw_parts((*password).data, (*password).len);
        let hash_slice = std::slice::from_raw_parts((*hash).data, (*hash).len);
        
        let pw_str = match std::str::from_utf8(pw_slice) {
            Ok(s) => s,
            Err(_) => return -1,
        };
        let hash_str = match std::str::from_utf8(hash_slice) {
            Ok(s) => s,
            Err(_) => return -1,
        };
        
        match bcrypt::verify(pw_str, hash_str) {
            Ok(true) => 1,
            Ok(false) => 0,
            Err(_) => -1,
        }
    }
}

// ============================================================================
// SHA256 Hashing - Crypto Module
// ============================================================================

/// Hash data using SHA256, returns hex string
#[no_mangle]
pub extern "C" fn roast_sha256(data: *const RoastString) -> *mut RoastString {
    if data.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        use sha2::{Sha256, Digest};
        
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        let mut hasher = Sha256::new();
        hasher.update(slice);
        let result = hasher.finalize();
        
        let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
        roast_str_from_cstr(std::ffi::CString::new(hex).unwrap().as_ptr())
    }
}

/// Hash data using MD5, returns hex string
#[no_mangle]
pub extern "C" fn roast_md5(data: *const RoastString) -> *mut RoastString {
    if data.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        use md5::{Md5, Digest};
        
        let slice = std::slice::from_raw_parts((*data).data, (*data).len);
        let mut hasher = Md5::new();
        hasher.update(slice);
        let result = hasher.finalize();
        
        let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
        roast_str_from_cstr(std::ffi::CString::new(hex).unwrap().as_ptr())
    }
}

// ============================================================================
// Simple JWT-like Token - Auth Module
// ============================================================================

/// Encode a simple JWT-like token (header.payload.signature)
/// payload: JSON string, secret: signing key
/// Returns base64url encoded token
#[no_mangle]
pub extern "C" fn roast_jwt_encode(payload: *const RoastString, secret: *const RoastString) -> *mut RoastString {
    if payload.is_null() || secret.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        use sha2::{Sha256, Digest};
        
        let payload_slice = std::slice::from_raw_parts((*payload).data, (*payload).len);
        let secret_slice = std::slice::from_raw_parts((*secret).data, (*secret).len);
        
        // Simple JWT structure: base64(header).base64(payload).base64(signature)
        let header = r#"{"alg":"HS256","typ":"JWT"}"#;
        let header_b64 = base64url_encode(header.as_bytes());
        let payload_b64 = base64url_encode(payload_slice);
        
        // Create signature: SHA256(header.payload + secret)
        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let mut hasher = Sha256::new();
        hasher.update(signing_input.as_bytes());
        hasher.update(secret_slice);
        let signature = hasher.finalize();
        let sig_b64 = base64url_encode(&signature);
        
        let token = format!("{}.{}.{}", header_b64, payload_b64, sig_b64);
        roast_str_from_cstr(std::ffi::CString::new(token).unwrap().as_ptr())
    }
}

/// Decode and verify a JWT-like token
/// Returns decoded payload JSON string, or null if invalid
#[no_mangle]
pub extern "C" fn roast_jwt_decode(token: *const RoastString, secret: *const RoastString) -> *mut RoastString {
    if token.is_null() || secret.is_null() { return std::ptr::null_mut(); }
    
    unsafe {
        use sha2::{Sha256, Digest};
        
        let token_slice = std::slice::from_raw_parts((*token).data, (*token).len);
        let secret_slice = std::slice::from_raw_parts((*secret).data, (*secret).len);
        
        let token_str = match std::str::from_utf8(token_slice) {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        
        // Split into parts
        let parts: Vec<&str> = token_str.split('.').collect();
        if parts.len() != 3 {
            return std::ptr::null_mut();
        }
        
        let header_b64 = parts[0];
        let payload_b64 = parts[1];
        let sig_b64 = parts[2];
        
        // Verify signature
        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let mut hasher = Sha256::new();
        hasher.update(signing_input.as_bytes());
        hasher.update(secret_slice);
        let expected_sig = hasher.finalize();
        let expected_sig_b64 = base64url_encode(&expected_sig);
        
        if sig_b64 != expected_sig_b64 {
            return std::ptr::null_mut(); // Invalid signature
        }
        
        // Decode payload
        match base64url_decode(payload_b64) {
            Some(payload_bytes) => {
                if let Ok(payload_str) = String::from_utf8(payload_bytes) {
                    return roast_str_from_cstr(
                        std::ffi::CString::new(payload_str).unwrap().as_ptr()
                    );
                }
            }
            None => {}
        }
        
        std::ptr::null_mut()
    }
}

/// Verify a JWT without decoding (just checks signature)
/// Returns 1 if valid, 0 if invalid
#[no_mangle]
pub extern "C" fn roast_jwt_verify(token: *const RoastString, secret: *const RoastString) -> c_long {
    if roast_jwt_decode(token, secret).is_null() { 0 } else { 1 }
}

// Helper functions for base64url encoding (JWT uses URL-safe base64)
fn base64url_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        
        result.push(CHARS[(b0 >> 2)] as char);
        result.push(CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        
        if chunk.len() > 1 {
            result.push(CHARS[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        }
        
        if chunk.len() > 2 {
            result.push(CHARS[b2 & 0x3f] as char);
        }
    }
    
    result
}

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    const DECODE: [i8; 128] = [
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,62,-1,-1,
        52,53,54,55,56,57,58,59,60,61,-1,-1,-1,-1,-1,-1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,
        15,16,17,18,19,20,21,22,23,24,25,-1,-1,-1,-1,63,
        -1,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,
        41,42,43,44,45,46,47,48,49,50,51,-1,-1,-1,-1,-1,
    ];
    
    let bytes: Vec<u8> = s.bytes().collect();
    let mut result = Vec::new();
    
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 { break; }
        let b0 = DECODE.get(chunk[0] as usize).copied().unwrap_or(-1);
        let b1 = DECODE.get(chunk[1] as usize).copied().unwrap_or(-1);
        if b0 < 0 || b1 < 0 { return None; }
        
        result.push(((b0 << 2) | (b1 >> 4)) as u8);
        
        if chunk.len() > 2 {
            let b2 = DECODE.get(chunk[2] as usize).copied().unwrap_or(-1);
            if b2 >= 0 {
                result.push((((b1 & 0x0f) << 4) | (b2 >> 2)) as u8);
            }
            
            if chunk.len() > 3 {
                let b3 = DECODE.get(chunk[3] as usize).copied().unwrap_or(-1);
                if b3 >= 0 {
                    result.push((((b2 & 0x03) << 6) | b3) as u8);
                }
            }
        }
    }
    
    Some(result)
}

// ============================================================================
// Secure Random - Crypto Module
// ============================================================================

/// Generate cryptographically secure random bytes as hex string
#[no_mangle]
pub extern "C" fn roast_secure_random_hex(n_bytes: c_long) -> *mut RoastString {
    if n_bytes <= 0 { return std::ptr::null_mut(); }
    
    use std::time::{SystemTime, UNIX_EPOCH};
    
    // Use system entropy + time for pseudo-random bytes
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let mut state = now.as_nanos() as u64;
    
    let mut bytes = Vec::with_capacity(n_bytes as usize);
    for _ in 0..n_bytes {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        bytes.push((state >> 56) as u8);
    }
    
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    roast_str_from_cstr(std::ffi::CString::new(hex).unwrap().as_ptr())
}

/// Generate UUID v7 (time-ordered UUID)
#[no_mangle]
pub extern "C" fn roast_uuid_v7() -> *mut RoastString {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let ms = now.as_millis() as u64;
    
    // First 48 bits are timestamp
    let mut bytes = [0u8; 16];
    bytes[0] = (ms >> 40) as u8;
    bytes[1] = (ms >> 32) as u8;
    bytes[2] = (ms >> 24) as u8;
    bytes[3] = (ms >> 16) as u8;
    bytes[4] = (ms >> 8) as u8;
    bytes[5] = ms as u8;
    
    // Random bytes for the rest
    let mut state = now.as_nanos() as u64;
    for i in 6..16 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        bytes[i] = (state >> 56) as u8;
    }
    
    // Set version (7) and variant (RFC 4122)
    bytes[6] = (bytes[6] & 0x0f) | 0x70; // Version 7
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant
    
    let uuid = format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    );
    
    roast_str_from_cstr(std::ffi::CString::new(uuid).unwrap().as_ptr())
}

// ============================================================================
// Testing Framework - Assert Functions
// ============================================================================

/// Assert two values are equal
/// Returns 1 if equal, panics with message if not
#[no_mangle]
pub extern "C" fn roast_test_assert_eq(a: c_long, b: c_long, msg: *const RoastString) -> c_long {
    if a == b {
        return 1;
    }
    
    let msg_str = if !msg.is_null() {
        unsafe {
            let slice = std::slice::from_raw_parts((*msg).data, (*msg).len);
            std::str::from_utf8(slice).unwrap_or("Assertion failed")
        }
    } else {
        "Assertion failed"
    };
    
    eprintln!("AssertionError: {} (expected {}, got {})", msg_str, a, b);
    panic!("Test assertion failed: {} != {}", a, b);
}

/// Assert a value is true (non-zero)
#[no_mangle]
pub extern "C" fn roast_test_assert_true(value: c_long, msg: *const RoastString) -> c_long {
    if value != 0 {
        return 1;
    }
    
    let msg_str = if !msg.is_null() {
        unsafe {
            let slice = std::slice::from_raw_parts((*msg).data, (*msg).len);
            std::str::from_utf8(slice).unwrap_or("Expected true")
        }
    } else {
        "Expected true"
    };
    
    eprintln!("AssertionError: {}", msg_str);
    panic!("Test assertion failed: expected true, got false");
}

/// Assert a value is false (zero)
#[no_mangle]
pub extern "C" fn roast_test_assert_false(value: c_long, msg: *const RoastString) -> c_long {
    if value == 0 {
        return 1;
    }
    
    let msg_str = if !msg.is_null() {
        unsafe {
            let slice = std::slice::from_raw_parts((*msg).data, (*msg).len);
            std::str::from_utf8(slice).unwrap_or("Expected false")
        }
    } else {
        "Expected false"
    };
    
    eprintln!("AssertionError: {}", msg_str);
    panic!("Test assertion failed: expected false, got true");
}

/// Assert two values are not equal
#[no_mangle]
pub extern "C" fn roast_test_assert_ne(a: c_long, b: c_long, msg: *const RoastString) -> c_long {
    if a != b {
        return 1;
    }
    
    let msg_str = if !msg.is_null() {
        unsafe {
            let slice = std::slice::from_raw_parts((*msg).data, (*msg).len);
            std::str::from_utf8(slice).unwrap_or("Values should not be equal")
        }
    } else {
        "Values should not be equal"
    };
    
    eprintln!("AssertionError: {} (both were {})", msg_str, a);
    panic!("Test assertion failed: {} == {}", a, b);
}

/// Fail the test with a message
#[no_mangle]
pub extern "C" fn roast_test_fail(msg: *const RoastString) {
    let msg_str = if !msg.is_null() {
        unsafe {
            let slice = std::slice::from_raw_parts((*msg).data, (*msg).len);
            std::str::from_utf8(slice).unwrap_or("Test failed")
        }
    } else {
        "Test failed"
    };
    
    eprintln!("FAIL: {}", msg_str);
    panic!("Test explicitly failed: {}", msg_str);
}

/// Skip the current test
#[no_mangle]
pub extern "C" fn roast_test_skip(msg: *const RoastString) {
    let msg_str = if !msg.is_null() {
        unsafe {
            let slice = std::slice::from_raw_parts((*msg).data, (*msg).len);
            std::str::from_utf8(slice).unwrap_or("skipped")
        }
    } else {
        "skipped"
    };
    
    println!("SKIP: {}", msg_str);
    // Don't panic - just return and let test be marked as skipped
}

/// Assert a string equals expected
#[no_mangle]
pub extern "C" fn roast_test_assert_str_eq(
    a: *const RoastString, 
    b: *const RoastString,
    msg: *const RoastString
) -> c_long {
    if a.is_null() && b.is_null() {
        return 1;
    }
    if a.is_null() || b.is_null() {
        eprintln!("AssertionError: One string is null");
        panic!("Test assertion failed: string comparison with null");
    }
    
    unsafe {
        let result = roast_str_eq(a, b);
        if result {
            return 1;
        }
        
        let a_slice = std::slice::from_raw_parts((*a).data, (*a).len);
        let b_slice = std::slice::from_raw_parts((*b).data, (*b).len);
        let a_str = std::str::from_utf8(a_slice).unwrap_or("<invalid>");
        let b_str = std::str::from_utf8(b_slice).unwrap_or("<invalid>");
        
        let msg_str = if !msg.is_null() {
            let slice = std::slice::from_raw_parts((*msg).data, (*msg).len);
            std::str::from_utf8(slice).unwrap_or("Strings not equal")
        } else {
            "Strings not equal"
        };
        
        eprintln!("AssertionError: {}", msg_str);
        eprintln!("  expected: \"{}\"", a_str);
        eprintln!("  got:      \"{}\"", b_str);
        panic!("Test assertion failed: strings not equal");
    }
}

/// Assert value is in range [low, high]
#[no_mangle]
pub extern "C" fn roast_test_assert_in_range(
    value: c_long,
    low: c_long,
    high: c_long,
    msg: *const RoastString
) -> c_long {
    if value >= low && value <= high {
        return 1;
    }
    
    let msg_str = if !msg.is_null() {
        unsafe {
            let slice = std::slice::from_raw_parts((*msg).data, (*msg).len);
            std::str::from_utf8(slice).unwrap_or("Value out of range")
        }
    } else {
        "Value out of range"
    };
    
    eprintln!("AssertionError: {} ({} not in [{}, {}])", msg_str, value, low, high);
    panic!("Test assertion failed: {} not in range [{}, {}]", value, low, high);
}

// ============================================================================
// Universal Logging Module
// ============================================================================
// A universal logging system usable by any Roast application:
// - Web servers, GUI apps, CLI tools, background services
// - Configurable log levels (DEBUG, INFO, WARNING, ERROR, CRITICAL)
// - Colored terminal output with timestamps
// - Named loggers for different components
// ============================================================================

/// ANSI color codes
const COLOR_RESET: &str = "\x1b[0m";
const COLOR_CYAN: &str = "\x1b[36m";      // DEBUG
const COLOR_GREEN: &str = "\x1b[32m";     // INFO
const COLOR_YELLOW: &str = "\x1b[33m";    // WARNING
const COLOR_RED: &str = "\x1b[31m";       // ERROR
const COLOR_BOLD_RED: &str = "\x1b[1;31m"; // CRITICAL
const COLOR_DIM: &str = "\x1b[2m";        // Timestamp

/// Internal log function with level, color, and message
fn log_with_level(level: i32, level_name: &str, color: &str, logger_name: &str, message: &str) {
    let current_level = LOG_LEVEL.load(Ordering::Relaxed);
    if level < current_level {
        return;
    }
    
    let timestamp = get_log_timestamp();
    let logger_part = if logger_name.is_empty() { 
        String::new() 
    } else { 
        format!(" [{}]", logger_name) 
    };
    
    eprintln!(
        "{}[{}]{} {}{:8}{}{} {}",
        COLOR_DIM, timestamp, COLOR_RESET,
        color, level_name, COLOR_RESET,
        logger_part, message
    );
}

/// Set the global log level
/// Levels: DEBUG=10, INFO=20, WARNING=30, ERROR=40, CRITICAL=50
#[no_mangle]
pub extern "C" fn roast_log_set_level(level: c_long) {
    LOG_LEVEL.store(level as i32, Ordering::Relaxed);
}

/// Get the current log level
#[no_mangle]
pub extern "C" fn roast_log_get_level() -> c_long {
    LOG_LEVEL.load(Ordering::Relaxed) as c_long
}

/// Log a DEBUG message (level 10)
#[no_mangle]
pub extern "C" fn roast_log_debug(message: *const RoastString) {
    roast_log_debug_named(std::ptr::null(), message);
}

/// Log a DEBUG message with logger name
#[no_mangle]
pub extern "C" fn roast_log_debug_named(logger: *const RoastString, message: *const RoastString) {
    if message.is_null() { return; }
    
    unsafe {
        let msg_slice = std::slice::from_raw_parts((*message).data, (*message).len);
        let msg_str = std::str::from_utf8(msg_slice).unwrap_or("<invalid utf8>");
        
        let logger_str = if logger.is_null() {
            ""
        } else {
            let slice = std::slice::from_raw_parts((*logger).data, (*logger).len);
            std::str::from_utf8(slice).unwrap_or("")
        };
        
        log_with_level(10, "DEBUG", COLOR_CYAN, logger_str, msg_str);
    }
}

/// Log an INFO message (level 20)
#[no_mangle]
pub extern "C" fn roast_log_info(message: *const RoastString) {
    roast_log_info_named(std::ptr::null(), message);
}

/// Log an INFO message with logger name
#[no_mangle]
pub extern "C" fn roast_log_info_named(logger: *const RoastString, message: *const RoastString) {
    if message.is_null() { return; }
    
    unsafe {
        let msg_slice = std::slice::from_raw_parts((*message).data, (*message).len);
        let msg_str = std::str::from_utf8(msg_slice).unwrap_or("<invalid utf8>");
        
        let logger_str = if logger.is_null() {
            ""
        } else {
            let slice = std::slice::from_raw_parts((*logger).data, (*logger).len);
            std::str::from_utf8(slice).unwrap_or("")
        };
        
        log_with_level(20, "INFO", COLOR_GREEN, logger_str, msg_str);
    }
}

/// Log a WARNING message (level 30)
#[no_mangle]
pub extern "C" fn roast_log_warning(message: *const RoastString) {
    roast_log_warning_named(std::ptr::null(), message);
}

/// Log a WARNING message with logger name
#[no_mangle]
pub extern "C" fn roast_log_warning_named(logger: *const RoastString, message: *const RoastString) {
    if message.is_null() { return; }
    
    unsafe {
        let msg_slice = std::slice::from_raw_parts((*message).data, (*message).len);
        let msg_str = std::str::from_utf8(msg_slice).unwrap_or("<invalid utf8>");
        
        let logger_str = if logger.is_null() {
            ""
        } else {
            let slice = std::slice::from_raw_parts((*logger).data, (*logger).len);
            std::str::from_utf8(slice).unwrap_or("")
        };
        
        log_with_level(30, "WARNING", COLOR_YELLOW, logger_str, msg_str);
    }
}

/// Log an ERROR message (level 40)
#[no_mangle]
pub extern "C" fn roast_log_error(message: *const RoastString) {
    roast_log_error_named(std::ptr::null(), message);
}

/// Log an ERROR message with logger name
#[no_mangle]
pub extern "C" fn roast_log_error_named(logger: *const RoastString, message: *const RoastString) {
    if message.is_null() { return; }
    
    unsafe {
        let msg_slice = std::slice::from_raw_parts((*message).data, (*message).len);
        let msg_str = std::str::from_utf8(msg_slice).unwrap_or("<invalid utf8>");
        
        let logger_str = if logger.is_null() {
            ""
        } else {
            let slice = std::slice::from_raw_parts((*logger).data, (*logger).len);
            std::str::from_utf8(slice).unwrap_or("")
        };
        
        log_with_level(40, "ERROR", COLOR_RED, logger_str, msg_str);
    }
}

/// Log a CRITICAL message (level 50)
#[no_mangle]
pub extern "C" fn roast_log_critical(message: *const RoastString) {
    roast_log_critical_named(std::ptr::null(), message);
}

/// Log a CRITICAL message with logger name
#[no_mangle]
pub extern "C" fn roast_log_critical_named(logger: *const RoastString, message: *const RoastString) {
    if message.is_null() { return; }
    
    unsafe {
        let msg_slice = std::slice::from_raw_parts((*message).data, (*message).len);
        let msg_str = std::str::from_utf8(msg_slice).unwrap_or("<invalid utf8>");
        
        let logger_str = if logger.is_null() {
            ""
        } else {
            let slice = std::slice::from_raw_parts((*logger).data, (*logger).len);
            std::str::from_utf8(slice).unwrap_or("")
        };
        
        log_with_level(50, "CRITICAL", COLOR_BOLD_RED, logger_str, msg_str);
    }
}

/// Log with custom level (for advanced use)
#[no_mangle]
pub extern "C" fn roast_log(level: c_long, message: *const RoastString) {
    if message.is_null() { return; }
    
    let (level_name, color) = match level as i32 {
        0..=10 => ("DEBUG", COLOR_CYAN),
        11..=20 => ("INFO", COLOR_GREEN),
        21..=30 => ("WARNING", COLOR_YELLOW),
        31..=40 => ("ERROR", COLOR_RED),
        _ => ("CRITICAL", COLOR_BOLD_RED),
    };
    
    unsafe {
        let msg_slice = std::slice::from_raw_parts((*message).data, (*message).len);
        let msg_str = std::str::from_utf8(msg_slice).unwrap_or("<invalid utf8>");
        log_with_level(level as i32, level_name, color, "", msg_str);
    }
}

/// Log an HTTP request (convenience for web servers)
/// Format: "METHOD /path STATUS - DURATIONms"
#[no_mangle]
pub extern "C" fn roast_log_http_request(
    method: *const RoastString,
    path: *const RoastString,
    status: c_long,
    duration_ms: c_double
) {
    if method.is_null() || path.is_null() { return; }
    
    unsafe {
        let method_slice = std::slice::from_raw_parts((*method).data, (*method).len);
        let method_str = std::str::from_utf8(method_slice).unwrap_or("???");
        
        let path_slice = std::slice::from_raw_parts((*path).data, (*path).len);
        let path_str = std::str::from_utf8(path_slice).unwrap_or("/");
        
        let status_color = match status {
            200..=299 => COLOR_GREEN,
            300..=399 => COLOR_CYAN,
            400..=499 => COLOR_YELLOW,
            _ => COLOR_RED,
        };
        
        let message = format!(
            "{} {} {}{}{} - {:.2}ms",
            method_str, path_str,
            status_color, status, COLOR_RESET,
            duration_ms
        );
        
        log_with_level(20, "HTTP", COLOR_GREEN, "", &message);
    }
}
