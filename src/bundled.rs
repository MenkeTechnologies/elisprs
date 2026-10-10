//! Emacs Lisp libraries bundled verbatim from the GNU Emacs 31.1 tree.
//!
//! A library here is the unmodified upstream `.el` file. It is not evaluated at
//! startup: `load`/`require` fall back to this table when FILE names no file on
//! `load-path`, which is where Emacs itself finds its own `lisp/` directory
//! (the last `load-path` entry). A user file named like a bundled library
//! therefore shadows it, exactly as it would shadow the stock one.

/// One bundled upstream library.
pub struct Library {
    /// Feature/file stem, e.g. `parse-time`.
    pub name: &'static str,
    /// Directory below Emacs's `lisp/` the file lives in, e.g. `calendar`; empty for a file directly in `lisp/`.
    pub dir: &'static str,
    /// The upstream source, byte for byte.
    pub source: &'static str,
}

/// Every bundled library.
pub const LIBRARIES: &[Library] = &[
    Library {
        name: "char-fold",
        dir: "",
        source: include_str!("lisp/char-fold.el"),
    },
    Library {
        name: "char-fold-data",
        dir: "",
        source: include_str!("lisp/char-fold-data.el"),
    },
    Library {
        name: "iso8601",
        dir: "calendar",
        source: include_str!("lisp/iso8601.el"),
    },
    Library {
        name: "parse-time",
        dir: "calendar",
        source: include_str!("lisp/parse-time.el"),
    },
    Library {
        name: "thunk",
        dir: "emacs-lisp",
        source: include_str!("lisp/thunk.el"),
    },
];

/// The bundled library `file` names: a bare name, with or without `.el`.
/// A name with a directory component is a path, never a library.
pub fn find(file: &str) -> Option<&'static Library> {
    if file.contains('/') {
        return None;
    }
    let stem = file.strip_suffix(".el").unwrap_or(file);
    LIBRARIES.iter().find(|l| l.name == stem)
}

/// The path `load-file-name` reports while a bundled library loads.
pub fn virtual_path(lib: &Library) -> String {
    let root = "/usr/local/share/emacs/31.1/lisp";
    if lib.dir.is_empty() {
        format!("{root}/{}.el", lib.name)
    } else {
        format!("{root}/{}/{}.el", lib.dir, lib.name)
    }
}
