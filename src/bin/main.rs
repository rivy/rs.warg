#![allow(non_snake_case)]

// ref: https://github.com/rust-lang/rust/issues/44650
// ref: https://github.com/rust-lang/rust/blob/c34fbfaad38cf5829ef5cfe780dc9d58480adeaa/src/libstd/sys/windows/args.rs @@ https://archive.is/IVc4N
// ref: https://github.com/rust-lang/rust/blob/1cdd68922d143c6d1f18f66572251b7078e9e850/src/libstd/sys/windows/args.rs#L29
// ref: https://github.com/rust-lang/rust/blob/master/src/libstd/sys/windows/args.rs
// ref: https://www.cs.brandeis.edu/~cs146a/rust/doc-02-21-2015/src/std/os.rs.html @@ http://archive.is/HcNl0

// spell-checker:ignore (crate/winapi) winapi processenv shellapi winbase ctypes LPCWSTR
// spell-checker:ignore (vars) cwstr wstr

use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;
use winapi::um::processenv::GetCommandLineW;
use winapi::um::shellapi::CommandLineToArgvW;
use winapi::um::winbase::LocalFree;

pub fn raw_command_line() -> Option<&'static [u16]> {
    unsafe {
        let line_ptr = GetCommandLineW();
        if line_ptr.is_null() {
            return None;
        }
        let mut len = 0;
        while *line_ptr.offset(len as isize) != 0 {
            len += 1;
        }
        Some(std::slice::from_raw_parts(line_ptr, len))
    }
}

pub fn command_line() -> Option<OsString> {
    match raw_command_line() {
        Some(v) => Some(OsString::from_wide(v)),
        None => None,
    }
}


struct CWSTR {
    wstr: Vec<u16>
}

impl CWSTR {
    fn as_ptr(&self) -> *const u16 {
        self.wstr.as_ptr()
    }
}

impl<T: Into<OsString>> From<T> for CWSTR {
    fn from(source: T) -> CWSTR {
        CWSTR { wstr: source.into().as_os_str().encode_wide().chain(Some(0).into_iter()).collect::<Vec<_>>() }
    }
}

fn toArgvW_args<T: Into<CWSTR>>(line: T) -> Vec<OsString> {
    use std::slice;
    use winapi::ctypes::{c_int, c_void};

    let mut nArgs: c_int = 0;
    let lpArgCount: *mut c_int = &mut nArgs;
    // let lpCmdLine = unsafe { GetCommandLineW() };
    let szArgList = unsafe { CommandLineToArgvW(line.into().as_ptr(), lpArgCount) };

    let args: Vec<OsString> = (0..nArgs as usize).map(|i| unsafe {
        // Determine the length of this argument.
        let ptr = *szArgList.offset(i as isize);
        let mut len = 0;
        while *ptr.offset(len as isize) != 0 { len += 1; }

        // Push it onto the list.
        let ptr = ptr as *const u16;
        let buf = slice::from_raw_parts(ptr, len);
        let opt_s = OsString::from_wide(&buf);
        // opt_s.ok().expect("CommandLineToArgvW returned invalid UTF-16")
        opt_s
    }).collect();

    unsafe {
        LocalFree(szArgList as *mut c_void);
    }

    return args
}

fn main() {
    let line_raw = raw_command_line().unwrap();
    println!("The raw command line (&[u16]) is: {:?}", line_raw);
    let line = command_line().unwrap();
    println!("The command line (OsString) is: {:?}", line);
    println!("The command line (to_string_lossy) is: {:?}", line.to_string_lossy());
    let cl_to_argv = toArgvW_args(line);
    // let cl_to_argv = toArgvW_args("test this");
    println!("CommandLineToArgvW are: {:?}", cl_to_argv);
    // let globs = wild::globs().unwrap();
    // println!("The globs are: {:?}", globs.collect::<Vec<_>>());
    let args = wild::args_os();
    println!("The args are: {:?}", args.collect::<Vec<_>>());
}
