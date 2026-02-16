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
        Bound::from_owned_ptr(py, ptr).into()
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
#[allow(dead_code)]
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

// ─── Callback Invocation (Vectorcall C API) ─────────────────────────────────

/// Call a Python callable with no arguments using C API.
/// Faster than PyO3's `cb.call0(py)` which internally creates an empty tuple.
#[inline(always)]
pub unsafe fn call_no_args(py: Python<'_>, callable: *mut ffi::PyObject) -> PyResult<()> {
    unsafe {
        let result = ffi::PyObject_CallNoArgs(callable);
        if result.is_null() {
            Err(PyErr::fetch(py))
        } else {
            ffi::Py_DECREF(result);
            Ok(())
        }
    }
}

/// Call a Python callable with exactly 1 argument using vectorcall.
/// Avoids tuple allocation entirely — args passed on the C stack.
/// This is the fastest possible path for 1-arg calls in CPython 3.9+.
#[inline(always)]
pub unsafe fn vectorcall_one_arg(
    py: Python<'_>,
    callable: *mut ffi::PyObject,
    arg: *mut ffi::PyObject,
) -> PyResult<()> {
    unsafe {
        let args = [arg];
        let result = ffi::PyObject_Vectorcall(
            callable,
            args.as_ptr(),
            1,
            std::ptr::null_mut(),
        );
        if result.is_null() {
            Err(PyErr::fetch(py))
        } else {
            ffi::Py_DECREF(result);
            Ok(())
        }
    }
}

/// Execute a Python callback with `Py<PyAny>` arguments using **vectorcall**.
///
/// - **0 args**: `PyObject_CallNoArgs` (no allocation at all)
/// - **1-2 args**: Stack array + `PyObject_Vectorcall` (no heap allocation, no tuple)
/// - **3+ args**: Heap Vec + `PyObject_Vectorcall` (rare, still no tuple overhead)
///
/// Vectorcall **borrows** argument references (no INCREF/DECREF per arg),
/// which is strictly faster than the old `PyTuple_New + SET_ITEM(steal) + PyObject_Call` path.
#[inline(always)]
pub unsafe fn call_callback(
    py: Python<'_>,
    callable: *mut ffi::PyObject,
    args: &[Py<PyAny>],
) -> PyResult<()> {
    unsafe {
        let result = match args.len() {
            0 => ffi::PyObject_CallNoArgs(callable),
            1 => {
                let ptrs = [args[0].as_ptr()];
                ffi::PyObject_Vectorcall(callable, ptrs.as_ptr(), 1, std::ptr::null_mut())
            }
            2 => {
                let ptrs = [args[0].as_ptr(), args[1].as_ptr()];
                ffi::PyObject_Vectorcall(callable, ptrs.as_ptr(), 2, std::ptr::null_mut())
            }
            n => {
                let ptrs: Vec<*mut ffi::PyObject> = args.iter().map(|a| a.as_ptr()).collect();
                ffi::PyObject_Vectorcall(
                    callable,
                    ptrs.as_ptr(),
                    n,
                    std::ptr::null_mut(),
                )
            }
        };

        if result.is_null() {
            Err(PyErr::fetch(py))
        } else {
            ffi::Py_DECREF(result);
            Ok(())
        }
    }
}

/// Execute a Python callback, ignoring errors. Used for timer callbacks.
/// Same vectorcall optimization as `call_callback`.
#[inline(always)]
pub unsafe fn call_callback_ignore_err(
    callable: *mut ffi::PyObject,
    args: &[Py<PyAny>],
) {
    unsafe {
        let result = match args.len() {
            0 => ffi::PyObject_CallNoArgs(callable),
            1 => {
                let ptrs = [args[0].as_ptr()];
                ffi::PyObject_Vectorcall(callable, ptrs.as_ptr(), 1, std::ptr::null_mut())
            }
            2 => {
                let ptrs = [args[0].as_ptr(), args[1].as_ptr()];
                ffi::PyObject_Vectorcall(callable, ptrs.as_ptr(), 2, std::ptr::null_mut())
            }
            n => {
                let ptrs: Vec<*mut ffi::PyObject> = args.iter().map(|a| a.as_ptr()).collect();
                ffi::PyObject_Vectorcall(
                    callable,
                    ptrs.as_ptr(),
                    n,
                    std::ptr::null_mut(),
                )
            }
        };

        if result.is_null() {
            ffi::PyErr_Clear();
        } else {
            ffi::Py_DECREF(result);
        }
    }
}
