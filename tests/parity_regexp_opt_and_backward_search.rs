//! `regexp-opt`'s SHAPE, `rx`'s `or`, and which way a backward search searches.
//!
//! Three findings from one thread: `looking-back` was void, writing it exposed
//! `re-search-backward`, and `rx`'s `or` turned out to depend on `regexp-opt`,
//! which was building the right language in the wrong shape.
//!
//! ```text
//!                                     emacs 31.1              elisprs (before)
//! (regexp-opt '("a" "b" "c"))         "[abc]"                 "\(?:a\|b\|c\)"
//! (regexp-opt '("cat" "cot" "cut"))   "\(?:c\(?:[aou]t\)\)"   "\(?:cat\|cot\|cut\)"
//! (rx (or "a" (: "b" "c")))           "a\|bc"                 "\(?:a\|bc\)"
//! (re-search-backward "a+") in "aaa" from 4  -> 3             -> 1
//! ```
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1
//! (with `regexp-opt` loaded, which is what defines those two functions there).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `regexp-opt` factors a common prefix, a common suffix, and an empty string;
/// what is left splits on the first character.
#[test]
fn regexp_opt_factors_shared_affixes() {
    assert_eq!(
        eval("(regexp-opt '(\"ab\" \"ac\"))"),
        "\"\\\\(?:a[bc]\\\\)\""
    );
    assert_eq!(
        eval("(regexp-opt '(\"cat\" \"cot\" \"cut\"))"),
        "\"\\\\(?:c\\\\(?:[aou]t\\\\)\\\\)\""
    );
    assert_eq!(
        eval("(regexp-opt '(\"foo\" \"foobar\"))"),
        "\"\\\\(?:foo\\\\(?:bar\\\\)?\\\\)\""
    );
    // A common SUFFIX: "ad" and "d" share the trailing d.
    assert_eq!(eval("(regexp-opt '(\"ad\" \"d\"))"), "\"\\\\(?:a?d\\\\)\"");
    assert_eq!(
        eval("(regexp-opt '(\"abc\" \"abd\" \"xyz\"))"),
        "\"\\\\(?:ab[cd]\\\\|xyz\\\\)\""
    );
    assert_eq!(eval("(regexp-opt '(\"\" \"a\"))"), "\"a?\"");
    // Nothing shared, nothing to fold.
    assert_eq!(
        eval("(regexp-opt '(\"foo\" \"bar\"))"),
        "\"\\\\(?:bar\\\\|foo\\\\)\""
    );
    // The strings are literal: metacharacters are quoted before factoring.
    assert_eq!(
        eval("(regexp-opt '(\"a.b\" \"a*b\"))"),
        "\"\\\\(?:a\\\\(?:[*.]b\\\\)\\\\)\""
    );
}

/// The PAREN argument, and the shapes that need no group at all.
#[test]
fn regexp_opt_paren_forms() {
    assert_eq!(eval("(regexp-opt '())"), "\"\\\\(?:\\\\`a\\\\`\\\\)\"");
    assert_eq!(eval("(regexp-opt '(\"a\"))"), "\"a\"");
    assert_eq!(eval("(regexp-opt '(\"ab\"))"), "\"\\\\(?:ab\\\\)\"");
    assert_eq!(eval("(regexp-opt '(\"ab\") t)"), "\"\\\\(ab\\\\)\"");
    assert_eq!(
        eval("(regexp-opt '(\"foo\" \"bar\") 'words)"),
        "\"\\\\<\\\\(bar\\\\|foo\\\\)\\\\>\""
    );
    assert_eq!(
        eval("(regexp-opt '(\"foo\" \"bar\") 'symbols)"),
        "\"\\\\_<\\\\(bar\\\\|foo\\\\)\\\\_>\""
    );
    assert_eq!(
        eval("(regexp-opt '(\"foo\" \"bar\") \"\\\\(?1:\")"),
        "\"\\\\(?1:bar\\\\|foo\\\\)\""
    );
    // An explicit group counts toward `regexp-opt-depth`; a shy one does not.
    assert_eq!(
        eval("(regexp-opt-depth (regexp-opt '(\"a\" \"b\") t))"),
        "1"
    );
    assert_eq!(eval("(regexp-opt-depth (regexp-opt '(\"a\" \"b\")))"), "0");
}

/// `regexp-opt-charset` collapses a run into a range only ABOVE three
/// characters, and places `]`, `^` and `-` where a bracket expression allows
/// them.
#[test]
fn regexp_opt_charset_ranges_and_metacharacters() {
    assert_eq!(eval("(regexp-opt-charset '(?a ?b ?c))"), "\"[abc]\"");
    assert_eq!(eval("(regexp-opt-charset '(?a ?b ?c ?d))"), "\"[a-d]\"");
    assert_eq!(eval("(regexp-opt-charset '(?a ?c))"), "\"[ac]\"");
    assert_eq!(eval("(regexp-opt-charset '(?z ?a ?m))"), "\"[amz]\"");
    // One character needs no brackets; none is the never-matching regexp.
    assert_eq!(eval("(regexp-opt-charset '(?a))"), "\"a\"");
    assert_eq!(eval("(regexp-opt-charset '())"), "\"\\\\`a\\\\`\"");
    assert_eq!(eval("(regexp-opt-charset '(?\\] ?a))"), "\"[]a]\"");
    assert_eq!(eval("(regexp-opt-charset '(?^ ?a))"), "\"[a^]\"");
    assert_eq!(eval("(regexp-opt-charset '(?- ?a))"), "\"[a-]\"");
    assert_eq!(eval("(regexp-opt-charset '(?^ ?-))"), "\"[-^]\"");
}

/// `rx`'s `or` uses `regexp-opt` for literal branches, which is deliberately
/// NOT the `any` rendering — it sorts but does not condense a run into a range.
#[test]
fn rx_or_uses_regexp_opt_not_the_charset_renderer() {
    assert_eq!(eval("(rx (or \"a\" \"b\" \"c\"))"), "\"[abc]\"");
    assert_eq!(eval("(rx (in \"abc\"))"), "\"[a-c]\"");
    assert_eq!(eval("(rx (or ?b ?a))"), "\"[ab]\"");
    assert_eq!(
        eval("(rx (or \"cat\" \"cot\"))"),
        "\"\\\\(?:c\\\\(?:[ao]t\\\\)\\\\)\""
    );
    assert_eq!(eval("(rx (or \"ab\" \"cd\"))"), "\"\\\\(?:ab\\\\|cd\\\\)\"");
}

/// Nested `or`s flatten, and branches that all denote character SETS merge into
/// one set.
#[test]
fn rx_or_flattens_and_merges_character_sets() {
    assert_eq!(eval("(rx (or (or \"a\" \"b\") \"c\"))"), "\"[abc]\"");
    assert_eq!(
        eval("(rx (or (any \"a-z\") (any \"0-9\")))"),
        "\"[0-9a-z]\""
    );
    assert_eq!(eval("(rx (or (any \"a-z\") (in ?_)))"), "\"[_a-z]\"");
    assert_eq!(eval("(rx (or \"a\" (any \"0-9\")))"), "\"[0-9a]\"");
    assert_eq!(eval("(rx (or (any \"a-z\") alpha))"), "\"[a-z[:alpha:]]\"");
    // A branch that is not a set stops the merge; the alternation stands.
    assert_eq!(eval("(rx (or \"foo\" (any \"0-9\")))"), "\"foo\\\\|[0-9]\"");
    assert_eq!(eval("(rx (or (any \"a-z\") \"foo\"))"), "\"[a-z]\\\\|foo\"");
}

/// Alternation binds loosest, so it is BARE when alone and parenthesized only
/// when it has siblings or a quantifier.
#[test]
fn an_alternation_is_grouped_only_where_it_must_be() {
    assert_eq!(eval("(rx (or \"a\" (: \"b\" \"c\")))"), "\"a\\\\|bc\"");
    assert_eq!(
        eval("(rx (seq (or \"a\" (: \"b\" \"c\"))))"),
        "\"a\\\\|bc\""
    );
    assert_eq!(
        eval("(rx (seq (seq (or \"a\" (: \"b\" \"c\")))))"),
        "\"a\\\\|bc\""
    );
    assert_eq!(
        eval("(rx (group (or \"a\" (: \"b\" \"c\"))))"),
        "\"\\\\(a\\\\|bc\\\\)\""
    );
    assert_eq!(
        eval("(rx-to-string '(or \"a\" (: \"b\" \"c\")) t)"),
        "\"a\\\\|bc\""
    );
    // …and grouped where it has a neighbour or an operator.
    assert_eq!(
        eval("(rx (seq (or \"a\" (: \"b\" \"c\")) \"x\"))"),
        "\"\\\\(?:a\\\\|bc\\\\)x\""
    );
    assert_eq!(
        eval("(rx \"z\" (or \"a\" (: \"b\" \"c\")))"),
        "\"z\\\\(?:a\\\\|bc\\\\)\""
    );
    assert_eq!(
        eval("(rx (one-or-more (or \"a\" (: \"b\" \"c\"))))"),
        "\"\\\\(?:a\\\\|bc\\\\)+\""
    );
    assert_eq!(
        eval("(rx-to-string '(or \"a\" (: \"b\" \"c\")))"),
        "\"\\\\(?:a\\\\|bc\\\\)\""
    );
}

/// A backward search tries START positions from point DOWNWARDS and takes the
/// first that matches — it is not "the last forward match before point" — and
/// it bounds the match END at the position the search started from.
#[test]
fn re_search_backward_scans_start_positions_downwards() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 4) \
              (list (re-search-backward \"a+\" nil t) (match-beginning 0) (match-end 0)))"
        ),
        "(3 3 4)"
    );
    // The end bound: at start 2 an unbounded `a+` would reach 4, past where the
    // search began.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 3) \
              (list (re-search-backward \"a+\" nil t) (match-beginning 0) (match-end 0)))"
        ),
        "(2 2 3)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"xaaa\") (goto-char 5) \
              (list (re-search-backward \"a+\" nil t) (match-beginning 0) (match-end 0)))"
        ),
        "(4 4 5)"
    );
    // BOUND: the match may not begin before it.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 4) \
              (list (re-search-backward \"a+\" 2 t) (match-beginning 0)))"
        ),
        "(3 3)"
    );
    // No match: nil with NOERROR, `search-failed` without.
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa\") (goto-char 4) (re-search-backward \"z\" nil t))"),
        "nil"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 4) \
              (condition-case e (re-search-backward \"z\") (error (car e))))"
        ),
        "search-failed"
    );
    // A literal backward search already agreed, and still does.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcabc\") (goto-char 7) \
              (list (search-backward \"abc\" nil t) (point)))"
        ),
        "(4 4)"
    );
}

/// `looking-back` is `looking-at` for the text before point. GREEDY extends the
/// match backwards as far as it can, which is visible in the match data rather
/// than in the answer.
#[test]
fn looking_back_matches_the_text_before_point() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 2) (looking-back \"a\" 1))"),
        "t"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 2) (looking-back \"z\" 1))"),
        "nil"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 4) (looking-back \"abc\"))"),
        "t"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 1) (looking-back \"a\"))"),
        "nil"
    );
    // Without GREEDY the match is the shortest one ending at point…
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 4) \
              (progn (looking-back \"a+\" 1) (match-beginning 0)))"
        ),
        "3"
    );
    // …and with it, the longest.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 4) \
              (progn (looking-back \"a+\" 1 t) (match-beginning 0)))"
        ),
        "1"
    );
}

/// COUNT is not a decoration: it selects the Nth match, 0 searches not at all,
/// and a NEGATIVE count searches the other way. All four commands ignored it.
#[test]
fn the_search_commands_take_a_count() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcabc\") (goto-char 1) \
              (list (search-forward \"abc\" nil t 2) (point)))"
        ),
        "(7 7)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 1) \
              (list (search-forward \"a\" nil t 3) (point)))"
        ),
        "(4 4)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 4) \
              (list (search-backward \"a\" nil t 2) (point)))"
        ),
        "(2 2)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aXbXcX\") (goto-char 1) \
              (list (re-search-forward \"X\" nil t 3) (point) (match-beginning 0)))"
        ),
        "(7 7 6)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 4) \
              (list (re-search-backward \"a\" nil t 2) (point)))"
        ),
        "(2 2)"
    );
    // The match data is the Nth match's, not the first's.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aXbXcX\") (goto-char 1) \
              (list (re-search-forward \"X\" nil t 2) (match-beginning 0) (match-end 0)))"
        ),
        "(5 4 5)"
    );
    // Zero searches not at all; negative searches the other way.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 1) \
              (list (search-forward \"a\" nil t 0) (point)))"
        ),
        "(1 1)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 2) \
              (list (search-forward \"a\" nil t -1) (point)))"
        ),
        "(1 1)"
    );
}

/// Failure has THREE cases, and COUNT is what makes the difference between the
/// first two visible: a partial run has already moved point.
#[test]
fn a_failed_search_restores_or_moves_point_by_noerror() {
    // NOERROR nil: signal.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 1) \
              (condition-case e (search-forward \"a\" nil nil 5) (error (car e))))"
        ),
        "search-failed"
    );
    // NOERROR t: nil, and point is back where it started — three of the five
    // repetitions succeeded first.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 1) \
              (list (search-forward \"a\" nil t 5) (point)))"
        ),
        "(nil 1)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 1) \
              (list (re-search-forward \"a\" nil t 5) (point)))"
        ),
        "(nil 1)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 2) \
              (list (search-forward \"a\" nil t -5) (point)))"
        ),
        "(nil 2)"
    );
    // Any other non-nil NOERROR: nil, and point moves to the limit — which end
    // depends on the direction actually searched.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 1) \
              (list (search-forward \"a\" nil 'move 5) (point)))"
        ),
        "(nil 4)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 4) \
              (list (search-backward \"a\" nil 'move 5) (point)))"
        ),
        "(nil 1)"
    );
    // …and an explicit BOUND is that limit.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaa\") (goto-char 1) \
              (list (search-forward \"a\" 2 'move 5) (point)))"
        ),
        "(nil 2)"
    );
}
