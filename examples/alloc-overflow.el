;;; alloc-overflow.el --- allocator length checks vs GNU Emacs  -*- lexical-binding: nil; -*-

;; A faithful interpreter must never panic/abort the process on valid elisp.
;; The fixed-size allocators (`make-vector', `make-string', `make-record',
;; `make-list') used to panic ("capacity overflow") or abort ("memory
;; allocation of N bytes failed") on out-of-range lengths. Emacs instead:
;;   * length > most-positive-fixnum (a bignum) -> `wrong-type-argument wholenump'
;;   * negative length                          -> `wrong-type-argument wholenump'
;;   * in-fixnum length too big to allocate     -> plain `error' (memory exhausted)
;;   * make-record over 4095 slots              -> plain `error' (slot cap)
;; Each `should` below is oracle-verified against `emacs -Q --batch` 30.2.
(message "== alloc-overflow demo ==")

(defconst alloc-bignum (1+ most-positive-fixnum)
  "One past `most-positive-fixnum'; a bignum in Emacs, rejected as non-wholenum.")

(ert-deftest alloc-bignum-length-is-wholenump ()
  "A length above `most-positive-fixnum' signals wrong-type-argument, not a panic."
  (should (equal (condition-case e (make-vector alloc-bignum 0) (error e))
                 (list 'wrong-type-argument 'wholenump alloc-bignum)))
  (should (equal (condition-case e (make-string alloc-bignum ?a) (error e))
                 (list 'wrong-type-argument 'wholenump alloc-bignum)))
  (should (equal (condition-case e (make-record 'x alloc-bignum nil) (error e))
                 (list 'wrong-type-argument 'wholenump alloc-bignum)))
  (should (equal (condition-case e (make-list alloc-bignum 0) (error e))
                 (list 'wrong-type-argument 'wholenump alloc-bignum))))

(ert-deftest alloc-negative-length-is-wholenump ()
  "A negative length signals wrong-type-argument wholenump (make-list too)."
  (should (equal (condition-case e (make-vector -1 0) (error e))
                 '(wrong-type-argument wholenump -1)))
  (should (equal (condition-case e (make-string -1 ?a) (error e))
                 '(wrong-type-argument wholenump -1)))
  (should (equal (condition-case e (make-list -5 'a) (error e))
                 '(wrong-type-argument wholenump -5))))

(ert-deftest alloc-too-large-is-memory-exhausted ()
  "An in-fixnum length too big to allocate signals a plain `error', not abort."
  (should (equal (condition-case e (make-vector most-positive-fixnum 0) (error e))
                 '(error "Memory exhausted--use C-x s then exit and restart Emacs")))
  (should (equal (condition-case e (make-string most-positive-fixnum ?a) (error e))
                 '(error "Memory exhausted--use C-x s then exit and restart Emacs"))))

(ert-deftest alloc-record-slot-cap ()
  "make-record over PSEUDOVECTOR_SIZE_MASK (4095) slots signals the slot-cap error."
  (should (equal (condition-case e (make-record 'x most-positive-fixnum nil) (error e))
                 (list 'error (format "Attempt to allocate a record of %d slots; max is %d"
                                      (1+ most-positive-fixnum) 4095)))))

(ert-deftest alloc-normal-lengths-still-work ()
  "Ordinary in-range allocations are unaffected by the new range checks."
  (should (equal (make-vector 3 7) [7 7 7]))
  (should (equal (make-string 3 ?a) "aaa"))
  (should (equal (make-list 3 'x) '(x x x)))
  (should (recordp (make-record 'pt 2 nil)))
  (should (equal (make-vector 0 0) [])))

(ert-run-tests-batch-and-exit)
