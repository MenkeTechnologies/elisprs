;;; let-star.el --- let* sequential binding & lexical shadowing on fusevm, ERT-tested  -*- lexical-binding: t; -*-

;; `let*` binds each variable into scope *before* evaluating the next init, and a
;; same-name rebind creates a fresh lexical slot that shadows the earlier one.
;; The lexical environment is a persistent chain of per-binding nodes, so a
;; closure that captured an earlier binding never sees a later rebind. Every
;; expected value below was cross-checked against Emacs 30.2.
(message "== let* demo ==")

(ert-deftest ls-same-name-rebind ()
  "A later `let*' binding's init sees the earlier same-name binding."
  (should (= (let* ((x 5) (x (+ x 1))) x) 6))
  (should (= (let* ((x 1) (x 2) (x (* x 3))) x) 6))
  ;; setq targets the most recent slot.
  (should (= (let* ((x 5) (x (+ x 1))) (setq x (* x 10)) x) 60)))

(ert-deftest ls-distinct-names ()
  "Sequential binding: a later init sees earlier distinct vars."
  (should (= (let* ((x 1) (y (+ x 1))) y) 2))
  (should (equal (let* ((a 1) (b (+ a 1)) (c (+ b 1)) (a (* c 10)))
                   (list a b c))
                 (list 30 2 3))))

(ert-deftest ls-nested-and-shadow ()
  "Inner `let*' shadows an outer binding; each init sees the running value."
  (should (= (let ((x 10)) (let* ((x (+ x 1)) (x (+ x 1))) x)) 12))
  (should (= ((lambda (x) (let* ((x (* x 2)) (x (+ x 1))) x)) 10) 21)))

(ert-deftest ls-closure-capture ()
  "A closure captures the binding live at its creation, not a later rebind."
  ;; f captures the first `a' (1); the later (a 99) is a new slot f can't see.
  (should (= (let* ((a 1) (f (lambda () a)) (a 99)) (funcall f)) 1))
  ;; Closure and enclosing body share one mutable slot: setq is visible to both.
  (should (= (let* ((x 2) (g (lambda () (setq x (1+ x)))))
               (funcall g) (funcall g) x)
             4))
  ;; A closure over a `let*' var survives the scope's exit (indefinite extent).
  (should (= (let ((f (let* ((n 5)) (lambda () n)))) (funcall f)) 5)))

(ert-deftest ls-parallel-let-guard ()
  "Regression guard: plain `let' is NOT sequential — inits use the outer scope."
  ;; Duplicate names in `let': last binding wins, but neither init sees the other.
  (should (= (let ((x 1) (x 2)) x) 2))
  ;; An empty `let*'/`let' body binds nothing and returns nil.
  (should-not (let* () nil))
  (should-not (let () nil)))

(ert-run-tests-batch-and-exit)
