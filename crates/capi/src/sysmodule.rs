use crate::pystate::with_vm;
use crate::unicodeobject::PyUnicode_FromFormatV;
use crate::util::FfiPtrExt;
use core::ffi::c_char;
use std::io::Write;

fn write_to_real_stdio(is_stderr: bool, text: &str) {
    if is_stderr {
        let _ = std::io::stderr().write_all(text.as_bytes());
    } else {
        let _ = std::io::stdout().write_all(text.as_bytes());
    }
}

fn write_to_sys_stream(
    vm: &rustpython_vm::VirtualMachine,
    name: &'static str,
    text: &str,
    is_stderr: bool,
) {
    let wrote_to_sys = vm
        .sys_module
        .get_attr(name, vm)
        .ok()
        .filter(|stream| !vm.is_none(stream))
        .is_some_and(|stream| vm.call_method(&stream, "write", (text,)).is_ok());

    if wrote_to_sys {
        return;
    }

    let _ = vm.take_raised_exception();
    write_to_real_stdio(is_stderr, text);
}

fn truncate_to_1000_bytes(text: &str) -> &str {
    if text.len() <= 1000 {
        return text;
    }
    let mut end = 0;
    for (i, _) in text.char_indices() {
        if i > 1000 {
            break;
        }
        end = i;
    }
    &text[..end]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn PySys_FormatStdout(format: *const c_char, args: ...) {
    with_vm::<(), ()>(|vm| {
        let formatted = unsafe { PyUnicode_FromFormatV(format, args) };
        let text = unsafe { formatted.assume_owned_or_opt() }
            .and_then(|obj| obj.str(vm).ok().map(|s| s.to_string()));

        if let Some(text) = text {
            write_to_sys_stream(vm, "stdout", &text, false);
        }
        let _ = vm.take_raised_exception();
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn PySys_FormatStderr(format: *const c_char, args: ...) {
    with_vm::<(), ()>(|vm| {
        let formatted = unsafe { PyUnicode_FromFormatV(format, args) };
        let text = unsafe { formatted.assume_owned_or_opt() }
            .and_then(|obj| obj.str(vm).ok().map(|s| s.to_string()));

        if let Some(text) = text {
            write_to_sys_stream(vm, "stderr", &text, true);
        }
        let _ = vm.take_raised_exception();
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn PySys_WriteStdout(format: *const c_char, args: ...) {
    with_vm::<(), ()>(|vm| {
        let formatted = unsafe { PyUnicode_FromFormatV(format, args) };
        let text = unsafe { formatted.assume_owned_or_opt() }
            .and_then(|obj| obj.str(vm).ok().map(|s| s.to_string()));

        if let Some(text) = text {
            write_to_sys_stream(vm, "stdout", truncate_to_1000_bytes(&text), false);
        }
        let _ = vm.take_raised_exception();
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn PySys_WriteStderr(format: *const c_char, args: ...) {
    with_vm::<(), ()>(|vm| {
        let formatted = unsafe { PyUnicode_FromFormatV(format, args) };
        let text = unsafe { formatted.assume_owned_or_opt() }
            .and_then(|obj| obj.str(vm).ok().map(|s| s.to_string()));

        if let Some(text) = text {
            write_to_sys_stream(vm, "stderr", truncate_to_1000_bytes(&text), true);
        }
        let _ = vm.take_raised_exception();
    });
}

#[cfg(test)]
mod tests {
    use pyo3::ffi;
    use pyo3::prelude::*;

    #[test]
    fn format_stdout_writes_to_sys_stdout() {
        Python::attach(|py| unsafe {
            let sys = py.import("sys").unwrap();
            let io = py.import("io").unwrap();
            let old_stdout = sys.getattr("stdout").unwrap();
            let capture = io.getattr("StringIO").unwrap().call0().unwrap();
            sys.setattr("stdout", &capture).unwrap();

            ffi::PySys_FormatStdout(c"value=%d".as_ptr(), 7);

            let captured: String = capture.call_method0("getvalue").unwrap().extract().unwrap();
            assert_eq!(captured, "value=7");
            sys.setattr("stdout", old_stdout).unwrap();
        });
    }

    #[test]
    fn write_stdout_truncates_at_1000_bytes() {
        Python::attach(|py| unsafe {
            let sys = py.import("sys").unwrap();
            let io = py.import("io").unwrap();
            let old_stdout = sys.getattr("stdout").unwrap();
            let capture = io.getattr("StringIO").unwrap().call0().unwrap();
            sys.setattr("stdout", &capture).unwrap();

            let long = std::ffi::CString::new("a".repeat(1200)).unwrap();
            ffi::PySys_WriteStdout(c"%s".as_ptr(), long.as_ptr());

            let captured: String = capture.call_method0("getvalue").unwrap().extract().unwrap();
            assert_eq!(captured.len(), 1000);
            sys.setattr("stdout", old_stdout).unwrap();
        });
    }
}
