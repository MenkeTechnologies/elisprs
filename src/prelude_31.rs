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

/// editfns.c's fields, minibuf.c's `minibufferp`, character.c's
/// `char-script-table` and keyboard.c's overriding keymaps: the C the editing
/// libraries below call.
pub const EDITFNS_C: &str = include_str!("lisp/editfns-c.el");

/// category.c, and the category data of characters.el.
pub const CATEGORY_C: &str = include_str!("lisp/category-c.el");

/// indent.el, verbatim (preloaded by loadup.el).
pub const INDENT: &str = include_str!("lisp/indent.el");

/// newcomment.el, verbatim (preloaded by loadup.el).
pub const NEWCOMMENT: &str = include_str!("lisp/newcomment.el");

/// textmodes/paragraphs.el, verbatim (preloaded by loadup.el).
pub const PARAGRAPHS: &str = include_str!("lisp/paragraphs.el");

/// textmodes/fill.el, verbatim (preloaded by loadup.el).
pub const FILL: &str = include_str!("lisp/fill.el");

/// simple.el, emacs-lisp/lisp.el and text-mode.el commands the core prelude
/// lacked, verbatim.
pub const SIMPLE_31: &str = include_str!("lisp/simple-31.el");

/// The `;;;###autoload` entries of the bundled libraries. Calling one loads its
/// library through `autoload-do-load`; the rest of a library's functions stay
/// unbound until it is `require`d, as in a stock `emacs -Q`.
pub const AUTOLOADS: &str = r#"
(autoload 'parse-time-string "parse-time" nil nil)
(autoload 'char-fold-to-regexp "char-fold" nil nil)
(autoload 'describe-char-fold-equivalences "char-fold" nil t)
(autoload 'rx "rx" nil nil t)
(autoload 'rx-to-string "rx" nil nil)
(autoload 'rx-let "rx" nil nil t)
(autoload 'rx-let-eval "rx" nil nil t)
(autoload 'rx-define "rx" nil nil t)
(autoload 'rx--pcase-macroexpander "rx" nil nil)
(define-symbol-prop 'rx 'pcase-macroexpander 'rx--pcase-macroexpander)
"#;

/// Every segment, in load order.
pub const SEGMENTS: [&str; 13] = [
    TIME_DATE, SUBR_31, KEYMAP_C, TEXTPROP_C, MAP_31, EDITFNS_C, CATEGORY_C, INDENT, NEWCOMMENT,
    PARAGRAPHS, FILL, SIMPLE_31, AUTOLOADS,
];
