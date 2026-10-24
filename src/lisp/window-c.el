;;; window-c.el --- the variables of window.c  -*- lexical-binding: t -*-

;; window.c's DEFVARs with the values `emacs -Q --batch' (GNU Emacs 31.1)
;; has; the window primitives themselves are src/window.rs.

(defvar temp-buffer-show-function nil)
(defvar minibuffer-scroll-window nil)
(defvar mode-line-in-non-selected-windows t)
(defvar other-window-scroll-buffer nil)
(defvar other-window-scroll-default nil)
(defvar auto-window-vscroll t)
(defvar next-screen-context-lines 2)
(defvar scroll-preserve-screen-position nil)
(defvar window-point-insertion-type nil)
(defvar window-buffer-change-functions nil)
(defvar window-size-change-functions nil)
(defvar window-selection-change-functions nil)
(defvar window-state-change-functions nil)
(defvar window-state-change-hook nil)
(defvar window-configuration-change-hook nil)
(defvar window-restore-killed-buffer-windows nil)
(defvar recenter-redisplay 'tty)
(defvar window-combination-resize nil)
(defvar window-combination-limit 'window-size)
(defvar window-persistent-parameters
  (list (cons 'context 'writable) (cons 'clone-of t)))
(defvar window-resize-pixelwise nil)
(defvar fast-but-imprecise-scrolling nil)
(defvar window-auto-redraw-on-parameter-change t)

;;;; xdisp.c

(defvar window-scroll-functions nil)

;;;; frame.c

(defvar frame-resize-pixelwise nil)
(defvar frame-inhibit-implied-resize t)

;;;; window.c functions that run hooks

(defun run-window-scroll-functions (&optional window)
  "Run `window-scroll-functions' for WINDOW.
If WINDOW is omitted or nil, it defaults to the selected window.

This function is called by `split-window' for the new window, after it
has established the size of the new window."
  (setq window (window-normalize-window window t))
  (with-current-buffer (window-buffer window)
    (when window-scroll-functions
      (run-hook-with-args 'window-scroll-functions
                          window (window-start window))))
  nil)

(defun run-window-configuration-change-hook (&optional frame)
  "Run `window-configuration-change-hook' for FRAME.
If FRAME is omitted or nil, it defaults to the selected frame.

This function should not be needed any more and will be therefore
considered obsolete."
  (setq frame (window-normalize-frame frame))
  (let ((global (default-value 'window-configuration-change-hook)))
    (with-current-buffer (window-buffer)
      (dolist (window (window-list frame 'lambda))
        (let ((buffer (window-buffer window)))
          (when (local-variable-p 'window-configuration-change-hook buffer)
            (save-selected-window
              (select-window window t)
              (dolist (fun (buffer-local-value
                            'window-configuration-change-hook buffer))
                (unless (eq fun t) (funcall fun)))))))
      (dolist (fun global)
        (unless (eq fun t) (funcall fun)))))
  nil)
