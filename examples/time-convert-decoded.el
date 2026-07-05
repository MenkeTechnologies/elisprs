;;; time-convert-decoded.el --- time-convert (TICKS . HZ) + decoded-time setf  -*- lexical-binding: nil; -*-

;; Regression gate for two faithful-port fixes:
;;
;; 1. `time-convert' with FORM `t' must return the highest-resolution
;;    (TICKS . HZ) pair, not a bare float.  Ported from timefns.c
;;    `decode_float_time': an integer stays (N . 1); a float f decomposes
;;    exactly as scale = 53 - (cdr (frexp f)), ticks = f*2^scale, hz = 2^scale.
;;    A numeric FORM is an explicit HZ.  Every value below was captured from
;;    the Emacs 30.2 binary (emacs -Q --batch --eval '(prin1 ...)').
;;    The old bug: (car (time-convert nil t)) errored "listp, <float>" — which
;;    is what broke cl-extra's cl--random-time at load.
;;
;; 2. `decoded-time' is (cl-defstruct (decoded-time (:type list)) ...) in Emacs
;;    (simple.el), so its accessors are setf-able list places.  The prelude had
;;    getters only; adding the setter (setcar (nthcdr INDEX TIME) VAL) lets
;;    time-date.el load and `decoded-time-add' work.

(message "== time-convert / decoded-time demo ==")

(ert-deftest time-convert-form-t ()
  "FORM t yields the exact (TICKS . HZ) pair Emacs 30.2 produces."
  ;; integer seconds -> (N . 1)
  (should (equal (time-convert 3 t) '(3 . 1)))
  (should (equal (time-convert 0 t) '(0 . 1)))
  ;; zero float is special-cased to (0 . 1)
  (should (equal (time-convert 0.0 t) '(0 . 1)))
  ;; exact binary decomposition of common floats
  (should (equal (time-convert 1.5 t) '(6755399441055744 . 4503599627370496)))
  (should (equal (time-convert 2.5 t) '(5629499534213120 . 2251799813685248)))
  (should (equal (time-convert 3.0 t) '(6755399441055744 . 2251799813685248)))
  (should (equal (time-convert -1.5 t) '(-6755399441055744 . 4503599627370496)))
  ;; the pair is exact: ticks/hz reconstructs the input float
  (let ((p (time-convert 2.5 t)))
    (should (= (/ (float (car p)) (cdr p)) 2.5))))

(ert-deftest time-convert-explicit-hz ()
  "A numeric FORM is an explicit HZ denominator."
  (should (equal (time-convert 0 1000) '(0 . 1000)))
  (should (equal (time-convert 2 1000) '(2000 . 1000))))

(ert-deftest time-convert-now-is-integer-pair ()
  "FORM t on the current time gives an integer-pair (car usable by cl--random-time)."
  (let ((p (time-convert nil t)))
    (should (consp p))
    (should (integerp (car p)))
    (should (integerp (cdr p)))))

(ert-deftest time-convert-integer-form ()
  "FORM `integer' truncates to whole seconds (matches Emacs 30.2)."
  (should (= (time-convert 2.9 'integer) 2))
  (should (= (time-convert 5 'integer) 5)))

(ert-deftest decoded-time-setf ()
  "decoded-time accessors are setf-able list places (Emacs :type list struct)."
  (let ((x (list 0 0 0 0 0 2020 0 nil nil)))
    (setf (decoded-time-year x) 2021)
    (should (= (decoded-time-year x) 2021))
    (cl-incf (decoded-time-month x) 3)
    (should (equal x '(0 0 0 0 3 2021 0 nil nil))))
  ;; every slot index maps correctly
  (let ((x (make-list 9 0)))
    (setf (decoded-time-second x) 11)
    (setf (decoded-time-minute x) 22)
    (setf (decoded-time-hour x) 3)
    (setf (decoded-time-day x) 4)
    (setf (decoded-time-month x) 5)
    (setf (decoded-time-year x) 2026)
    (setf (decoded-time-weekday x) 6)
    (setf (decoded-time-dst x) t)
    (setf (decoded-time-zone x) 3600)
    (should (equal x '(11 22 3 4 5 2026 6 t 3600)))))

(ert-run-tests-batch-and-exit)
