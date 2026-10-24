;;; bidi-data.el --- print src/bidi_data.rs from the oracle  -*- lexical-binding: t -*-

;; emacs -Q --batch -l scripts/gen/bidi-data.el > src/bidi_data.rs
;;
;; Scans `bidi-class' (`get-char-code-property') over every character and
;; prints the runs of one class as a Rust table.

(let* ((classes '(L R AL EN ES ET AN CS NSM BN B S WS ON
                  LRE LRO RLE RLO PDF LRI RLI FSI PDI))
       (runs nil) (prev nil) (start 0))
  (dotimes (c (1+ (max-char)))
    (let ((cls (get-char-code-property c 'bidi-class)))
      (unless (eq cls prev)
        (when (> c 0) (push (list start (1- c) prev) runs))
        (setq prev cls start c))))
  (push (list start (max-char) prev) runs)
  (princ "//! The `bidi-class` of every character (Emacs 31.1 `get-char-code-property`
//! over 0..=#x3FFFFF), as runs `(FIRST, LAST, CLASS)`, generated from the
//! oracle by scripts/gen/bidi-data.el. CLASS indexes [`CLASSES`].

/// The class symbols, as `bidi-class` names them.
pub const CLASSES: [&str; 23] = [
    \"L\", \"R\", \"AL\", \"EN\", \"ES\", \"ET\", \"AN\", \"CS\", \"NSM\", \"BN\", \"B\", \"S\", \"WS\", \"ON\",
    \"LRE\", \"LRO\", \"RLE\", \"RLO\", \"PDF\", \"LRI\", \"RLI\", \"FSI\", \"PDI\",
];

/// Runs of one class, ascending and covering every character.
pub const RUNS: &[(u32, u32, u8)] = &[
")
  (dolist (r (nreverse runs))
    (princ (format "    (0x%X, 0x%X, %d),\n" (nth 0 r) (nth 1 r)
                   (seq-position classes (nth 2 r)))))
  (princ "];\n"))
