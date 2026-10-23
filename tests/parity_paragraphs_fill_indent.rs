//! paragraphs.el, fill.el, indent.el and newcomment.el are loaded verbatim
//! (they are preloaded in Emacs); text-mode.el's centering commands with them.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

/// Evaluate SRC in a fresh host; a signalled error comes back as `(ERR ERROR)`.
fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

/// `forward-paragraph`/`backward-paragraph` stop at `paragraph-start` and
/// `paragraph-separate` lines; `mark-paragraph` and `kill-paragraph` use them.
#[test]
fn paragraph_motion() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"p1 l1\\np1 l2\\n\\n\\np2 l1\\n\\np3\\n\") (goto-char 1) (list (progn (forward-paragraph) (point)) (progn (forward-paragraph) (point)) (progn (forward-paragraph) (point)) (progn (forward-paragraph) (point))))"),
        "(13 21 25 25)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"p1 l1\\np1 l2\\n\\n\\np2 l1\\n\\np3\\n\") (goto-char (point-max)) (list (progn (backward-paragraph) (point)) (progn (backward-paragraph) (point)) (progn (backward-paragraph) (point)) (progn (backward-paragraph) (point))))"),
        "(21 14 1 1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"p1 l1\\np1 l2\\n\\np2 l1\\n\") (goto-char 3) (mark-paragraph) (list (point) (mark)))"),
        "(1 13)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"p1 l1\\np1 l2\\n\\np2 l1\\n\") (goto-char 3) (setq kill-ring nil) (kill-paragraph 1) (list (buffer-string) (car kill-ring)))"),
        "(\"p1\np2 l1\n\" \" l1\np1 l2\n\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\n\\nb\\n\\nc\") (goto-char 1) (transpose-paragraphs 1) (buffer-string))"),
        "\"\nb\na\n\nc\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\\n\\np1\\np1b\\n\\np2\") (goto-char 6) (list (progn (start-of-paragraph-text) (point)) (progn (end-of-paragraph-text) (point))))"),
        "(4 10)"
    );
}

/// Sentences end at `sentence-end`, which honours `sentence-end-double-space`.
#[test]
fn sentence_motion() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"One.  Two three.  Four five six.\") (goto-char 1) (list (progn (forward-sentence) (point)) (progn (forward-sentence) (point)) (progn (backward-sentence) (point)) (progn (forward-sentence 2) (point))))"),
        "(5 17 7 33)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"One. Two three. Four.\") (goto-char 1) (let ((sentence-end-double-space nil)) (list (progn (forward-sentence) (point)) (progn (forward-sentence) (point)))))"),
        "(5 16)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"One. Two.  Three.  Four\") (count-sentences 1 (point-max)))"),
        "3"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"One.  Two.  Three.\") (goto-char 1) (kill-sentence) (buffer-string))"),
        "\"  Two.  Three.\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"One.  Two.  Three.\") (goto-char 1) (transpose-sentences 1) (buffer-string))"),
        "\"Two.  One.  Three.\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"One.  Two.  Three.\") (goto-char (point-max)) (setq kill-ring nil) (backward-kill-sentence) (list (buffer-string) (car kill-ring)))"),
        "(\"One.  Two.  \" \"Three.\")"
    );
}

/// Filling breaks at `fill-column`, keeps the double space after a sentence
/// together, and carries comment and indentation prefixes.
#[test]
fn fill_region_and_paragraph() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm nnn ooo ppp qqq rrr sss ttt uuu vvv www xxx\") (let ((fill-column 20)) (fill-region 1 (point-max))) (buffer-string))"),
        "\"aaa bbb ccc ddd eee\nfff ggg hhh iii jjj\nkkk lll mmm nnn ooo\nppp qqq rrr sss ttt\nuuu vvv www xxx\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm nnn ooo. Ppp qqq rrr\\nsss ttt\\n\\nuuu vvv www xxx\") (let ((fill-column 15)) (goto-char 3) (fill-paragraph)) (buffer-string))"),
        "\"aaa bbb ccc ddd\neee fff ggg hhh\niii jjj kkk lll\nmmm nnn\nooo. Ppp qqq\nrrr sss ttt\n\nuuu vvv www xxx\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \";; aaa bbb ccc ddd eee fff ggg hhh iii\\n;; jjj kkk\") (let ((fill-column 15)) (goto-char 5) (fill-paragraph)) (buffer-string))"),
        "\";; aaa bbb ccc\n;; ddd eee fff\n;; ggg hhh iii\n;; jjj kkk\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  aaa bbb ccc ddd eee fff ggg hhh iii\\n  jjj kkk\") (let ((fill-column 15)) (goto-char 5) (fill-paragraph)) (buffer-string))"),
        "\"  aaa bbb ccc\n  ddd eee fff\n  ggg hhh iii\n  jjj kkk\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  > aaa bbb ccc ddd eee fff ggg hhh\\n  > iii\") (let ((fill-column 15)) (fill-paragraph)) (buffer-string))"),
        "\"  > aaa bbb ccc\n  > ddd eee fff\n  > ggg hhh iii\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa bbb ccc ddd eee fff ggg hhh iii\") (let ((fill-column 12) (fill-prefix \"-- \")) (fill-region 1 (point-max))) (buffer-string))"),
        "\"aaa bbb ccc\n-- ddd eee\n-- fff ggg\n-- hhh iii\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa bbb\\nccc ddd\\neee\") (let ((fill-column 70)) (fill-region 1 (point-max))) (buffer-string))"),
        "\"aaa bbb ccc ddd eee\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"Hello world.  Bye\") (let ((fill-column 8)) (fill-region 1 (point-max))) (buffer-string))"),
        "\"Hello\nworld.\nBye\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  first line here and more words\\n  second\") (let ((fill-column 12)) (fill-individual-paragraphs 1 (point-max))) (buffer-string))"),
        "\"  first line\n  here and\n  more words\n  second\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  > a b\\n  > c d\\n\") (fill-context-prefix 1 (point-max)))"),
        "\"  > \""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaaa bbbb cccc dddd\") (let ((fill-column 10)) (goto-char (point-max)) (do-auto-fill)) (buffer-string))"),
        "\"aaaa bbbb\ncccc dddd\""
    );
}

/// `fill-region`'s JUSTIFY argument, `justify-current-line` and the
/// text-mode.el centering commands.
#[test]
fn justification() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa  bbb   ccc\") (let ((fill-column 30)) (fill-region 1 (point-max) 'full)) (buffer-string))"),
        "\"aaa bbb ccc\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa bbb ccc ddd eee fff\") (let ((fill-column 10)) (fill-region 1 (point-max) 'right)) (buffer-string))"),
        "\"   aaa bbb\n   ccc ddd\n   eee fff\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa bbb ccc ddd eee fff ggg\") (let ((fill-column 12)) (fill-region 1 (point-max) 'full)) (buffer-string))"),
        "\"aaa  bbb ccc\nddd  eee fff\nggg\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc def\") (let ((fill-column 20)) (fill-region 1 (point-max) 'center)) (buffer-string))"),
        "\"      abc def\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar   baz\") (goto-char 1) (justify-current-line 'full) (buffer-string))"),
        "\"foo                              bar                             baz\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar\") (let ((fill-column 20)) (goto-char 1) (center-line)) (buffer-string))"),
        "\"      foo bar\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"     foo   bar   \") (goto-char 1) (unjustify-current-line) (buffer-string))"),
        "\"     foo   bar   \""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\") (list (current-fill-column) (current-justification) (fill-nobreak-p)))"),
        "(70 left nil)"
    );
}

/// indent.el: rigid indentation, `indent-region` with a column, relative
/// indentation, tab stops and the left margin.
#[test]
fn indentation_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\nb\\nc\") (indent-rigidly 1 (point-max) 2) (buffer-string))"),
        "\"  a\n  b\n  c\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  a\\n    b\\nc\") (indent-rigidly 1 (point-max) -2) (buffer-string))"),
        "\"a\n  b\nc\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\n b\\n  c\") (let ((indent-tabs-mode nil)) (indent-region 1 (point-max) 4)) (buffer-string))"),
        "\"    a\n    b\n    c\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\\n y\\n  z\") (let ((indent-tabs-mode nil)) (indent-region 1 (point-max) 0)) (buffer-string))"),
        "\"x\ny\nz\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  foo bar\\nx\") (goto-char (point-max)) (indent-relative) (buffer-string))"),
        "\"  foo bar\nx \""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\n  b\") (let ((indent-line-function 'indent-relative) (indent-tabs-mode nil)) (goto-char (point-max)) (indent-according-to-mode)) (buffer-string))"),
        "\"a\nb\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\") (let ((indent-tabs-mode nil)) (tab-to-tab-stop)) (list (buffer-string) (current-column)))"),
        "(\"x       \" 8)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"ab\") (let ((indent-tabs-mode nil) (tab-stop-list '(3 7))) (tab-to-tab-stop) (tab-to-tab-stop) (list (buffer-string) (indent-next-tab-stop 7) (indent-next-tab-stop 7 t))))"),
        "(\"ab     \" 11 3)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\tb\") (list (move-to-tab-stop) (point)))"),
        "(nil 5)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  foo\") (beginning-of-line-text) (point))"),
        "3"
    );
    assert_eq!(
        eval("(with-temp-buffer (list (current-left-margin) (progn (insert \"  x\") (move-to-left-margin) (point))))"),
        "(0 1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a b\") (let ((left-margin 4)) (indent-to-left-margin) (list (buffer-string) (current-left-margin))))"),
        "(\"    a b\" 4)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"   x\") (goto-char (point-max)) (back-to-indentation) (indent-line-to 6) (list (buffer-string) (point)))"),
        "(\"      x\" 7)"
    );
}

/// newcomment.el over a `comment-start` set by the caller.
#[test]
fn comment_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\\ndef\") (let ((comment-start \"# \")) (comment-region 1 (point-max))) (buffer-string))"),
        "\"# abc\n# def\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"# abc\\n# def\") (let ((comment-start \"# \")) (uncomment-region 1 (point-max))) (buffer-string))"),
        "\"abc\ndef\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo\") (let ((comment-start \";; \") (comment-column 10)) (comment-indent)) (buffer-string))"),
        "\"foo\t  ;; \""
    );
    assert_eq!(
        eval("(with-temp-buffer (let ((comment-start \"/* \") (comment-end \" */\")) (insert \"x\") (comment-region 1 2)) (buffer-string))"),
        "\"/* x */\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo\") (let ((comment-start \"# \")) (comment-line 1)) (buffer-string))"),
        "\"# foo\""
    );
}
