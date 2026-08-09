//! The AOT runtime must install the same numeric contract as the interpreter.
//!
//! `elisp --aot` emits an object that deserializes the chunk and runs it on a
//! fresh `VM` (fusevm's AOT model), calling back into elisprs through
//! `fusevm_aot_register_builtins` to set the VM up. That callback installed the
//! extension handlers and nothing else — no `set_numeric_hook`, no
//! `set_fixnum_range` — so an AOT-compiled program silently lost bignum
//! promotion while the interpreted one kept it. Verified against ground truth:
//!
//! ```text
//! $ emacs --batch -Q --eval '(princ (format "%s" (* 1000000000000 1000000000000)))'
//! 1000000000000000000000000
//! $ ./elisp -e '(* 1000000000000 1000000000000)'
//! 1000000000000000000000000
//! $ ./fix_aot                      # the same source, AOT-compiled
//! 2003764205206896640              # fusevm's native i64 Op::Mul, wrapped
//! ```
//!
//! and, purely from the missing range (no `i64` overflow involved at all),
//! `(bignump (+ most-positive-fixnum 1))` answered nil where Emacs says t.
//!
//! Building a real AOT executable needs `cc` and the platform frameworks, which
//! is not something a unit test should require. The test instead does what the
//! AOT entry point does — build a `VM` over a chunk and hand it to
//! `fusevm_aot_register_builtins` — so it exercises the exact callback that was
//! incomplete.

use elisprs::host::MOST_POSITIVE_FIXNUM;
use fusevm::{Value, VM};

/// Run `src` the way the AOT object does: compile it, then set the VM up through
/// the AOT registration hook alone (never through `host::run_chunk`, which is
/// the interpreter path and installs the contract separately).
fn run_via_aot_hook(src: &str) -> Value {
    elisprs::reset_host();
    let chunk = elisprs::compile_str(src).expect("compile");
    let mut vm = VM::new(chunk);
    unsafe { elisprs::aot_runtime::fusevm_aot_register_builtins(&mut vm) };
    match vm.run() {
        fusevm::VMResult::Ok(v) => v,
        fusevm::VMResult::Halted => vm.stack.last().cloned().unwrap_or(Value::Undef),
        fusevm::VMResult::Error(e) => panic!("AOT-configured VM errored: {e}"),
    }
}

fn printed(src: &str) -> String {
    let v = run_via_aot_hook(src);
    elisprs::print(&v, true)
}

/// An `i64`-overflowing product promotes to a bignum, exactly as in the
/// interpreter. Ground truth (`emacs --batch -Q`):
/// `(* 1000000000000 1000000000000)` => `1000000000000000000000000`.
#[test]
fn aot_hook_promotes_on_integer_overflow() {
    assert_eq!(
        printed("(* 1000000000000 1000000000000)"),
        "1000000000000000000000000"
    );
    // Ground truth: (+ 9223372036854775807 1) => 9223372036854775808.
    // Without the hook this answered the float 1.0.
    assert_eq!(printed("(+ 9223372036854775807 1)"), "9223372036854775808");
    // Ground truth: (* 4611686018427387903 4611686018427387903) =>
    // 21267647932558653957237540927630737409. Without the hook: 0.0.
    assert_eq!(
        printed("(* 4611686018427387903 4611686018427387903)"),
        "21267647932558653957237540927630737409"
    );
}

/// Crossing Emacs's 62-bit fixnum boundary promotes even though the result still
/// fits an `i64` — this one depends on `set_fixnum_range` alone, so it fails
/// whenever the range is not installed even if the numeric hook is. Ground
/// truth: `(bignump (+ most-positive-fixnum 1))` => `t`, `(fixnump …)` => nil.
#[test]
fn aot_hook_installs_emacs_fixnum_range() {
    let past = format!("(+ {MOST_POSITIVE_FIXNUM} 1)");
    assert_eq!(printed(&format!("(bignump {past})")), "t");
    assert_eq!(printed(&format!("(fixnump {past})")), "nil");
    // The value itself is right either way — which is what made the missing
    // range silent — so pin it too.
    assert_eq!(printed(&past), "2305843009213693952");
    // At the boundary it is still a fixnum.
    assert_eq!(printed(&format!("(fixnump {MOST_POSITIVE_FIXNUM})")), "t");
}
