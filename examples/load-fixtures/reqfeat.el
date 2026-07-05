;;; reqfeat.el --- fixture: a loadable feature for the `require' tests  -*- lexical-binding: nil; -*-

;; Loaded by examples/custom-load-preload.el via `require'. Lives under
;; load-fixtures/ so run_examples.sh's examples/*.el glob never runs it alone.
(defvar reqfeat-load-count 0
  "Bumped each time this file is loaded — proves `require' loads at most once.")
(setq reqfeat-load-count (1+ reqfeat-load-count))
(defvar reqfeat-marker 'reqfeat-was-loaded)
(provide 'reqfeat)
;;; reqfeat.el ends here
