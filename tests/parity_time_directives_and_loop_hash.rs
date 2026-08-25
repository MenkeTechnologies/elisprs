//! `format-time-string`'s ISO-week directives and case flags, and `cl-loop`'s
//! hash-table iteration order.
//!
//! Both were found by sweeping surfaces the other parity files had not reached,
//! and both are the same kind of bug: a thing that looked right because the
//! *values* were right and only the shape or the order was wrong.
//!
//! ```text
//!                                             emacs 31.1        elisprs (before)
//! (format-time-string "%U %W %V %G" T)        "00 01 01 2024"   "%U %W %V %G"
//! (format-time-string "%#a" 0 t)              "THU"             "Thu"
//! (cl-loop for k being the hash-keys of H collect k)
//!                                             (a b c)           (c b a)
//! ```
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1.
//! The timestamps are UTC (`format-time-string`'s ZONE argument is `t`) so the
//! answers do not depend on where the test runs.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `%G`/`%g`/`%V` are the ISO 8601 week-based year and week: a week runs Monday
/// to Sunday and belongs to the year containing its Thursday. These four
/// timestamps are the cases where that disagrees with the calendar year.
#[test]
fn iso_week_directives_follow_the_thursday_rule() {
    // 1970-01-01, a Thursday: week 01 of 1970.
    assert_eq!(eval("(format-time-string \"%G-%V\" 0 t)"), "\"1970-01\"");
    // 2024-01-01, a Monday: week 01 of 2024.
    assert_eq!(
        eval("(format-time-string \"%G-%V\" 1704067200 t)"),
        "\"2024-01\""
    );
    // 2025-01-01, a Wednesday: still week 01 of 2025.
    assert_eq!(
        eval("(format-time-string \"%G-%V\" 1735689600 t)"),
        "\"2025-01\""
    );
    // 2023-01-01, a Sunday: week 52 of 2022 — the year-before case.
    assert_eq!(
        eval("(format-time-string \"%G-%V\" 1672531200 t)"),
        "\"2022-52\""
    );
    // 2019-12-30, a Monday: already week 01 of 2020 — the year-after case.
    assert_eq!(
        eval("(format-time-string \"%G-%V\" 1577664000 t)"),
        "\"2020-01\""
    );
    // 2020 starts on a Wednesday and is a leap year, so 2020 has 53 ISO weeks
    // and 2020-12-31 is week 53.
    assert_eq!(
        eval("(format-time-string \"%G-%V\" 1609372800 t)"),
        "\"2020-53\""
    );
    // %g is the same year, two digits.
    assert_eq!(eval("(format-time-string \"%g\" 1672531200 t)"), "\"22\"");
}

/// `%U` and `%W` are the simpler thing the ISO week is often confused with:
/// weeks counted from the first Sunday and the first Monday, with the days
/// before it as week 00. On 2024-01-01 all three disagree.
#[test]
fn sunday_and_monday_week_numbers_are_not_the_iso_week() {
    assert_eq!(
        eval("(format-time-string \"%U %W %V\" 1704067200 t)"),
        "\"00 01 01\""
    );
    assert_eq!(
        eval("(format-time-string \"%U %W %V\" 1735689600 t)"),
        "\"00 00 01\""
    );
    assert_eq!(
        eval("(format-time-string \"%U %W %V\" 1700000000 t)"),
        "\"46 46 46\""
    );
    assert_eq!(eval("(format-time-string \"%U %W\" 0 t)"), "\"00 00\"");
    // %C is the century.
    assert_eq!(eval("(format-time-string \"%C\" 0 t)"), "\"19\"");
    assert_eq!(eval("(format-time-string \"%C\" 1700000000 t)"), "\"20\"");
}

/// `^` upcases; `#` changes case, which Emacs resolves as "upcase unless the
/// text is already caseless-or-upper" — NOT the swap-case glibc does, which
/// would make `%#a` "tHU".
#[test]
fn the_case_flags_upcase_or_flip() {
    assert_eq!(eval("(format-time-string \"%^a\" 0 t)"), "\"THU\"");
    assert_eq!(eval("(format-time-string \"%#a\" 0 t)"), "\"THU\"");
    assert_eq!(eval("(format-time-string \"%#A\" 0 t)"), "\"THURSDAY\"");
    assert_eq!(eval("(format-time-string \"%^B\" 0 t)"), "\"JANUARY\"");
    assert_eq!(eval("(format-time-string \"%#B\" 0 t)"), "\"JANUARY\"");
    // Already upper: `^` leaves it, `#` downcases it.
    assert_eq!(eval("(format-time-string \"%^p\" 3600 t)"), "\"AM\"");
    assert_eq!(eval("(format-time-string \"%#p\" 3600 t)"), "\"am\"");
    assert_eq!(eval("(format-time-string \"%#Z\" 0 t)"), "\"utc\"");
    // A caseless (numeric) directive is unaffected.
    assert_eq!(eval("(format-time-string \"%#j\" 0 t)"), "\"001\"");
    assert_eq!(eval("(format-time-string \"%#Y\" 0 t)"), "\"1970\"");
    // A field width right-aligns a STRING directive, as it does a numeric one.
    assert_eq!(
        eval("(format-time-string \"%^10a|\" 0 t)"),
        "\"       THU|\""
    );
}

/// `cl-loop`'s hash iteration follows the TABLE's order — which is `maphash`'s,
/// not `hash-table-keys`'s. Those two genuinely differ in Emacs, so routing the
/// loop through the wrong one reversed it.
#[test]
fn cl_loop_hash_iteration_uses_the_tables_own_order() {
    const H: &str = "(let ((h (make-hash-table :test 'eq))) \
                     (puthash 'a 1 h) (puthash 'b 2 h) (puthash 'c 3 h) ";
    assert_eq!(
        eval(&format!(
            "{H} (cl-loop for k being the hash-keys of h collect k))"
        )),
        "(a b c)"
    );
    assert_eq!(
        eval(&format!(
            "{H} (cl-loop for v being the hash-values of h collect v))"
        )),
        "(1 2 3)"
    );
    // `hash-table-keys` really is reverse-insertion order in Emacs; the loop
    // must NOT agree with it. Both are asserted so the distinction is pinned.
    assert_eq!(eval(&format!("{H} (hash-table-keys h))")), "(c b a)");
    assert_eq!(eval(&format!("{H} (hash-table-values h))")), "(3 2 1)");
    assert_eq!(
        eval(&format!(
            "{H} (let (r) (maphash (lambda (k _v) (setq r (cons k r))) h) (nreverse r)))"
        )),
        "(a b c)"
    );
}

/// `using` reaches the other half of the pair in BOTH directions. Only the
/// key-iteration companion was wired, because it could reach the value with
/// `gethash`; there is no `gethash` from a value back to a key.
#[test]
fn the_using_companion_works_in_both_directions() {
    const H: &str = "(let ((h (make-hash-table :test 'eq))) \
                     (puthash 'a 1 h) (puthash 'b 2 h) ";
    assert_eq!(
        eval(&format!(
            "{H} (cl-loop for k being the hash-keys of h using (hash-values v) \
             collect (cons k v)))"
        )),
        "((a . 1) (b . 2))"
    );
    assert_eq!(
        eval(&format!(
            "{H} (cl-loop for v being the hash-values of h using (hash-keys k) \
             collect (cons k v)))"
        )),
        "((a . 1) (b . 2))"
    );
    // An `equal`-test table iterates the same way.
    assert_eq!(
        eval(
            "(let ((h (make-hash-table :test 'equal))) (puthash \"a\" 1 h) \
              (puthash \"b\" 2 h) (cl-loop for k being the hash-keys of h collect k))"
        ),
        "(\"a\" \"b\")"
    );
}
