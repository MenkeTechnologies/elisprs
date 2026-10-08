//! Indentation and docstring properties of the preloaded definers and macros.
//!
//! Emacs's preloaded files are byte-compiled, so every `(declare (indent N)
//! (doc-string N))` in them has already run its `function-put`, and
//! lisp-mode.el adds the properties of the special forms and definers that
//! have no `declare`. elisprs's prelude dropped the `declare` of every macro
//! defined before its declaration bridge, left it out of many ports, and had
//! no lisp-mode.el block, so `(get 'when 'lisp-indent-function)` was nil.
//! Expectations are GNU Emacs 31.1 (`emacs -Q --batch`).

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn preloaded_indent_and_doc_string_properties() {
    check(
        "(mapcar (lambda (s) (list (get s 'lisp-indent-function) (get s 'doc-string-elt))) \
           '(when unless lambda let if progn prog1 prog2 condition-case unwind-protect catch \
             save-excursion defun defmacro defvar defconst defalias defvar-local defcustom \
             defgroup defface dolist dotimes with-temp-buffer with-current-buffer \
             with-silent-modifications ignore-errors save-match-data if-let* when-let* \
             letrec dlet seq-let pcase-let define-keymap))",
        "((1 nil) (1 nil) (defun 2) (1 nil) (2 nil) (0 nil) (1 nil) (2 nil) (2 nil) (1 nil) \
(1 nil) (0 nil) (2 3) (2 3) (defun 3) (defun 3) (defun 3) (defun 3) (defun 3) (defun 3) \
(defun 3) (1 nil) (1 nil) (0 nil) (1 nil) (0 nil) (0 nil) (0 nil) (2 nil) (1 nil) (1 nil) \
(1 nil) (2 nil) (nil nil) (defun nil))",
    );
    check(
        "(list (symbol-plist 'defmacro) (symbol-plist 'when))",
        "((doc-string-elt 3 lisp-indent-function 2 autoload-macro expand) \
(lisp-indent-function 1 edebug-form-spec t))",
    );
}

/// A user `defmacro`'s `declare` records its properties and is not part of
/// the body.
#[test]
fn user_declare_is_recorded_and_stripped() {
    check(
        "(progn (defmacro my-m (a &rest body) \"Doc.\" (declare (indent 1) (doc-string 3)) \
                  (list a body)) \
                (list (get 'my-m 'lisp-indent-function) (get 'my-m 'doc-string-elt) \
                      (symbol-function 'my-m)))",
        "(1 3 (macro . #[(a &rest body) ((list a body)) (t) nil \"Doc.\"]))",
    );
}
