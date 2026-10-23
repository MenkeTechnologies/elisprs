//! The file-name primitives of Emacs's `src/fileio.c`, Unix branch:
//! `file-name-directory`, `file-name-nondirectory`, `file-name-as-directory`,
//! `directory-file-name`, `directory-name-p`, `file-name-concat`,
//! `file-name-absolute-p`, `expand-file-name` and `substitute-in-file-name`.
//!
//! Each function follows its C counterpart's byte walk. File name handlers are
//! not dispatched (elisprs has none), and the `DOS_NT` / `WINDOWSNT` branches
//! are not ported. Strings are UTF-8, and every split point the walks choose is
//! an ASCII `/`, `.` or `~`, so slicing the bytes never splits a character.

use crate::host::ElispHost;
use fusevm::Value;

type R = Result<Value, String>;

fn is_nil(v: &Value) -> bool {
    matches!(v, Value::Undef | Value::Bool(false))
}

/// `CHECK_STRING`.
fn check_string(h: &ElispHost, v: &Value) -> Result<String, String> {
    match h.str_text(v) {
        Some(s) => Ok(s.to_string()),
        None => Err(format!("wrong-type-argument: stringp {}", h.print(v, true))),
    }
}

/// `CHECK_STRING_NULL_BYTES`: a file name may not contain a NUL byte.
fn check_null_bytes(h: &ElispHost, v: &Value, s: &str) -> Result<(), String> {
    if s.as_bytes().contains(&0) {
        return Err(format!(
            "wrong-type-argument: filenamep {}",
            h.print(v, true)
        ));
    }
    Ok(())
}

/// The byte at `i`, or the C string's terminating NUL past the end.
fn at(b: &[u8], i: usize) -> u8 {
    b.get(i).copied().unwrap_or(0)
}

/// The index just past the last `/` of `s`, or `None` when it has none.
fn after_last_sep(s: &str) -> Option<usize> {
    s.rfind('/').map(|i| i + 1)
}

/// `file_name_as_directory`: "" is "./", otherwise ensure a trailing `/`.
fn as_directory(s: &str) -> String {
    if s.is_empty() {
        return "./".to_string();
    }
    if s.ends_with('/') {
        s.to_string()
    } else {
        format!("{s}/")
    }
}

/// `directory_file_name`: drop the trailing slashes, except that "//" stays
/// as it is and a name of three or more slashes becomes "/".
fn directory_file(s: &str) -> &str {
    let b = s.as_bytes();
    let mut len = b.len();
    if !(len == 2 && b[0] == b'/') {
        while len > 1 && b[len - 1] == b'/' {
            len -= 1;
        }
    }
    &s[..len]
}

/// `user_homedir`: the home directory of the user named by NAME up to the
/// next `/`, or `None` when the name is empty, no such user exists, or the
/// user's home directory is not absolute.
fn user_homedir(name: &[u8]) -> Option<String> {
    let len = name
        .iter()
        .position(|&c| c == b'/' || c == 0)
        .unwrap_or(name.len());
    if len == 0 {
        return None;
    }
    let cname = std::ffi::CString::new(&name[..len]).ok()?;
    // SAFETY: getpwnam takes a NUL-terminated string and returns either NULL
    // or a pointer to a static passwd record, which is read before any other
    // call can overwrite it.
    unsafe {
        let pw = libc::getpwnam(cname.as_ptr());
        if pw.is_null() || (*pw).pw_dir.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr((*pw).pw_dir).to_string_lossy();
        if !dir.starts_with('/') {
            return None;
        }
        Some(dir.into_owned())
    }
}

/// `get_homedir`: $HOME, else the password entry of $LOGNAME, $USER or the
/// real uid, made absolute against the working directory.
fn get_homedir() -> String {
    let home = std::env::var("HOME").ok().or_else(|| {
        let by_name = ["LOGNAME", "USER"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .find_map(|u| user_homedir(u.as_bytes()));
        // SAFETY: as in `user_homedir`; getpwuid's record is read at once.
        by_name.or_else(|| unsafe {
            let pw = libc::getpwuid(libc::getuid());
            if pw.is_null() || (*pw).pw_dir.is_null() {
                None
            } else {
                Some(
                    std::ffi::CStr::from_ptr((*pw).pw_dir)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        })
    });
    let Some(home) = home else {
        return String::new();
    };
    if home.starts_with('/') {
        return home;
    }
    let wd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "/".to_string());
    format!("{}{home}", as_directory(&wd))
}

/// `file_name_absolute_p`: `/…`, `~`, `~/…`, or `~USER…` for a USER that
/// exists.
fn absolute_p(s: &str) -> bool {
    let b = s.as_bytes();
    at(b, 0) == b'/'
        || (at(b, 0) == b'~' && (b.len() == 1 || b[1] == b'/' || user_homedir(&b[1..]).is_some()))
}

/// `(file-name-directory FILENAME)`.
pub fn file_name_directory(h: &mut ElispHost, a: &[Value]) -> R {
    let s = check_string(h, &a[0])?;
    Ok(match after_last_sep(&s) {
        Some(i) => h.new_string(&s[..i]),
        None => Value::Undef,
    })
}

/// `(file-name-nondirectory FILENAME)`.
pub fn file_name_nondirectory(h: &mut ElispHost, a: &[Value]) -> R {
    let s = check_string(h, &a[0])?;
    let i = after_last_sep(&s).unwrap_or(0);
    Ok(h.new_string(&s[i..]))
}

/// `(file-name-as-directory FILE)`.
pub fn file_name_as_directory(h: &mut ElispHost, a: &[Value]) -> R {
    let s = check_string(h, &a[0])?;
    Ok(h.new_string(as_directory(&s)))
}

/// `(directory-file-name DIRECTORY)`.
pub fn directory_file_name(h: &mut ElispHost, a: &[Value]) -> R {
    let s = check_string(h, &a[0])?;
    Ok(h.new_string(directory_file(&s)))
}

/// `(directory-name-p NAME)`.
pub fn directory_name_p(h: &mut ElispHost, a: &[Value]) -> R {
    let s = check_string(h, &a[0])?;
    Ok(if s.ends_with('/') {
        Value::Bool(true)
    } else {
        Value::Undef
    })
}

/// `(file-name-concat DIRECTORY &rest COMPONENTS)`: nil and "" elements are
/// skipped, every other element is a string, and a `/` goes between two
/// elements unless the first already ends in one.
pub fn file_name_concat(h: &mut ElispHost, a: &[Value]) -> R {
    let mut parts = Vec::with_capacity(a.len());
    for v in a {
        if is_nil(v) {
            continue;
        }
        let s = check_string(h, v)?;
        if !s.is_empty() {
            parts.push(s);
        }
    }
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        out.push_str(p);
        if i + 1 < parts.len() && !out.ends_with('/') {
            out.push('/');
        }
    }
    Ok(h.new_string(out))
}

/// `(file-name-absolute-p FILENAME)`.
pub fn file_name_absolute_p(h: &mut ElispHost, a: &[Value]) -> R {
    let s = check_string(h, &a[0])?;
    Ok(if absolute_p(&s) {
        Value::Bool(true)
    } else {
        Value::Undef
    })
}

/// `(expand-file-name NAME &optional DEFAULT-DIRECTORY)`.
pub fn expand_file_name(h: &mut ElispHost, a: &[Value]) -> R {
    let dd = a.get(1).cloned().unwrap_or(Value::Undef);
    let out = expand(h, &a[0], dd)?;
    Ok(match out {
        Expanded::Unchanged => a[0].clone(),
        Expanded::New(s) => h.new_string(s),
    })
}

enum Expanded {
    /// NAME was already absolute and canonical: Emacs returns it as is.
    Unchanged,
    New(String),
}

fn var_string(h: &mut ElispHost, name: &str) -> Option<(Value, String)> {
    let sym = h.intern(name);
    let v = h.get_dynamic_value(&sym).ok()?;
    let s = h.str_text(&v)?.to_string();
    Some((v, s))
}

/// The expansion of a string object, as a Rust string.
fn expand_to_string(h: &mut ElispHost, name: &Value, dd: Value) -> Result<String, String> {
    match expand(h, name, dd)? {
        Expanded::Unchanged => check_string(h, name),
        Expanded::New(s) => Ok(s),
    }
}

/// `Fexpand_file_name`, Unix branch.
fn expand(h: &mut ElispHost, name_v: &Value, dd_in: Value) -> Result<Expanded, String> {
    let name = check_string(h, name_v)?;
    check_null_bytes(h, name_v, &name)?;
    let root = |h: &mut ElispHost| h.new_string("/");

    // Use the buffer's `default-directory' if DEFAULT-DIRECTORY is omitted.
    // A relative one is expanded against `invocation-directory' when that is
    // absolute, else against the root.
    let mut dd = dd_in;
    if is_nil(&dd) {
        if let Some((dir_v, dir)) = var_string(h, "default-directory") {
            if dir.starts_with('/') {
                check_null_bytes(h, &dir_v, &dir)?;
                dd = dir_v;
            } else {
                let absdir = match var_string(h, "invocation-directory") {
                    Some((v, s)) if s.starts_with('/') => v,
                    _ => root(h),
                };
                let s = expand_to_string(h, &dir_v, absdir)?;
                dd = h.new_string(s);
            }
        }
    }
    if h.str_text(&dd).is_none() {
        dd = root(h);
    }

    // A relative default directory is expanded first; the identity test is
    // what stops the recursion.
    let dd_str = h.str_text(&dd).unwrap_or("/").to_string();
    if dd != *name_v && !dd_str.starts_with('/') {
        let s = expand_to_string(h, &dd, Value::Undef)?;
        dd = h.new_string(s);
    }
    let dd_str = h.str_text(&dd).unwrap_or("/").to_string();

    let nb = name.as_bytes();

    // An absolute NAME with no "/.", "/..", or "//" (past a leading "//") is
    // already expanded.
    if at(nb, 0) == b'/' {
        // "." and ".." are hairy; multiple slashes collapse, except a leading "//".
        let dots = |p: usize| {
            at(nb, p + 1) == b'.'
                && (at(nb, p + 2) == b'/'
                    || at(nb, p + 2) == 0
                    || (at(nb, p + 2) == b'.' && (at(nb, p + 3) == b'/' || at(nb, p + 3) == 0)))
        };
        let slashes = |p: usize| at(nb, p + 1) == b'/' && (p != 0 || at(nb, p + 2) == b'/');
        let lose = (0..nb.len()).any(|p| nb[p] == b'/' && (dots(p) || slashes(p)));
        if !lose {
            return Ok(Expanded::Unchanged);
        }
    }

    // NEWDIR is the prefix: the home directory for `~' or `~USER', else the
    // default directory when NM is relative.
    let mut nm: &[u8] = nb;
    let mut newdir: Option<String> = None;
    if at(nm, 0) == b'~' {
        if at(nm, 1) == b'/' || nm.len() == 1 {
            newdir = Some(get_homedir());
            nm = &nm[1..];
        } else if let Some(home) = user_homedir(&nm[1..]) {
            newdir = Some(home);
            let end = nm[1..]
                .iter()
                .position(|&c| c == b'/')
                .map_or(nm.len(), |i| i + 1);
            nm = &nm[end..];
        }
        // An unknown user leaves the name unchanged.
    }
    if at(nm, 0) != b'/' && newdir.is_none() {
        newdir = Some(dd_str);
    }

    let mut target: Vec<u8> = Vec::with_capacity(nm.len() + 64);
    if let Some(dir) = &newdir {
        // Ignore any slash at the end of NEWDIR, unless it is just "/" or "//".
        let db = dir.as_bytes();
        let mut length = db.len();
        while length > 1 && db[length - 1] == b'/' && !(length == 2 && db[0] == b'/') {
            length -= 1;
        }
        if nm.is_empty() || nm[0] == b'/' {
            target.extend_from_slice(&db[..length]);
        } else {
            target.extend_from_slice(as_directory(&dir[..length]).as_bytes());
        }
    }
    target.extend_from_slice(nm);

    // Canonicalize in place: remove "//", "/." and "/foo/..".
    let t = &mut target;
    let (mut p, mut o) = (0usize, 0usize);
    while p < t.len() {
        if t[p] != b'/' {
            t[o] = t[p];
            o += 1;
            p += 1;
        } else if at(t, p + 1) == b'.' && (at(t, p + 2) == b'/' || at(t, p + 2) == 0) {
            // A "/." that is the entire name keeps its "/".
            if o == 0 && at(t, p + 2) == 0 {
                t[o] = t[p];
                o += 1;
            }
            p += 2;
        } else if at(t, p + 1) == b'.'
            && at(t, p + 2) == b'.'
            && o != 0
            && (at(t, p + 3) == b'/' || at(t, p + 3) == 0)
        {
            loop {
                o -= 1;
                if o == 0 || t[o] == b'/' {
                    break;
                }
            }
            // Keep the initial "/" only if this is the whole name.
            if o == 0 && t[0] == b'/' && at(t, p + 3) == 0 {
                o += 1;
            }
            p += 3;
        } else if at(t, p + 1) == b'/' && (p != 0 || at(t, p + 2) == b'/') {
            // Collapse multiple "/", except leave a leading "//" alone.
            p += 1;
        } else {
            t[o] = t[p];
            o += 1;
            p += 1;
        }
    }
    target.truncate(o);
    Ok(Expanded::New(String::from_utf8_lossy(&target).into_owned()))
}

/// `file_name_case_insensitive_err`: -1 case-insensitive, 0 case-sensitive,
/// 1 undetermined. Only macOS answers `_PC_CASE_SENSITIVE`; every other Unix
/// is case-sensitive.
fn case_insensitive_err(file: &str) -> i32 {
    #[cfg(target_os = "macos")]
    {
        let Ok(c) = std::ffi::CString::new(file) else {
            return 1;
        };
        // SAFETY: pathconf reads a NUL-terminated path.
        let res = unsafe { libc::pathconf(c.as_ptr(), libc::_PC_CASE_SENSITIVE) };
        if res >= 0 {
            return -i32::from(res == 0);
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINVAL) {
            return 1;
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = file;
    0
}

/// `(file-name-case-insensitive-p FILENAME)`: walk up from the expanded name
/// to the first directory whose file system answers.
pub fn file_name_case_insensitive_p(h: &mut ElispHost, a: &[Value]) -> R {
    check_string(h, &a[0])?;
    let mut filename = expand_to_string(h, &a[0], Value::Undef)?;
    loop {
        let err = case_insensitive_err(&filename);
        if err <= 0 {
            return Ok(if err < 0 {
                Value::Bool(true)
            } else {
                Value::Undef
            });
        }
        match after_last_sep(&filename) {
            Some(i) if filename[..i] != filename => filename.truncate(i),
            _ => return Ok(Value::Undef),
        }
    }
}
