//! regex-emacs.c's reading of postfix-operator runs, intervals applied to a
//! repeated expression, context-dependent `^`/`$`, and `\=` (point).
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn match_end(re: &str, s: &str) -> String {
    eval(&format!(
        "(condition-case e (progn (string-match \"{re}\" \"{s}\") (match-end 0)) (error e))"
    ))
}

/// A run of `*`, `+` and `?` is one operator: many if any of `*`/`+`, zero
/// if any of `*`/`?`, and non-greedy when a `?` follows either.
#[test]
fn postfix_runs_collapse_to_one_operator() {
    assert_eq!(match_end("a**", "aaa"), "3");
    assert_eq!(match_end("a+*", "aaa"), "3");
    assert_eq!(match_end("a?+", "aaa"), "3");
    assert_eq!(match_end("a*??", "aaa"), "0");
    assert_eq!(match_end("a+?*", "aaa"), "0");
    assert_eq!(match_end(".**", "abc"), "3");
    assert_eq!(match_end("\\\\s-**", "  x"), "2");
}

/// An operator after a repeated expression repeats the whole of it.
#[test]
fn an_operator_after_an_interval_repeats_the_interval() {
    assert_eq!(match_end("a\\\\{2\\\\}*", "aaaaa"), "4");
    assert_eq!(match_end("a*\\\\{2\\\\}", "aaaaa"), "5");
    assert_eq!(match_end("a\\\\{1\\\\}\\\\{2\\\\}", "aaaaa"), "2");
    assert_eq!(match_end("a\\\\{,2\\\\}\\\\{2\\\\}", "aaaaa"), "4");
    assert_eq!(match_end("\\\\(ab\\\\)\\\\{2\\\\}*", "ababababa"), "8");
    // With nothing to repeat, a valid interval's `\{` is a literal `{`.
    assert_eq!(eval("(string-match \"\\\\{2\\\\}\" \"{2}\")"), "0");
}

/// `^` anchors only at the start or after `\(`/`\|`, `$` only at the end or
/// before `\)`/`\|`; elsewhere both are ordinary characters.
#[test]
fn caret_and_dollar_depend_on_context() {
    assert_eq!(eval("(string-match \"a^b\" \"a^b\")"), "0");
    assert_eq!(eval("(string-match \"a$b\" \"a$b\")"), "0");
    assert_eq!(eval("(string-match \"x\\\\(^a\\\\)\" \"x\\na\")"), "nil");
    assert_eq!(eval("(string-match \"a$\\\\|b\" \"a\")"), "0");
    assert_eq!(eval("(string-match \"^*a\" \"*a\")"), "0");
}

/// `\=` is point: it never matches in a string, and in a buffer it holds
/// only where point is, whichever way the search goes.
#[test]
fn backslash_equals_is_point() {
    assert_eq!(eval("(string-match \"\\\\=a\" \"a\")"), "nil");
    assert_eq!(eval("(string-match \"\\\\=\" \"\")"), "nil");
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 1) (re-search-forward \"\\\\=b\" nil t))"),
        "nil"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 2) (re-search-forward \"\\\\=b\" nil t))"),
        "3"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcabc\") (goto-char 3) \
             (list (re-search-backward \"b\\\\=c\" nil t) (point)))"
        ),
        "(nil 3)"
    );
    // A backward match may not run past point, so `\=` can only end it.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcabc\") (goto-char 3) \
             (list (re-search-backward \"b\\\\=\" nil t) (point)))"
        ),
        "(2 2)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"xyz abc\") (narrow-to-region 5 8) (goto-char 6) \
             (list (looking-back \"a\" nil) (re-search-backward \"a\\\\=\" nil t)))"
        ),
        "(t 5)"
    );
}
