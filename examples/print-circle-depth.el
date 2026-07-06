;;; print-circle-depth.el --- printer depth guard vs GNU Emacs  -*- lexical-binding: nil; -*-

;; print.c defines PRINT_CIRCLE = 200. With `print-circle' nil (the default),
;; the printer aborts once nesting reaches that depth, signalling
;;   (error "Apparently circular structure being printed")
;; This is a DEPTH bound, not a length bound: a long *flat* list prints fine,
;; but a structure nested >= 200 deep errors. The boundary is exact — 199 deep
;; prints, 200 deep signals — for both lists and vectors, and it fires from
;; every print entry point (prin1 / prin1-to-string / princ / format %s %S).
;; Every `should' below is oracle-verified against `emacs -Q --batch' 30.2.
(message "== print-circle-depth demo ==")

(defun pcd-nest-list (n)
  "A list nested N levels deep in the car: N=2 => ((nil))."
  (let ((x nil) (i 0))
    (while (< i n) (setq x (list x)) (setq i (1+ i)))
    x))

(defun pcd-nest-vec (n)
  "A vector nested N levels deep: N=2 => [[0]]."
  (let ((x 0) (i 0))
    (while (< i n) (setq x (vector x)) (setq i (1+ i)))
    x))

(defun pcd-signals-circular (thunk)
  "Call THUNK; return the condition object it signals, or `no-error'."
  (condition-case e (progn (funcall thunk) 'no-error) (error e)))

(ert-deftest pcd-list-depth-boundary ()
  "A list 199 deep prints; 200 deep signals the circular-print error."
  (should (stringp (prin1-to-string (pcd-nest-list 199))))
  (should (equal (pcd-signals-circular
                  (lambda () (prin1-to-string (pcd-nest-list 200))))
                 '(error "Apparently circular structure being printed"))))

(ert-deftest pcd-vector-depth-boundary ()
  "Vectors count toward print depth identically to lists."
  (should (stringp (prin1-to-string (pcd-nest-vec 199))))
  (should (equal (pcd-signals-circular
                  (lambda () (prin1-to-string (pcd-nest-vec 200))))
                 '(error "Apparently circular structure being printed"))))

(ert-deftest pcd-depth-not-length ()
  "The guard is on nesting DEPTH, not list length: a 1000-element flat list
prints without error."
  (should (stringp
           (prin1-to-string (let ((x nil) (i 0))
                              (while (< i 1000) (setq x (cons i x)) (setq i (1+ i)))
                              x)))))

(ert-deftest pcd-all-print-entry-points ()
  "prin1 / princ / format %S / format %s all fire the guard at depth 200."
  (let ((deep (pcd-nest-list 200)))
    (should (equal (pcd-signals-circular (lambda () (prin1 deep)))
                   '(error "Apparently circular structure being printed")))
    (should (equal (pcd-signals-circular (lambda () (princ deep)))
                   '(error "Apparently circular structure being printed")))
    (should (equal (pcd-signals-circular (lambda () (format "%S" deep)))
                   '(error "Apparently circular structure being printed")))
    (should (equal (pcd-signals-circular (lambda () (format "%s" deep)))
                   '(error "Apparently circular structure being printed")))))

(ert-run-tests-batch-and-exit)
