//! Fast C API utilities for reducing GIL overhead.
//!
//! These functions use CPython's C API directly to create Python objects,
//! bypassing PyO3's safe wrappers for maximum performance on hot paths.
//! The key savings:
//! - No Result wrapping / error path overhead for infallible operations
//! - No trait dispatch (IntoPyObject, etc.)
//! - No Bound/Borrowed reference wrapper management
//! - For callbacks: vectorcall avoids tuple allocation entirely (Python 3.12+)
//! - For 0-arg callbacks: PyObject_CallNoArgs avoids tuple allocation

use pyo3::ffi;
use pyo3::prelude::*;
use std::ffi::c_char;
use std::os::raw::c_long;

// ─── Object Creation (C API) ────────────────────────────────────────────────

/// Create a `PyBytes` object from a byte slice using C API.
/// Avoids PyO3's `PyBytes::new()` wrapper overhead.
#[inline(always)]
pub unsafe fn bytes_from_slice(py: Python<'_>, data: &[u8]) -> Py<PyAny> {
    unsafe {
        let ptr = ffi::PyBytes_FromStringAndSize(
            data.as_ptr() as *const c_char,
            data.len() as ffi::Py_ssize_t,
        );
        Py::from_owned_ptr(py, ptr)
    }
}

/// Create a `PyUnicode` string from a Rust `&str` using C API.
/// Returns a new reference (raw pointer).
#[inline(always)]
pub unsafe fn string_from_str(s: &str) -> *mut ffi::PyObject {
    unsafe { ffi::PyUnicode_FromStringAndSize(s.as_ptr() as *const c_char, s.len() as ffi::Py_ssize_t) }
}

/// Create a `PyLong` from an `i32` using C API. Returns a new reference.
#[inline(always)]
pub unsafe fn long_from_i32(v: i32) -> *mut ffi::PyObject {
    unsafe { ffi::PyLong_FromLong(v as c_long) }
}

/// Create a `PyLong` from a `u16` using C API. Returns a new reference.
#[inline(always)]
pub unsafe fn long_from_u16(v: u16) -> *mut ffi::PyObject {
    unsafe { ffi::PyLong_FromLong(v as c_long) }
}

/// Create a `PyLong` from a `u32` using C API. Returns a new reference.
#[inline(always)]
pub unsafe fn long_from_u32(v: u32) -> *mut ffi::PyObject {
    unsafe { ffi::PyLong_FromUnsignedLong(v as std::ffi::c_ulong) }
}

// ─── Tuple Creation (C API, steals references) ─────────────────────────────

/// Create a 1-element tuple. **Steals** the reference to `a`.
#[inline(always)]
pub unsafe fn tuple1(a: *mut ffi::PyObject) -> *mut ffi::PyObject {
    unsafe {
        let t = ffi::PyTuple_New(1);
        ffi::PyTuple_SET_ITEM(t, 0, a);
        t
    }
}

/// Create a 2-element tuple. **Steals** references to `a` and `b`.
#[inline(always)]
pub unsafe fn tuple2(a: *mut ffi::PyObject, b: *mut ffi::PyObject) -> *mut ffi::PyObject {
    unsafe {
        let t = ffi::PyTuple_New(2);
        ffi::PyTuple_SET_ITEM(t, 0, a);
        ffi::PyTuple_SET_ITEM(t, 1, b);
        t
    }
}

/// Create a 4-element tuple. **Steals** references.
#[inline(always)]
pub unsafe fn tuple4(
    a: *mut ffi::PyObject,
    b: *mut ffi::PyObject,
    c: *mut ffi::PyObject,
    d: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        let t = ffi::PyTuple_New(4);
        ffi::PyTuple_SET_ITEM(t, 0, a);
        ffi::PyTuple_SET_ITEM(t, 1, b);
        ffi::PyTuple_SET_ITEM(t, 2, c);
        ffi::PyTuple_SET_ITEM(t, 3, d);
        t
    }
}

/// Create a 5-element tuple. **Steals** references.
#[inline(always)]
pub unsafe fn tuple5(
    a: *mut ffi::PyObject,
    b: *mut ffi::PyObject,
    c: *mut ffi::PyObject,
    d: *mut ffi::PyObject,
    e: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        let t = ffi::PyTuple_New(5);
        ffi::PyTuple_SET_ITEM(t, 0, a);
        ffi::PyTuple_SET_ITEM(t, 1, b);
        ffi::PyTuple_SET_ITEM(t, 2, c);
        ffi::PyTuple_SET_ITEM(t, 3, d);
        ffi::PyTuple_SET_ITEM(t, 4, e);
        t
    }
}

// ─── List Operations (C API) ────────────────────────────────────────────────

/// Create an empty `PyList`. Returns a new reference.
#[inline(always)]
pub unsafe fn list_new(size: usize) -> *mut ffi::PyObject {
    unsafe { ffi::PyList_New(size as ffi::Py_ssize_t) }
}

/// Append an item to a `PyList`. The item is **borrowed** (its refcount is incremented
/// by PyList_Append internally). Caller still owns `item` ref and should DECREF if needed.
#[inline(always)]
pub unsafe fn list_append(list: *mut ffi::PyObject, item: *mut ffi::PyObject) -> i32 {
    unsafe { ffi::PyList_Append(list, item) }
}

// ─── Callback Invocation (C API) ────────────────────────────────────────────

/// Execute a Python callback with arguments using the most efficient C API path.
///
/// - **0 args**: Uses `PyObject_CallNoArgs` (no tuple allocation at all)
/// - **N args**: Builds a minimal C-API tuple + `PyObject_Call`
#[inline(always)]
pub unsafe fn call_callback(
    py: Python<'_>,
    callable: *mut ffi::PyObject,
    args: &[Py<PyAny>],
) -> PyResult<()> {
    unsafe {
        let result = if args.is_empty() {
            ffi::PyObject_CallNoArgs(callable)
        } else {
            let nargs = args.len();
            let tuple = ffi::PyTuple_New(nargs as ffi::Py_ssize_t);
            for (i, arg) in args.iter().enumerate() {
                ffi::Py_INCREF(arg.as_ptr());
                ffi::PyTuple_SET_ITEM(tuple, i as ffi::Py_ssize_t, arg.as_ptr());
            }
            let r = ffi::PyObject_Call(callable, tuple, std::ptr::null_mut());
            ffi::Py_DECREF(tuple);
            r
        };

        if result.is_null() {
            Err(PyErr::fetch(py))
        } else {
            ffi::Py_DECREF(result);
            Ok(())
        }
    }
}

/// Execute a Python callback with arguments, ignoring errors.
/// Used for timer callbacks where errors are silently dropped.
#[inline(always)]
pub unsafe fn call_callback_ignore_err(
    callable: *mut ffi::PyObject,
    args: &[Py<PyAny>],
) {
    unsafe {
        let result = if args.is_empty() {
            ffi::PyObject_CallNoArgs(callable)
        } else {
            let nargs = args.len();
            let tuple = ffi::PyTuple_New(nargs as ffi::Py_ssize_t);
            for (i, arg) in args.iter().enumerate() {
                ffi::Py_INCREF(arg.as_ptr());
                ffi::PyTuple_SET_ITEM(tuple, i as ffi::Py_ssize_t, arg.as_ptr());
            }
            let r = ffi::PyObject_Call(callable, tuple, std::ptr::null_mut());
            ffi::Py_DECREF(tuple);
            r
        };

        if result.is_null() {
            ffi::PyErr_Clear();
        } else {
            ffi::Py_DECREF(result);
        }
    }
}
