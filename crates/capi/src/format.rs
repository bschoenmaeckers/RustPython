use core::ffi::{CStr, VaList, c_char, c_int, c_long, c_uint, c_ulong, c_void};
use rustpython_vm::common::wtf8::{CodePoint, Wtf8Buf};
use rustpython_vm::{PyResult, VirtualMachine};

pub(crate) enum FormatTarget {
    Bytes,
    Unicode,
}

pub(crate) unsafe fn from_format_v_to_bytes(
    vm: &VirtualMachine,
    format: *const c_char,
    mut args: VaList<'_>,
    target: FormatTarget,
) -> PyResult<Vec<u8>> {
    let format = unsafe { CStr::from_ptr(format) }.to_bytes();
    let mut result = Vec::with_capacity(format.len());
    let mut pos = 0;
    while pos < format.len() {
        if format[pos] != b'%' {
            result.push(format[pos]);
            pos += 1;
            continue;
        }
        let start = pos;
        pos += 1;
        // The C API ignores field width and applies precision only to %s.
        while format.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        let mut precision = 0usize;
        if format.get(pos) == Some(&b'.') {
            pos += 1;
            while let Some(&digit) = format.get(pos).filter(|b| b.is_ascii_digit()) {
                precision = precision
                    .saturating_mul(10)
                    .saturating_add((digit - b'0').into());
                pos += 1;
            }
        }
        while format
            .get(pos)
            .is_some_and(|b| *b != b'%' && !b.is_ascii_alphabetic())
        {
            pos += 1;
        }
        let modifier = if matches!(format.get(pos), Some(b'l' | b'z'))
            && matches!(format.get(pos + 1), Some(b'd' | b'u'))
        {
            let modifier = format[pos];
            pos += 1;
            Some(modifier)
        } else {
            None
        };
        match format.get(pos) {
            Some(b'c') => {
                let value = unsafe { args.next_arg::<c_int>() };
                match target {
                    FormatTarget::Bytes => {
                        let value = u8::try_from(value).map_err(|_| {
                            vm.new_overflow_error(
                                "PyBytes_FromFormatV(): %c format expects an integer in range [0; 255]",
                            )
                        })?;
                        result.push(value);
                    }
                    FormatTarget::Unicode => {
                        let value = u32::try_from(value).map_err(|_| {
                            vm.new_overflow_error(
                                "PyUnicode_FromFormatV(): %c format expects an integer in range [0; 1114111]",
                            )
                        })?;
                        let cp = CodePoint::from_u32(value).ok_or_else(|| {
                            vm.new_overflow_error(
                                "PyUnicode_FromFormatV(): %c format expects an integer in range [0; 1114111]",
                            )
                        })?;
                        let mut ch = Wtf8Buf::new();
                        ch.push(cp);
                        result.extend_from_slice(ch.as_bytes());
                    }
                }
            }
            Some(b'd') => {
                let value = match modifier {
                    Some(b'l') => unsafe { args.next_arg::<c_long>() }.to_string(),
                    Some(b'z') => unsafe { args.next_arg::<isize>() }.to_string(),
                    _ => unsafe { args.next_arg::<c_int>() }.to_string(),
                };
                result.extend_from_slice(value.as_bytes());
            }
            Some(b'u') => {
                let value = match modifier {
                    Some(b'l') => unsafe { args.next_arg::<c_ulong>() }.to_string(),
                    Some(b'z') => unsafe { args.next_arg::<usize>() }.to_string(),
                    _ => unsafe { args.next_arg::<c_uint>() }.to_string(),
                };
                result.extend_from_slice(value.as_bytes());
            }
            Some(b'i') => {
                result.extend_from_slice(unsafe { args.next_arg::<c_int>() }.to_string().as_bytes())
            }
            Some(b'x') => result.extend_from_slice(
                format!("{:x}", unsafe { args.next_arg::<c_int>() } as c_uint).as_bytes(),
            ),
            Some(b's') => {
                let string = unsafe { args.next_arg::<*const c_char>() };
                if precision == 0 {
                    result.extend_from_slice(unsafe { CStr::from_ptr(string) }.to_bytes());
                } else {
                    // A precision-limited string need not have a terminator
                    // beyond the specified readable region.
                    for offset in 0..precision {
                        let byte = unsafe { *string.add(offset) } as u8;
                        if byte == 0 {
                            break;
                        }
                        result.push(byte);
                    }
                }
            }
            Some(b'p') => {
                let pointer = unsafe { args.next_arg::<*const c_void>() };
                // Preserve the platform's %p spelling, including Windows
                // uppercase digits, but always supply the C API's 0x prefix.
                let mut buffer = [0 as c_char; 2 * size_of::<usize>() + 16];
                unsafe {
                    libc::snprintf(buffer.as_mut_ptr(), buffer.len(), c"%p".as_ptr(), pointer);
                }
                let pointer = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_bytes();
                result.extend_from_slice(b"0x");
                result.extend_from_slice(
                    if pointer.starts_with(b"0x") || pointer.starts_with(b"0X") {
                        &pointer[2..]
                    } else {
                        pointer
                    },
                );
            }
            Some(b'%') => result.push(b'%'),
            _ => {
                // The first unrecognized format copies the remaining
                // format verbatim, without consuming any more arguments.
                result.extend_from_slice(&format[start..]);
                break;
            }
        }
        pos += 1;
    }
    Ok(result)
}
