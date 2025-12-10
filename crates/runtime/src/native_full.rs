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
    unsafe {
        let header = ptr as *mut ObjectHeader;
        if (*header).refcount.fetch_sub(1, Ordering::Release) == 1 {
            std::sync::atomic::fence(Ordering::Acquire);
            // Free based on type
            match (*header).type_tag {
                TypeTag::Str => roast_str_free(ptr as *mut RoastString),
                TypeTag::List => roast_list_free_internal(ptr as *mut RoastList),
                TypeTag::Dict => roast_dict_free_internal(ptr as *mut RoastDict),
                TypeTag::Set => roast_set_free_internal(ptr as *mut RoastSet),
                TypeTag::Tuple => roast_tuple_free_internal(ptr as *mut RoastTuple),
                TypeTag::Object => roast_object_free_internal(ptr as *mut RoastObject),
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
    let s = format!("{}", value);
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
        if !(*s).data.is_null() {
            let layout = Layout::from_size_align((*s).capacity, 1).unwrap();
            dealloc((*s).data, layout);
        }
        let layout = Layout::new::<RoastString>();
        dealloc(s as *mut u8, layout);
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
            if *(*list).data.add(i) == value {
                return true;
            }
        }
        false
    }
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
        let start = if start < 0 { (len + start).max(0) } else { start.min(len) };
        let end = if end < 0 { (len + end).max(0) } else { end.min(len) };
        let step = if step == 0 { 1 } else { step };

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
    map: *mut HashMap<c_long, c_long>,
}

#[no_mangle]
pub extern "C" fn roast_dict_new() -> *mut RoastDict {
    unsafe {
        let layout = Layout::new::<RoastDict>();
        let ptr = alloc(layout) as *mut RoastDict;

        (*ptr).header = ObjectHeader::new(TypeTag::Dict);
        (*ptr).map = Box::into_raw(Box::new(HashMap::new()));

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
    unsafe { (*(*dict).map).insert(hash_key, value); }
}

#[no_mangle]
pub extern "C" fn roast_dict_get(dict: *const RoastDict, key: c_long) -> c_long {
    if dict.is_null() { return 0; }
    let hash_key = dict_hash_key(key);
    unsafe { (*(*dict).map).get(&hash_key).copied().unwrap_or(0) }
}

#[no_mangle]
pub extern "C" fn roast_dict_contains(dict: *const RoastDict, key: c_long) -> bool {
    if dict.is_null() { return false; }
    let hash_key = dict_hash_key(key);
    unsafe { (*(*dict).map).contains_key(&hash_key) }
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
        for key in (*(*dict).map).keys() {
            roast_list_append(result, *key);
        }
    }
    result
}

#[no_mangle]
pub extern "C" fn roast_dict_values(dict: *const RoastDict) -> *mut RoastList {
    let result = roast_list_new(0);
    if dict.is_null() { return result; }
    unsafe {
        for value in (*(*dict).map).values() {
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
    unsafe { (*(*dict).map).get(&hash_key).copied().unwrap_or(default) }
}

/// Pop value (remove and return) with default if not found
#[no_mangle]
pub extern "C" fn roast_dict_pop(dict: *mut RoastDict, key: c_long, default: c_long) -> c_long {
    if dict.is_null() { return default; }
    let hash_key = dict_hash_key(key);
    unsafe { (*(*dict).map).remove(&hash_key).unwrap_or(default) }
}

/// Set value if key doesn't exist, return value
#[no_mangle]
pub extern "C" fn roast_dict_setdefault(dict: *mut RoastDict, key: c_long, default: c_long) -> c_long {
    if dict.is_null() { return default; }
    let hash_key = dict_hash_key(key);
    unsafe {
        *(*(*dict).map).entry(hash_key).or_insert(default)
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
        for (k, v) in (*(*dict).map).iter() {
            // Create a 2-tuple for each pair
            let tuple = roast_tuple_new(2);
            roast_tuple_set(tuple, 0, *k);
            roast_tuple_set(tuple, 1, *v);
            roast_list_append(result, tuple as c_long);
        }
    }
    result
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
        (*ptr).source = source;
        (*ptr).source_type = if source.is_null() { TypeTag::None } else { (*(source as *const ObjectHeader)).type_tag };
        (*ptr).index = 0;

        // Incref source
        roast_incref(source);

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
                let value = *(*s).data.add((*iter).index) as c_long;
                (*iter).index += 1;
                if !done.is_null() { *done = false; }
                value
            }
            _ => {
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

        ptr
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

        // Start from parent class
        if !current_class.is_null() && !(*current_class).parent.is_null() {
            (*ptr).start_class = (*current_class).parent;
        } else if !obj.is_null() && !(*obj).class.is_null() {
            // If no current class specified, derive from object's class
            (*ptr).start_class = (*(*obj).class).parent;
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
    println!("{}", value);
}

#[no_mangle]
pub extern "C" fn roast_print_float(value: c_double) {
    println!("{}", value);
}

#[no_mangle]
pub extern "C" fn roast_print_str(s: *const c_char) {
    if s.is_null() { return; }
    unsafe {
        if let Ok(string) = CStr::from_ptr(s).to_str() {
            println!("{}", string);
        }
    }
}

#[no_mangle]
pub extern "C" fn roast_print_bool(value: bool) {
    println!("{}", if value { "True" } else { "False" });
}

#[no_mangle]
pub extern "C" fn roast_print_newline() {
    println!();
}

#[no_mangle]
pub extern "C" fn roast_print_roast_str(s: *const RoastString) {
    if s.is_null() { println!("None"); return; }
    unsafe {
        let slice = std::slice::from_raw_parts((*s).data, (*s).len);
        if let Ok(string) = std::str::from_utf8(slice) {
            println!("{}", string);
        }
    }
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
    // - In a reasonable address range for heap (typically > 0x10000000000 on Linux x86_64)
    if value <= 0 {
        return false;
    }
    let ptr = value as u64;
    // Check alignment (heap pointers are 8-byte aligned)
    if ptr & 0x7 != 0 {
        return false;
    }
    // On Linux x86_64, heap addresses are typically in high memory
    // The mmap/heap region usually starts around 0x7f0000000000 or 0x555555...
    // Values below 0x100000000 (4GB) are almost certainly integers, not pointers
    // This handles all fibonacci results and most integer calculations
    if ptr < 0x100000000 {
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
                                print!("{}", elem);
                            }
                            println!("]");
                            return 0;
                        }
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

/// Convert any value to string (Python str() builtin)
#[no_mangle]
pub extern "C" fn roast_str(value: c_long) -> c_long {
    let ptr = value as *const c_void;

    // Handle null
    if ptr.is_null() {
        let s = CString::new("None").unwrap();
        return roast_str_from_cstr(s.as_ptr()) as c_long;
    }

    // Check if it's a likely pointer to an object
    if is_likely_pointer(value) {
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
                            s.push_str(&format!("{}", elem));
                        }
                        s.push(']');
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
// Async (Stub - for future implementation)
// ============================================================================

#[no_mangle]
pub extern "C" fn roast_await(_future: *mut c_void) -> c_long {
    // Stub for async - will need proper coroutine support
    0
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
