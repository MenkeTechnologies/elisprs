//! Emacs 31.1 library segments loaded after the core prelude.
//!
//! Each segment is an upstream file included verbatim (see `src/lisp/`), or a
//! table of `autoload` forms for libraries that [`crate::bundled`] loads on
//! first use.

/// time-date.el, verbatim. Its `format-seconds`, `seconds-to-string`,
/// `date-to-time`, `decoded-time-add` and friends are preloaded here the way
/// the rest of the prelude is.
pub const TIME_DATE: &str = include_str!("lisp/time-date.el");

/// Definitions from Emacs 31.1 `subr.el` and `simple.el` the core prelude lacked,
/// verbatim.
pub const SUBR_31: &str = include_str!("lisp/subr-31.el");

/// Ports of the C primitives of `keymap.c` and `keyboard.c` that read keymaps
/// and event symbols.
pub const KEYMAP_C: &str = include_str!("lisp/keymap-c.el");

/// `textprop.c` queries that combine overlays with text properties.
pub const TEXTPROP_C: &str = include_str!("lisp/textprop-c.el");

/// `map.el`'s generic functions, dispatched by hand on `list`, `hash-table` and
/// `array`.
pub const MAP_31: &str = include_str!("lisp/map-31.el");

/// The `;;;###autoload` entries of the bundled libraries. Calling one loads its
/// library through `autoload-do-load`; the rest of a library's functions stay
/// unbound until it is `require`d, as in a stock `emacs -Q`.
pub const AUTOLOADS: &str = r#"
(autoload 'parse-time-string "parse-time" nil nil)
(autoload 'char-fold-to-regexp "char-fold" nil nil)
(autoload 'describe-char-fold-equivalences "char-fold" nil t)
"#;

/// Every segment, in load order.
pub const SEGMENTS: [&str; 6] = [TIME_DATE, SUBR_31, KEYMAP_C, TEXTPROP_C, MAP_31, AUTOLOADS];
