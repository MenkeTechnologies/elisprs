//! Unit tests for the ported ERT framework (`should` / `should-not` /
//! `should-error` / `ert-deftest`), exercised directly through the library.
//!
//! Each test resets the host (which reloads the prelude, where ERT lives) and
//! evaluates a small suite ending in `(ert-run-tests-batch)`, whose return value
//! is the number of failing tests.

fn failures(suite: &str) -> String {
    elisprs::reset_host();
    let v = elisprs::eval_str(suite).unwrap_or_else(|e| panic!("eval error: {e}"));
    elisprs::print(&v, true)
}

#[test]
fn canonical_example_passes() {
    // The example from the Emacs Lisp manual: a passing `should` and a
    // `should-error` matching its `:type`.
    let n = failures(
        r#"
(ert-deftest my-math-test ()
  "Ensure basic addition works."
  (should (= (+ 1 2) 3))
  (should-error (/ 1 0) :type 'arith-error))
(ert-run-tests-batch)"#,
    );
    assert_eq!(n, "0", "canonical ERT example should pass");
}

#[test]
fn failing_assertions_are_counted() {
    // One pass, one failed `should`, one `should-error` that saw no error.
    let n = failures(
        r#"
(ert-deftest a-pass () (should (= 1 1)))
(ert-deftest a-bad-assert () (should (= 1 2)))
(ert-deftest a-no-error () (should-error (+ 1 1)))
(ert-run-tests-batch)"#,
    );
    assert_eq!(n, "2");
}

#[test]
fn should_error_type_must_match() {
    // `should-error` with the wrong `:type` fails the test.
    let n = failures(
        r#"
(ert-deftest wrong-type () (should-error (/ 1 0) :type 'some-other-error))
(ert-run-tests-batch)"#,
    );
    assert_eq!(n, "1");
}

#[test]
fn should_not_works() {
    let n = failures(
        r#"
(ert-deftest sn-pass () (should-not (eq 'a 'b)))
(ert-deftest sn-fail () (should-not (= 1 1)))
(ert-run-tests-batch)"#,
    );
    assert_eq!(n, "1");
}

#[test]
fn expected_failure_is_not_unexpected() {
    // A test marked `:expected-result :failed` that fails is an XFAIL → not
    // counted; a normal passing test alongside it keeps the count at 0.
    let n = failures(
        r#"
(ert-deftest xfail () :expected-result :failed (should (= 1 2)))
(ert-deftest ok () (should (= 1 1)))
(ert-run-tests-batch)"#,
    );
    assert_eq!(n, "0");
}

#[test]
fn unexpected_pass_is_counted() {
    // A test marked `:expected-result :failed` that *passes* is unexpected.
    let n = failures(
        r#"
(ert-deftest should-have-failed () :expected-result :failed (should (= 1 1)))
(ert-run-tests-batch)"#,
    );
    assert_eq!(n, "1");
}

#[test]
fn skip_unless_skips_without_failing() {
    let n = failures(
        r#"
(ert-deftest skipped () (skip-unless nil) (should (= 1 2)))
(ert-run-tests-batch)"#,
    );
    assert_eq!(n, "0");
}

#[test]
fn fail_pass_info_primitives() {
    // The primitives under `should`: `ert-fail` signals `ert-test-failed`
    // (a failing test); `ert-pass` is a no-op leaving the test passing;
    // `ert-info` annotates without failing. One failure expected.
    let n = failures(
        r#"
(ert-deftest explicit-fail () (ert-fail "nope"))
(ert-deftest explicit-pass () (ert-info "noting") (ert-pass) (should t))
(ert-run-tests-batch)"#,
    );
    assert_eq!(n, "1");
}

#[test]
fn should_failure_explains_subform_values() {
    // A failing `should` on a known predicate reports the form and each
    // sub-form's evaluated value (ERT-style explanation), reachable as the
    // `ert-test-failed` signal's data.
    let s =
        failures(r#"(condition-case e (should (= (+ 1 2) 4)) (ert-test-failed (car (cdr e))))"#);
    assert!(s.contains("should (= (+ 1 2) 4)"), "missing form: {s}");
    assert!(s.contains("(+ 1 2) . 3"), "missing explained value: {s}");
}
