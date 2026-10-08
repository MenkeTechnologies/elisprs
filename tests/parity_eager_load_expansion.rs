//! `load` expands each top-level form whole before running it (macroexp.el
//! `internal-macroexpand-for-load`), and an expander's error becomes
//! `(error "Eager macro-expansion failure: %S" ERR)`, signalled before any of
//! the form runs — so a `condition-case` inside the form does not catch it,
//! and the forms before it in the file have already run.
//!
//! elisprs let the raw error through, and left a COND-less `(when)` for run
//! time, where the form's own `condition-case` caught it.
//! Expectations are GNU Emacs 31.1 (`emacs -Q --batch`).

use elisprs::{eval_str, print, reset_host};

#[test]
fn load_wraps_expansion_failures() {
    let dir = std::env::temp_dir().join(format!("elisprs-eager-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let bad_macro = dir.join("bad-macro.el");
    std::fs::write(
        &bad_macro,
        ";;; -*- lexical-binding: t -*-\n(defmacro bad () (error \"boom\"))\n(prin1 (bad))\n",
    )
    .expect("write");
    let condless = dir.join("condless.el");
    std::fs::write(
        &condless,
        ";;; -*- lexical-binding: t -*-\n(setq eager-probe 1)\n\
         (prin1 (condition-case e (when) (error (list 'caught e))))\n(setq eager-probe 2)\n",
    )
    .expect("write");
    reset_host();
    let src = format!(
        "(list (condition-case e (load {:?} nil t) (error e)) \
               (condition-case e (load {:?} nil t) (error e)) \
               (bound-and-true-p eager-probe))",
        bad_macro.to_string_lossy(),
        condless.to_string_lossy()
    );
    let v = eval_str(&src).expect("eval failed");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        print(&v, true),
        "((error \"Eager macro-expansion failure: (error \\\"boom\\\")\") \
(error \"Eager macro-expansion failure: (wrong-number-of-arguments (1 . 1) 0)\") 1)"
    );
}
