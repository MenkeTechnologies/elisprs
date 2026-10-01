//! Round 32: json.c's parser and serializer (`json-parse-string`,
//! `json-parse-buffer`, `json-serialize`, `json-insert`) and eval.c's `signal`
//! argument checks.
//!
//! Every expectation was byte-checked against GNU Emacs 31.1
//! (`emacs -Q --batch`, `lexical-binding` t).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn parse_err(json: &str) -> String {
    eval(&format!(
        "(condition-case e (json-parse-string {}) (error e))",
        lisp_string(json)
    ))
}

/// JSON text as an elisp string literal.
fn lisp_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Errors are `(SYMBOL LINE nil POSITION)`: POSITION counts the characters
/// consumed, the offending one included; end of input reports the length.
#[test]
fn parse_errors_carry_line_and_position() {
    let cases = [
        ("[1,", "(json-end-of-file 1 nil 3)"),
        ("", "(json-end-of-file 1 nil 0)"),
        ("[1] x", "(json-trailing-content 1 nil 5)"),
        ("{\"a\" 1}", "(json-parse-error 1 nil 6)"),
        ("[1,]", "(json-parse-error 1 nil 4)"),
        ("{\"a\":1,}", "(json-parse-error 1 nil 8)"),
        ("01", "(json-trailing-content 1 nil 2)"),
        ("\"abc", "(json-end-of-file 1 nil 4)"),
        ("[1,\n2,", "(json-end-of-file 2 nil 6)"),
        ("\n\n  tru", "(json-parse-error 3 nil 7)"),
        ("\"a\nb\"", "(json-parse-error 2 nil 3)"),
        ("[\"é\" x", "(json-parse-error 1 nil 6)"),
        ("1.e5", "(json-parse-error 1 nil 3)"),
        ("1e+", "(json-end-of-file 1 nil 3)"),
        ("-x", "(json-parse-error 1 nil 2)"),
        ("true1", "(json-parse-error 1 nil 5)"),
        ("true-", "(json-parse-error 1 nil 5)"),
        ("true_", "(json-trailing-content 1 nil 5)"),
        ("nan", "(json-parse-error 1 nil 2)"),
        ("1.8e308", "(json-number-out-of-range-error 1 nil 7)"),
    ];
    for (json, want) in cases {
        assert_eq!(parse_err(json), want, "json-parse-string {json:?}");
    }
}

/// `\u` escapes: a high surrogate needs a following low one, a lone low one is
/// an error, and a bad hex digit or escape letter is an escape-sequence error.
#[test]
fn parse_string_escapes_and_surrogates() {
    assert_eq!(
        eval("(json-parse-string \"\\\"\\\\ud83d\\\\ude00\\\\u00e9\\\\/\\\\t\\\"\")"),
        "\"😀é/\t\""
    );
    let cases = [
        ("\"\\ud800\"", "(json-invalid-surrogate-error 1 nil 8)"),
        ("\"\\ud83d\\x\"", "(json-invalid-surrogate-error 1 nil 9)"),
        (
            "\"\\ud800\\u0041\"",
            "(json-invalid-surrogate-error 1 nil 13)",
        ),
        ("\"\\udc00\"", "(json-invalid-surrogate-error 1 nil 7)"),
        ("\"\\u12\"", "(json-escape-sequence-error 1 nil 6)"),
        ("\"a\\q\"", "(json-escape-sequence-error 1 nil 4)"),
        ("\"\\ud83d", "(json-end-of-file 1 nil 7)"),
    ];
    for (json, want) in cases {
        assert_eq!(parse_err(json), want, "json-parse-string {json:?}");
    }
}

#[test]
fn parse_values_and_object_types() {
    assert_eq!(
        eval(
            "(json-parse-string \"[1.5e3, -0.0, 1E-2, -12, 123456789012345678901234567890, 1e-400]\")"
        ),
        "[1500.0 -0.0 0.01 -12 123456789012345678901234567890 0.0]"
    );
    assert_eq!(
        eval("(json-parse-string \"{\\\"a\\\":1,\\\"a\\\":2,\\\"b\\\":3}\" :object-type 'alist)"),
        "((a . 1) (a . 2) (b . 3))"
    );
    assert_eq!(
        eval("(json-parse-string \"{\\\"a\\\":1,\\\"b\\\":[null,false]}\" :object-type 'plist :array-type 'list)"),
        "(:a 1 :b (:null :false))"
    );
    assert_eq!(
        eval("(let ((h (json-parse-string \"{\\\"a\\\":1,\\\"a\\\":2}\"))) (list (hash-table-test h) (hash-table-count h) (gethash \"a\" h)))"),
        "(equal 1 2)"
    );
    assert_eq!(
        eval("(json-parse-string \"[null,false,true]\" :null-object nil :false-object 'no)"),
        "[nil no t]"
    );
}

/// json.c `json_parse_args`: the error datum is the offending VALUE.
#[test]
fn parse_and_serialize_argument_checks() {
    let err = |form: &str| eval(&format!("(condition-case e {form} (error e))"));
    assert_eq!(
        err("(json-parse-string \"1\" :object-type 'foo)"),
        "(error \"One of hash-table, alist or plist should be specified\" foo)"
    );
    assert_eq!(
        err("(json-parse-string \"1\" :array-type 'foo)"),
        "(error \"One of array or list should be specified\" foo)"
    );
    assert_eq!(
        err("(json-parse-string \"1\" :bogus 2)"),
        "(error \"One of :object-type, :array-type, :null-object or :false-object should be specified\" 2)"
    );
    assert_eq!(
        err("(json-parse-string \"1\" :null-object)"),
        "(wrong-type-argument plistp (:null-object))"
    );
    assert_eq!(
        err("(json-parse-string 1)"),
        "(wrong-type-argument stringp 1)"
    );
    assert_eq!(
        err("(json-serialize \"x\" :object-type 'alist)"),
        "(error \"One of :null-object or :false-object should be specified\" alist)"
    );
}

/// `json-parse-buffer` reads from point within the narrowing, leaves point
/// after the value, and on an error leaves point alone and counts from it.
#[test]
fn parse_buffer_moves_point_past_the_value() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"[1,2] rest\") (goto-char 1) (list (json-parse-buffer) (point)))"),
        "([1 2] 6)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\nb\\n  [1,\") (goto-char 5) (condition-case e (json-parse-buffer) (error (list e (point)))))"),
        "((json-end-of-file 1 nil 5) 5)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"[1] [2]\") (narrow-to-region 1 3) (goto-char 1) (condition-case e (json-parse-buffer) (error (list e (point)))))"),
        "((json-end-of-file 1 nil 2) 1)"
    );
}

#[test]
fn serialize_values() {
    assert_eq!(
        eval("(json-serialize '((a . [1 2.5 -0.0 1e20]) (b (c . t)) (a . 9) (d)))"),
        "\"{\\\"a\\\":[1,2.5,-0.0,1e+20],\\\"b\\\":{\\\"c\\\":true},\\\"d\\\":{}}\""
    );
    assert_eq!(
        eval("(json-serialize '(:a :null b :false :a 2))"),
        "\"{\\\"a\\\":null,\\\"b\\\":false}\""
    );
    assert_eq!(
        eval("(json-serialize \"q\\\"\\\\\\n\\e\\x7f\")"),
        "\"\\\"q\\\\\\\"\\\\\\\\\\\\n\\\\u001B\x7f\\\"\""
    );
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'equal))) (puthash \"k\" [] h) (json-serialize h))"),
        "\"{\\\"k\\\":[]}\""
    );
    assert_eq!(
        eval("(with-temp-buffer (json-insert '(:x 1)) (insert \"|\") (list (buffer-string) (point)))"),
        "(\"{\\\"x\\\":1}|\" 9)"
    );
}

#[test]
fn serialize_type_errors() {
    let err = |v: &str| {
        eval(&format!(
            "(condition-case e (json-serialize {v}) (error e))"
        ))
    };
    assert_eq!(err("'foo"), "(wrong-type-argument json-value-p foo)");
    assert_eq!(err("'(1 2)"), "(wrong-type-argument symbolp 1)");
    assert_eq!(err("'(1 . 2)"), "(wrong-type-argument consp 2)");
    assert_eq!(err("'(:a 1 :b)"), "(wrong-type-argument consp nil)");
    assert_eq!(err("'(:a 1 . 2)"), "(wrong-type-argument listp (:a 1 . 2))");
    assert_eq!(err("'((a . 1) b)"), "(wrong-type-argument consp b)");
    assert_eq!(err("'((\"a\" . 1))"), "(wrong-type-argument symbolp \"a\")");
    assert_eq!(
        err("(let ((h (make-hash-table))) (puthash 'k 1 h) h)"),
        "(wrong-type-argument stringp k)"
    );
    assert_eq!(
        err("1.0e+INF"),
        "(error \"JSON does not allow Inf or NaN\" 1.0e+INF)"
    );
}

/// eval.c `Fsignal` / `signal_or_quit`: a nil symbol takes `(car DATA)`, the
/// symbol must be a symbol with `error-conditions`.
#[test]
fn signal_checks_its_error_symbol() {
    let err = |form: &str| eval(&format!("(condition-case e {form} (t e))"));
    assert_eq!(
        err("(signal 'r32-nope '(1 2))"),
        "(error \"Invalid error symbol\" r32-nope)"
    );
    assert_eq!(err("(signal t nil)"), "(error \"Invalid error symbol\" t)");
    assert_eq!(
        err("(signal \"str\" nil)"),
        "(wrong-type-argument symbolp \"str\")"
    );
    assert_eq!(err("(signal nil '(error \"x\"))"), "(error \"x\")");
    assert_eq!(err("(signal nil nil)"), "(error)");
    assert_eq!(err("(signal nil 5)"), "(error . 5)");
    assert_eq!(
        err("(signal nil '(5 1))"),
        "(wrong-type-argument symbolp 5)"
    );
    assert_eq!(
        err("(progn (put 'r32-pe 'error-conditions 'notalist) (signal 'r32-pe nil))"),
        "(wrong-type-argument listp notalist)"
    );
    // The standard conditions `emacs -Q' seeds are all defined.
    assert_eq!(
        eval(
            "(list (get 'json-end-of-file 'error-conditions) \
                   (get 'cl-no-applicable-method 'error-conditions) \
                   (get 'singularity-error 'error-conditions) \
                   (get 'user-search-failed 'error-conditions))"
        ),
        "((json-end-of-file json-parse-error json-error error) \
          (cl-no-applicable-method cl-no-method error) \
          (singularity-error domain-error arith-error error) \
          (user-search-failed user-error search-failed error))"
    );
}
