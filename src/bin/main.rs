#![allow(non_snake_case)]

// ref: http://daviddeley.com/autohotkey/parameters/parameters.htm @@ http://archive.is/9jM4q

// ref: https://github.com/rust-lang/rust/issues/44650
// ref: https://github.com/rust-lang/rust/blob/c34fbfaad38cf5829ef5cfe780dc9d58480adeaa/src/libstd/sys/windows/args.rs @@ https://archive.is/IVc4N
// ref: https://github.com/rust-lang/rust/blob/1cdd68922d143c6d1f18f66572251b7078e9e850/src/libstd/sys/windows/args.rs#L29
// ref: https://github.com/rust-lang/rust/blob/master/src/libstd/sys/windows/args.rs
// ref: https://www.cs.brandeis.edu/~cs146a/rust/doc-02-21-2015/src/std/os.rs.html @@ http://archive.is/HcNl0

// ref: [WTF-8/UCS-2/WTF-16 (aka potentially ill-formed UTF-16)] https://simonsapin.github.io/wtf-8 @@ https://archive.is/mpdT0
// ref: [Wikipedia ~ UTF-16](https://en.wikipedia.org/wiki/UTF-16)

// ref: https://www.reddit.com/r/rust/comments/4r161o/why_not_osstrnewchars_and_etc @@ https://archive.is/HJapK

// spell-checker:ignore (crate/winapi) winapi processenv shellapi winbase ctypes LPCWSTR
// spell-checker:ignore (vars) cwstr wstr

// ## notable argument constructions
// `EXE this "that""the other"` => differs between pre2008 and 2008+
// `EXE this "that"\"the other"` => shows a bug in `CommandLineToArgvW()`, separating out 'other' as a separate last argument

use std::ffi::OsStr;
use std::ffi::OsString;

use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;
use winapi::um::processenv::GetCommandLineW;
use winapi::um::shellapi::CommandLineToArgvW;
use winapi::um::winbase::LocalFree;

/// raw WTF-16/UCS-2 string from `GetCommandLineW()`
pub fn command_line_raw() -> Option<&'static [u16]> {
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

/// OsString from `GetCommandLineW()`
pub fn command_line() -> Option<OsString> {
    match command_line_raw() {
        Some(v) => Some(OsString::from_wide(v)),
        None => None,
    }
}

/// NUL-terminated UCS-2/WTF-16 string
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

fn CommandLineToArgvW_args<T: Into<CWSTR>>(line: T) -> Vec<OsString> {
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

enum TokenStatePre2008 {
    OutsideToken,
    InToken(/* quoted */ bool),
    OnDoubleQuote(/* quoted */ bool),
    OnBackslash(/* n_backslashes */ usize, /* quoted */ bool),
}

#[cfg(not(windows))]
type OsStrCharType = u8;
#[cfg(windows)]
type OsStrCharType = u16;

fn tokens_pre2008<T: AsRef<OsStr>>(line: T) -> Vec<OsString> {
    let substring: Vec<OsStrCharType> = line.as_ref().encode_wide().collect();
    let mut tokens: Vec<OsString> = vec![];
    let mut token: Vec<OsStrCharType> = vec![];

    use self::TokenStatePre2008::*;
    let mut state: TokenStatePre2008 = OutsideToken;

    let length = substring.len();
    for (i, &cu) in substring.iter().enumerate() {
        state = match state {
            OutsideToken => match cu {
                c if (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => OutsideToken,
                c if c == OsStrCharType::from(b'"') => InToken(true),
                c if c == OsStrCharType::from(b'\\') => OnBackslash(1, false),
                c => {
                    token.push(c);
                    InToken(false)
                }
            },
            InToken(quoted) => match cu {
                c if c == OsStrCharType::from(b'\\') => OnBackslash(1, quoted),
                c if quoted && c == OsStrCharType::from(b'"') => OnDoubleQuote(quoted),
                c if !quoted && c == OsStrCharType::from(b'"') => InToken(true),
                c if !quoted && (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => {
                    tokens.push(OsString::from_wide(&token));
                    token = vec![];
                    OutsideToken
                },
                c => {
                    token.push(c);
                    InToken(quoted)
                },
            },
            OnDoubleQuote(quoted) => match cu {
                c if c == OsStrCharType::from(b'"') => {
                    // (pre-2008) In quoted arg "" means literal quote and the end of the quoted string (but not arg)
                    token.push(c);
                    InToken(false)
                },
                c if (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => {
                    tokens.push(OsString::from_wide(&token));
                    token = vec![];
                    OutsideToken
                },
                c => {
                    token.push(c);
                    InToken(false)
                },
            },
            OnBackslash(count, quoted) => match cu {
                c if c == OsStrCharType::from(b'"') => {
                    // backslashes followed by a quotation mark are treated as pairs of protected backslashes
                    for _ in 0..count/2 {
                        token.push(OsStrCharType::from(b'\\'));
                    }

                    if count & 1 != 0 {
                        // An odd number of backslashes is treated as followed by a protected quotation mark.
                        token.push(c);
                        InToken(quoted)
                    } else if quoted {
                        // An even number of backslashes followed by a double-quote terminates the double-quote
                        tokens.push(OsString::from_wide(&token));
                        token = vec![];
                        InToken(!quoted)
                    } else {
                        InToken(quoted)
                    }
                },
                c if (c == OsStrCharType::from(b'\\') && i < length-1) => OnBackslash(count + 1, quoted),
                c => {
                    // A string of backslashes not followed by a quotation mark has no special meaning.
                    for _ in 0..count {
                        token.push(OsStrCharType::from(b'\\'));
                    }
                    token.push(c);
                    InToken(quoted)
                },
            },
        }
    }

    if !token.is_empty() {
        tokens.push(OsString::from_wide(&token))
    }

    tokens
}

enum TokenState2008 {
    OutsideToken,
    InToken(/* quoted */ bool),
    OnDoubleQuote(/* quoted */ bool),
    OnBackslash(/* n_backslashes */ usize, /* quoted */ bool),
}

fn tokens_2008<T: AsRef<OsStr>>(line: T) -> Vec<OsString> {
    let substring: Vec<OsStrCharType> = line.as_ref().encode_wide().collect();
    let mut tokens: Vec<OsString> = vec![];
    let mut token: Vec<OsStrCharType> = vec![];

    use self::TokenState2008::*;
    let mut state: TokenState2008 = OutsideToken;

    let length = substring.len();
    for (i, &cu) in substring.iter().enumerate() {
        state = match state {
            OutsideToken => match cu {
                c if (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => OutsideToken,
                c if c == OsStrCharType::from(b'"') => InToken(true),
                c if c == OsStrCharType::from(b'\\') => OnBackslash(1, false),
                c => {
                    token.push(c);
                    InToken(false)
                }
            },
            InToken(quoted) => match cu {
                c if c == u16::from(b'\\') => OnBackslash(1, quoted),
                c if quoted && c == OsStrCharType::from(b'"') => OnDoubleQuote(quoted),
                c if !quoted && c == OsStrCharType::from(b'"') => InToken(true),
                c if !quoted && (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => {
                    tokens.push(OsString::from_wide(&token));
                    token = vec![];
                    OutsideToken
                },
                c => {
                    token.push(c);
                    InToken(quoted)
                },
            },
            OnDoubleQuote(quoted) => match cu {
                c if c == OsStrCharType::from(b'"') => {
                    // (2008) In quoted arg "" means literal quote and *continue* the quoted string
                    token.push(c);
                    InToken(true)
                },
                c if (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => {
                    tokens.push(OsString::from_wide(&token));
                    token = vec![];
                    OutsideToken
                },
                c => {
                    token.push(c);
                    InToken(false)
                },
            },
            OnBackslash(count, quoted) => match cu {
                c if c == OsStrCharType::from(b'"') => {
                    // backslashes followed by a quotation mark are treated as pairs of protected backslashes
                    for _ in 0..count/2 {
                        token.push(OsStrCharType::from(b'\\'));
                    }

                    if count & 1 != 0 {
                        // An odd number of backslashes is treated as followed by a protected quotation mark.
                        token.push(c);
                        InToken(quoted)
                    } else if quoted {
                        // An even number of backslashes is treated as followed by a word terminator.
                        tokens.push(OsString::from_wide(&token));
                        token = vec![];
                        OutsideToken
                    } else {
                        InToken(quoted)
                    }
                },
                c if (c == OsStrCharType::from(b'\\') && i < length-1) => OnBackslash(count + 1, quoted),
                c => {
                    // A string of backslashes not followed by a quotation mark has no special meaning.
                    for _ in 0..count {
                        token.push(OsStrCharType::from(b'\\'));
                    }
                    token.push(c);
                    InToken(quoted)
                },
            },
        }
    }

    if !token.is_empty() {
        tokens.push(OsString::from_wide(&token))
    }

    tokens
}

#[derive(PartialEq)]
enum TokenQuoteType {
    Literal,
    DoubleQuote,
    SingleQuote,
}
enum TokenState {
    OutsideToken,
    InToken(/* quoted */ TokenQuoteType),
    OnDoubleQuote(/* quoted */ TokenQuoteType),
    OnSingleQuote(/* quoted */ TokenQuoteType),
    OnBackslash(/* n_backslashes */ usize, /* quoted */ TokenQuoteType),
}

fn tokens<T: AsRef<OsStr>>(line: T) -> Vec<OsString> {
    let substring: Vec<OsStrCharType> = line.as_ref().encode_wide().collect();
    let mut tokens: Vec<OsString> = vec![];
    let mut token: Vec<OsStrCharType> = vec![];

    use self::TokenState::*;
    let mut state: TokenState = OutsideToken;

    let length = substring.len();
    for (i, &cu) in substring.iter().enumerate() {
        state = match state {
            OutsideToken => match cu {
                c if (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => OutsideToken,
                c if c == OsStrCharType::from(b'"') => InToken(TokenQuoteType::DoubleQuote),
                c if c == OsStrCharType::from(b'\\') => OnBackslash(1, TokenQuoteType::Literal),
                c => {
                    token.push(c);
                    InToken(TokenQuoteType::Literal)
                }
            },
            InToken(quoted) => match cu {
                c if c == u16::from(b'\\') => OnBackslash(1, quoted),
                c if quoted != TokenQuoteType::Literal && c == OsStrCharType::from(b'"') => OnDoubleQuote(quoted),
                c if quoted == TokenQuoteType::Literal && c == OsStrCharType::from(b'"') => InToken(TokenQuoteType::DoubleQuote),
                c if quoted == TokenQuoteType::Literal && (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => {
                    tokens.push(OsString::from_wide(&token));
                    token = vec![];
                    OutsideToken
                },
                c => {
                    token.push(c);
                    InToken(quoted)
                },
            },
            OnDoubleQuote(quoted) => match cu {
                c if c == OsStrCharType::from(b'"') => {
                    // (2008) In quoted arg "" means literal quote and *continue* the quoted string
                    token.push(c);
                    InToken(TokenQuoteType::DoubleQuote)
                },
                c if (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => {
                    tokens.push(OsString::from_wide(&token));
                    token = vec![];
                    OutsideToken
                },
                c => {
                    token.push(c);
                    InToken(TokenQuoteType::Literal)
                },
            },
            OnSingleQuote(quoted) => match cu {
                c if c == OsStrCharType::from(b'\'') => {
                    // WIP .... NA .... (2008) In quoted arg "" means literal quote and *continue* the quoted string
                    token.push(c);
                    InToken(TokenQuoteType::DoubleQuote)
                },
                c if (c == OsStrCharType::from(b' ') || c == OsStrCharType::from(b'\t')) => {
                    tokens.push(OsString::from_wide(&token));
                    token = vec![];
                    OutsideToken
                },
                c => {
                    token.push(c);
                    InToken(TokenQuoteType::Literal)
                },
            },
            OnBackslash(count, quoted) => match cu {
                c if c == OsStrCharType::from(b'"') => {
                    // backslashes followed by a quotation mark are treated as pairs of protected backslashes
                    for _ in 0..count/2 {
                        token.push(OsStrCharType::from(b'\\'));
                    }

                    if count & 1 != 0 {
                        // An odd number of backslashes is treated as followed by a protected quotation mark.
                        token.push(c);
                        InToken(quoted)
                    } else {
                        match quoted {
                            q if q == TokenQuoteType::SingleQuote || q == TokenQuoteType::DoubleQuote => {
                                // An even number of backslashes is treated as followed by a word terminator.
                                tokens.push(OsString::from_wide(&token));
                                token = vec![];
                                OutsideToken
                                },
                            _ => { InToken(quoted) }
                        }
                    }
                },
                c if (c == OsStrCharType::from(b'\\') && i < length-1) => OnBackslash(count + 1, quoted),
                c => {
                    // A string of backslashes not followed by a quotation mark has no special meaning.
                    for _ in 0..count {
                        token.push(OsStrCharType::from(b'\\'));
                    }
                    token.push(c);
                    InToken(quoted)
                },
            },
        }
    }

    if !token.is_empty() {
        tokens.push(OsString::from_wide(&token))
    }

    tokens
}

fn main() {
    let line_raw = command_line_raw().unwrap();
    println!("The raw command line (&[u16]) is: {:?}", line_raw);
    let line = command_line().unwrap();
    println!("The command line (OsString) is: {:?}", &line);
    println!("The command line (to_string_lossy) is: {:?}", line.to_string_lossy());
    let cl_to_argv = CommandLineToArgvW_args(&line);
    // let cl_to_argv = toArgvW_args("test this");
    println!("CommandLineToArgvW are: {:?}", cl_to_argv);
    println!("tokens_pre2008 are: {:?}", tokens_pre2008(&line));
    println!("tokens_2008 are: {:?}", tokens_2008(&line));
    // let globs = wild::globs().unwrap();
    // println!("The globs are: {:?}", globs.collect::<Vec<_>>());
    let args = wild::args_os();
    println!("The args are: {:?}", args.collect::<Vec<_>>());
}
