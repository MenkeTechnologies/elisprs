# elisprs — known parity bugs vs Emacs Lisp

Goal: behavioral parity with real Emacs Lisp. Each entry below is a **reproduced
divergence** between `elisp -e EXPR` and `emacs -Q --batch --eval '(prin1 EXPR)'`,
checked against **GNU Emacs 30.2**.

Repro helpers:

```sh
E=./target/debug/elisp
elref() { emacs -Q --batch --eval "(prin1 $1)" 2>&1; }   # ground truth
```

---

## Core semantics — wrong values (highest severity)

### 1. No bignum support — silent integer overflow
- `(expt 2 100)` → Emacs `1267650600228229401496703205376`, elisprs `0`
- `(* 1000000000000 1000000000000)` → Emacs `1000000000000000000000000`, elisprs `2003764205206896640`
- Integer ops wrap i64 instead of promoting to bignum.

### 2. ✅ FIXED — `(lambda …)` in operator (head) position fails
- `((lambda (x) x) 5)` → Emacs `5`, elisprs `error: invalid-function`
- `((lambda (x &optional y) (list x y)) 1)` → Emacs `(1 nil)`, elisprs `error: invalid-function`
- `(funcall (lambda …) …)` works, so only *direct* application of a lambda form is
  broken. Very common idiom.

### 3. ✅ FIXED — `eq` on floats returns `t` (must be object identity → `nil`)
- `(eq 1.0 1.0)` → Emacs `nil`, elisprs `t`
- `el_eq` compares floats by bit pattern. `src/builtins.rs:163`
  (`Value::Float(x), Value::Float(y) => x.to_bits() == y.to_bits()`).
  `eql`/`equal` are correct; `eq` must not equate distinct float objects.

### 4. ✅ FIXED — `round` uses round-half-away-from-zero, not banker's rounding
- `(round 2.5)` → Emacs `2`, elisprs `3`
- `(round 0.5)` → Emacs `0`, elisprs `1`
- `(round -2.5)` → Emacs `-2`, elisprs `-3`
- Emacs rounds half to even.

### 5. ✅ FIXED — Float contagion lost in inlined `1+` / `1-`
- `(1+ 1.0)` → Emacs `2.0`, elisprs `2`
- `(1- 1.0)` → Emacs `1.0`, elisprs `0`
- The compiler inlines to integer opcodes `Op::Inc`/`Op::Dec`
  (`src/compiler.rs:170-176`), bypassing the correct `one_plus` builtin
  (`src/builtins.rs:115`). `(funcall #'1+ 1.0)` is correct.

### 6. ✅ FIXED — `mod` truncates a float operand
- `(mod 13.5 4)` → Emacs `1.5`, elisprs `1`

### 7. ✅ FIXED — `expt` mishandles float / negative exponents
- `(expt 2.0 0.5)` → Emacs `1.4142135623730951`, elisprs `2.0` (fractional exponent ignored)
- `(expt 2 -1)` → Emacs `0.5`, elisprs `1` (negative exponent should yield float)
- `(expt 0.0 0)` → Emacs `1.0`, elisprs `1` (result should be float)

### 8. ✅ FIXED — `string-to-number` can't parse floats / scientific notation / base arg
- `(string-to-number "1.5e3")` → Emacs `1500.0`, elisprs `1`
- `(string-to-number "ff" 16)` → Emacs `255`, elisprs `error: wrong-number-of-arguments`

### 9. ✅ FIXED — `split-string` ignores OMIT-NULLS
- `(split-string "a,b,,c" "," t)` → Emacs `("a" "b" "c")`, elisprs `("a" "b" "" "c")`

### 10. ✅ FIXED — `dotimes` / `dolist` ignore the RESULT form (3rd spec element)
- `(dotimes (i 3 i) i)` → Emacs `3`, elisprs `nil`
- `(let ((s nil)) (dolist (x '(1 2 3) s) (push x s)))` → Emacs `(3 2 1)`, elisprs `nil`
- Macros in `src/prelude.rs:122-134` never emit the result form `(caddr spec)`.

### 11. ✅ FIXED — `capitalize` only capitalizes the first word
- `(capitalize "hello world")` → Emacs `"Hello World"`, elisprs `"Hello world"`

---

## Reader — read syntax not supported

### 12. Vector literals `[…]` not read
- `[1 2 3]` → Emacs `[1 2 3]`, elisprs `error: Symbol's value as variable is void: [1`
- `(vector 1 2 3)` works and prints `[1 2 3]`; only the literal reader is missing.
  Cascades to `(aref [10 20 30] 1)`, `(vconcat [1 2] [3 4])`, `(equal [1 2] [1 2])`, …

### 13. Radix literals `#x` `#b` `#o` not read
- `#x1f` → Emacs `31`, elisprs `error: …void: #x1f` (same for `#b101`→5, `#o17`→15)

### 14. Char modifier syntax `?\C-` / `?\M-` not read
- `?\C-a` → Emacs `1`, elisprs `error: …void: -a`
- `?\M-a` → Emacs `134217825`, elisprs `error: …void: -a`
- Plain `?A`→65 and `?\n`→10 work; only modifier escapes fail. `src/reader.rs:156`
  (`read_char_literal`).

### 15. Float special-value read syntax not supported
- `1.0e+INF` → Emacs `1.0e+INF`, elisprs `error: …void: 1.0e+INF`

---

## `format` directives

### 16. Width / precision / flags silently ignored (returned literally)
- `(format "%5d" 42)` → Emacs `"   42"`, elisprs `"%5d"`
- `(format "%-5d|" 42)` → Emacs `"42   |"`, elisprs `"%-5d|"`
- `(format "%05d" 42)` → Emacs `"00042"`, elisprs `"%05d"`
- `(format "%.2f" 3.14159)` → Emacs `"3.14"`, elisprs `"%.2f"`
- `(format "%3.1f" 3.14159)` → Emacs `"3.1"`, elisprs `"%3.1f"`
- `(format "%+d" 5)` → Emacs `"+5"`, elisprs `"%+d"`
- `(format "% d" 5)` → Emacs `" 5"`, elisprs `"% d"`
- Also `(format "%f" 3.14159)` → Emacs `"3.141590"` (6 digits), elisprs `"3.14159"`.

### 17. Conversions `%X` `%o` `%e` `%g` unsupported (returned literally)
- `(format "%X" 255)` → Emacs `"FF"`, elisprs `"%X"`
- `(format "%o" 8)` → Emacs `"10"`, elisprs `"%o"`
- `(format "%e" 31415.9)` → Emacs `"3.141590e+04"`, elisprs `"%e"`
- `(format "%g" 100000.0)` → Emacs `"100000"`, elisprs `"%g"`

### 18. Argument field numbers `%N$` unsupported
- `(format "%2$s %1$s" "a" "b")` → Emacs `"b a"`, elisprs `"%2$s %1$s"`

---

## Float printing

### 19. ✅ FIXED — Infinity prints `inf`, should be `1.0e+INF`
- `(/ 1.0 0)` → Emacs `1.0e+INF`, elisprs `inf` (same for `(/ 1 0.0)`)

### 20. ✅ FIXED — NaN prints `NaN`, should be `0.0e+NaN`
- `(/ 0.0 0.0)` → Emacs `0.0e+NaN`, elisprs `NaN`

---

## Missing optional args / sequence coercion

### 21. ✅ FIXED — Optional second arg unsupported on several builtins
- `(floor 7 2)` → Emacs `3`, elisprs `7` (divisor arg ignored)
- `(last '(1 2 3) 2)` → Emacs `(2 3)`, elisprs `error: wrong-number-of-arguments`
- `(butlast '(1 2 3) 2)` → Emacs `(1)`, elisprs `error: wrong-number-of-arguments`

### 22. ✅ FIXED — Sequence coercion missing (strings/chars as sequences)
- `(reverse "abc")` → Emacs `"cba"`, elisprs `error: wrong-type-argument: listp`
- `(append "ab" nil)` → Emacs `(97 98)`, elisprs `error: wrong-type-argument: listp`
- `(append '(1 2) '(3 4) 5)` → Emacs `(1 2 3 4 . 5)` (dotted tail), elisprs `error: wrong-type-argument: listp`
- `(downcase ?A)` → Emacs `97`, elisprs `error: wrong-type-argument: stringp 65` (also `(upcase ?a)`)

### 23. 🟡 PARTIAL — Core functions present in `emacs -Q` but void in elisprs
Added: `type-of`, `functionp`, `char-or-string-p`, `sqrt`, `fround`, `ffloor`,
`fceiling`, `ftruncate`, `isnan`, `char-equal` (`prin1-to-string` already present).
Still missing: `read`, `logb`, `compare-strings`, `error-message-string`,
`format-message`, `seq-mapn`.

---

## Coverage — verified at parity (no bug)

Integer `/` truncation toward zero, `%`/`mod` integer sign rules, float contagion in
`+ - * = < min max`, `(/ 1 0)` arith-error, `ash`/`logand`/`logior`/`logxor`/`lognot`,
`?A`/`?\n`, dotted-pair printing, `nthcdr`/`nreverse`/`assoc`/`assq`/`alist-get`/
`member`/`memq`/`setcar`, `mapcar`/`mapconcat`/`sort`, `elt`/`aref`(via `vector`)/
`concat`/`substring` (incl. negative indices), `string-match`/`replace-regexp-in-string`,
basic `format` (`%d %s %S %c %%`), plist-get/put, `eql`/`equal`, `cond`/`and`/`or`/
`when`/`unless`/`while`/`catch-throw`/`condition-case` (incl. `arith-error`,
`wrong-type-argument`), `unwind-protect`, `let`/`let*`, lexical closures,
`funcall`/`apply`, `&optional`/`&rest` via funcall, hash tables, `car`/`cdr` of nil,
`(nth 99 …)`→nil, `number-sequence`, string utils, `intern`/`eq` on symbols, keywords,
backquote/unquote/splice.

---

# Round 2 — additional confirmed divergences (vs `emacs -Q` 30.2)

Found in a deeper second pass; all reproduced against the current binary. **Ground
truth is bare `emacs -Q --batch`** — `cl-lib` macros that are `void-function` there
(`cl-loop`, `cl-flet`, `cl-labels`, `cl-typecase`, `cl-destructuring-bind`,
`cl-reduce`/`cl-find`/`cl-position`/`cl-mapcar`/`cl-getf` with keywords,
`cl-remove-duplicates`) are **not** listed: Emacs errors too, so they aren't `-Q`
parity bugs (they'd need `(require 'cl-lib)`).

## Critical — wrong values / silent miscomputation

### R2-A. Arithmetic silently coerces non-numbers instead of signaling
- `(+ 1 "a")` → Emacs signals `(wrong-type-argument number-or-marker-p "a")`, elisprs `1.0`
- `(* 2 "x")` → Emacs signals, elisprs `0.0`; `(+ 1 'sym)` → elisprs `1.0`
- Most dangerous: arithmetic on bad data never errors and returns silent wrong numbers.

### R2-B. `wrong-type-argument` error data is one string, not separate elements
- `(condition-case e (car 5) (error e))` → Emacs `(wrong-type-argument listp 5)`,
  elisprs `(wrong-type-argument "listp 5")`
- Predicate+value collapsed into a single string; breaks handlers reading `(cadr e)`/`(caddr e)`.

### R2-C. `user-error` signals the `error` symbol, not `user-error`
- `(condition-case e (user-error "nope") (error e))` → Emacs `(user-error "nope")`,
  elisprs `(error "nope")` — the two conditions can't be distinguished.

### R2-D. Float printer doesn't use exponent form for large / small magnitudes
- `(prin1-to-string 1e20)` → Emacs `"1e+20"`, elisprs `"100000000000000000000.0"`
- `1e15`→`"1e+15"` vs `"1000000000000000.0"`; `1.5e-10`→`"1.5e-10"` vs `"0.00000000015"`
- Affects `prin1`, `number-to-string`, `format "%s"`. (Distinct from the inf/NaN entries.)

## Macros / special forms

### R2-E. `cl-incf` / `cl-decf` only accept a bare symbol, not a generalized place
- `(let ((l (list 1 2))) (cl-incf (car l)) l)` → Emacs `(2 2)`, elisprs `error: setq: expected a symbol`
- `setf` itself works on places, so the cl-incf/decf macros just don't expand through it.

### R2-F. `setq-default` is broken
- `(setq-default x 5)` → Emacs `5`, elisprs `error: Symbol's value as variable is void: x`

### R2-G. `pcase` backquote patterns unsupported
- `` (pcase (list 1 2) (`(,a ,b) (+ a b))) `` → Emacs `3`, elisprs `error: pcase: unsupported pattern (cons a (cons b nil))`
- Plain reader backquote works; only pcase destructuring on it fails. (`pcase-let` backquote too.)

## Sequence / string semantics

### R2-H. `mapcar` (and `seq-map`) reject vector/string sequences
- `(mapcar #'1+ [1 2 3])` → Emacs `(2 3 4)`, elisprs `error: mapcar: not a list`
- `(mapcar #'1+ "abc")` → Emacs `(98 99 100)`, elisprs errors. (Broader than #22.)

### R2-I. `seq-empty-p` wrong on the empty string
- `(seq-empty-p "")` → Emacs `t`, elisprs `nil`

### R2-J. `string-blank-p` returns `t` instead of the match position
- `(string-blank-p "  ")` → Emacs `0`, elisprs `t`

### R2-K. `string-pad` 4-arg form (PADDING + START) unsupported
- `(string-pad "ab" 5 ?* t)` → Emacs `"***ab"`, elisprs `error: wrong-number-of-arguments`

### R2-L. `make-hash-table` print format diverges from Emacs 30
- `(make-hash-table)` → Emacs `#s(hash-table)`, elisprs `#s(hash-table size 0)`

### R2-M. `assoc` TESTFN (3rd arg) unsupported
- `(assoc 2 '((1 . 10) (2 . 20)) #'=)` → Emacs `(2 . 20)`, elisprs `error: wrong-number-of-arguments`

### R2-N. Emacs-30 `sort` keyword API (`:key`/`:lessp`) unsupported
- `(sort '(3 1 2) :key #'- :lessp #'<)` → Emacs `(3 2 1)`, elisprs `error: void-function: :key`

## Missing builtins / constants (present in bare `emacs -Q`, void in elisprs)

All confirmed `void-function`/void-variable in elisprs while `emacs -Q` returns a value:

- **Symbols/eval:** `boundp` (`t`), `fboundp` (`t`), `gensym` (`g0`), `macrop` (`t`),
  `special-variable-p` (`nil`), `func-arity` (`(1 . 1)`), `indirect-function`,
  `featurep`/`provide`/`require`, `named-let` (`6`)
- **Constants:** `most-positive-fixnum` (`2305843009213693951`), `most-negative-fixnum`
- **Sequences:** `vconcat` (`[1 2 3 4]`), `copy-alist`, `length=`/`length<`/`length>` (`t`),
  `string-to-vector` (`[97 98 99]`), `seq-mapn` (`(4 6)`)
- **Strings/props:** `propertize` (`#("hi" 0 2 (face bold))`) + text properties,
  `string-width` (`3`), `string-distance` (`3`), `string-equal-ignore-case` (`t`)
- **Math:** `frexp` (`(0.5 . 4)`), `ldexp` (`8.0`), `copysign` (`-3.0`)

Areas probed in round 2 that PASSED (now match Emacs): `floor`/`ceiling`/`truncate`/
`round` with a divisor arg (the #21 follow-ups — fixed), `seq-reverse`,
`string-search`/`string-replace`, `cl-case`, `when-let`/`if-let`, `pcase`
`pred`/`or`/`and`/`guard`, the autoloaded `seq-` family on lists, `type-of`/`functionp`,
`prin1-to-string` (the #23 entry — now present).
