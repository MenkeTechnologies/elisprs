//! End-to-end inline Rust FFI: a `rust { ... }` block is desugared, compiled to
//! a cdylib via `rustc`, `dlopen`ed, and its exports called from elisp by
//! bareword. Requires `rustc` on PATH (always present in Rust CI); skips cleanly
//! otherwise so a toolchain-less environment never reports a false failure.

fn rustc_available() -> bool {
    std::process::Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn rust_block_exports_are_callable_by_bareword() {
    if !rustc_available() {
        eprintln!("skipping FFI test: rustc not on PATH");
        return;
    }
    elisprs::reset_host();
    // Unique export names so this test's process-global registry entries never
    // collide with another test's. Exercises int-arity and float-arity
    // marshalling; `=` avoids depending on the exact float print format.
    let src = r#"rust {
    pub extern "C" fn el_ffi_addi(a: i64, b: i64) -> i64 { a + b }
    pub extern "C" fn el_ffi_mulf(x: f64, y: f64) -> f64 { x * y }
}
(if (and (= 42 (el_ffi_addi 21 21)) (= 7.0 (el_ffi_mulf 2.0 3.5))) 42 nil)
"#;
    let v = elisprs::eval_str(src).expect("FFI program should run");
    assert_eq!(elisprs::print(&v, true), "42");
}

#[test]
fn user_defun_shadows_ffi_export() {
    if !rustc_available() {
        return;
    }
    elisprs::reset_host();
    // A `defun` with the same name as an export must win: the function cell is
    // resolved before the `void-function` FFI fallback ever runs.
    let src = r#"rust { pub extern "C" fn el_ffi_shadow(a: i64, b: i64) -> i64 { a + b } }
(defun el_ffi_shadow (a b) (* a b))
(el_ffi_shadow 6 7)
"#;
    let v = elisprs::eval_str(src).expect("program should run");
    assert_eq!(elisprs::print(&v, true), "42");
}

#[test]
fn rust_block_with_no_exports_errors() {
    if !rustc_available() {
        return;
    }
    elisprs::reset_host();
    // A block with no `pub extern "C" fn` is a hard error — v1 requires at least
    // one exported function.
    let src = "rust { fn helper() -> i64 { 1 } }\n1\n";
    let err = elisprs::eval_str(src).expect_err("empty-export block must error");
    assert!(err.contains("rust FFI"), "unexpected error: {err}");
}
