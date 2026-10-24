;;; window-31.el --- window functions over the batch window model  -*- lexical-binding: t -*-

;; Verbatim definitions from GNU Emacs 31.1 lisp/subr.el and lisp/simple.el,
;; then lisp/frame.el, each in file order: `with-selected-window',
;; `save-window-excursion', and the simple.el commands that read or set
;; window state (`end-of-buffer', `yank-pop', `append-to-buffer').
;; lisp/window.el itself is src/lisp/window.el; the window and frame
;; primitives are src/window.rs.

;;;; subr.el

(defun internal--before-with-selected-window (window)
  (let ((other-frame (window-frame window)))
    (list window (selected-window)
          ;; Selecting a window on another frame also changes that
          ;; frame's frame-selected-window.  We must save&restore it.
          (unless (eq (selected-frame) other-frame)
            (frame-selected-window other-frame))
          ;; Also remember the top-frame if on ttys.
          (unless (eq (selected-frame) other-frame)
            (tty-top-frame other-frame)))))

(defun internal--after-with-selected-window (state)
  ;; First reset frame-selected-window.
  (when (window-live-p (nth 2 state))
    ;; We don't use set-frame-selected-window because it does not
    ;; pass the `norecord' argument to Fselect_window.
    (select-window (nth 2 state) 'norecord)
    (and (frame-live-p (nth 3 state))
         (not (eq (tty-top-frame) (nth 3 state)))
         (select-frame (nth 3 state) 'norecord)))
  ;; Then reset the actual selected-window.
  (when (window-live-p (nth 1 state))
    (select-window (nth 1 state) 'norecord)))

(defmacro with-selected-window (window &rest body)
  "Execute the forms in BODY with WINDOW as the selected window.
The value returned is the value of the last form in BODY.

This macro saves and restores the selected window, as well as the
selected window of each frame.  It does not change the order of
recently selected windows.  If the previously selected window of
some frame is no longer live at the end of BODY, that frame's
selected window is left alone.  If the selected window is no
longer live, then whatever window is selected at the end of BODY
remains selected.

This macro uses `save-current-buffer' to save and restore the
current buffer, since otherwise its normal operation could
potentially make a different buffer current.  It does not alter
the buffer list ordering."
  (declare (indent 1) (debug t))
  `(let ((save-selected-window--state
          (internal--before-with-selected-window ,window)))
     (save-current-buffer
       (unwind-protect
           (progn (select-window (car save-selected-window--state) 'norecord)
		  ,@body)
         (internal--after-with-selected-window save-selected-window--state)))))

(defmacro save-window-excursion (&rest body)
  "Execute BODY, then restore previous window configuration.
This macro saves the window configuration on the selected frame,
executes BODY, then calls `set-window-configuration' to restore
the saved window configuration.  The return value is the last
form in BODY.  The window configuration is also restored if BODY
exits nonlocally.

BEWARE: Most uses of this macro introduce bugs.
E.g. it should not be used to try and prevent some code from opening
a new window, since that window may sometimes appear in another frame,
in which case `save-window-excursion' cannot help."
  (declare (indent 0) (debug t))
  (let ((c (make-symbol "wconfig")))
    `(let ((,c (current-window-configuration)))
       (unwind-protect (progn ,@body)
         (set-window-configuration ,c)))))

;;;; simple.el

(defun end-of-buffer (&optional arg)
  "Move point to the end of the buffer.
With numeric arg N, put point N/10 of the way from the end.
If the buffer is narrowed, this command uses the end of the
accessible part of the buffer.

Push mark at previous position, unless either a \\[universal-argument] prefix
is supplied, or Transient Mark mode is enabled and the mark is active."
  (declare (interactive-only "use `(goto-char (point-max))' instead."))
  (interactive "^P")
  (or (consp arg) (region-active-p) (push-mark))
  (let ((size (- (point-max) (point-min))))
    (goto-char (if (and arg (not (consp arg)))
		   (- (point-max)
		      (/ (* size (prefix-numeric-value arg)) 10))
		 (point-max))))
  ;; If we went to a place in the middle of the buffer,
  ;; adjust it to the beginning of a line.
  (cond ((and arg (not (consp arg))) (forward-line 1))
	((and (eq (current-buffer) (window-buffer))
              (> (point) (window-end nil t)))
	 ;; If the end of the buffer is not already on the screen,
	 ;; then scroll specially to put it near, but not at, the bottom.
	 (overlay-recenter (point))
	 ;; FIXME: Arguably if `scroll-conservatively' is set, then
         ;; we should pass -1 to `recenter'.
	 (recenter (if (and scroll-minibuffer-conservatively
	                    (window-minibuffer-p))
	               -1 -3)))))

(defvar yank-window-start nil)

(defun yank-pop (&optional arg)
  "Replace just-yanked stretch of killed text with a different stretch.
The main use of this command is immediately after a `yank' or a
`yank-pop'.  At such a time, the region contains a stretch of
reinserted (\"pasted\") previously-killed text.  `yank-pop' deletes
that text and inserts in its place a different stretch of killed text
by traversing the value of the `kill-ring' variable and selecting
another kill from there.

With no argument, the previous kill is inserted.
With argument N, insert the Nth previous kill.
If N is negative, it means to use a more recent kill.

The sequence of kills wraps around, so if you keep invoking this command
time after time, and pass the oldest kill, you get the newest one.

You can also invoke this command after a command other than `yank'
or `yank-pop'.  This is the same as invoking `yank-from-kill-ring',
including the effect of the prefix argument; see there for the details.

This command honors the `yank-handled-properties' and
`yank-excluded-properties' variables, and the `yank-handler' text
property, in the way that `yank' does."
  (interactive "p")
  (if (not (eq last-command 'yank))
      (yank-from-kill-ring (read-from-kill-ring "Yank from kill-ring: ")
                           current-prefix-arg)
    (setq this-command 'yank)
    (unless arg (setq arg 1))
    (let ((inhibit-read-only t)
          (before (< (point) (mark t))))
      (if before
          (funcall (or yank-undo-function 'delete-region) (point) (mark t))
        (funcall (or yank-undo-function 'delete-region) (mark t) (point)))
      (setq yank-undo-function nil)
      (set-marker (mark-marker) (point) (current-buffer))
      (insert-for-yank (current-kill arg))
      ;; Set the window start back where it was in the yank command,
      ;; if possible.
      (set-window-start (selected-window) yank-window-start t)
      (if before
          ;; This is like exchange-point-and-mark, but doesn't activate the mark.
          ;; It is cleaner to avoid activation, even though the command
          ;; loop would deactivate the mark because we inserted text.
          (goto-char (prog1 (mark t)
                       (set-marker (mark-marker) (point) (current-buffer))))))
    nil))

(defun append-to-buffer (buffer start end)
  "Append to specified BUFFER the text of the region.
The text is inserted into that buffer before its point.
BUFFER can be a buffer or the name of a buffer; this
function will create BUFFER if it doesn't already exist.

When calling from a program, give three arguments:
BUFFER (or buffer name), START and END.
START and END specify the portion of the current buffer to be copied."
  (interactive
   (list (read-buffer "Append to buffer: " (other-buffer (current-buffer) t))
	 (region-beginning) (region-end)))
  (let* ((oldbuf (current-buffer))
         (append-to (get-buffer-create buffer))
         (windows (get-buffer-window-list append-to t t))
         point)
    (save-excursion
      (with-current-buffer append-to
        (setq point (point))
        (barf-if-buffer-read-only)
        (insert-buffer-substring oldbuf start end)
        (dolist (window windows)
          (when (= (window-point window) point)
            (set-window-point window (point))))))))

(defun get-scratch-buffer-create ()
  "Return the *scratch* buffer, creating a new one if needed."
  (or (get-buffer "*scratch*")
      (let ((scratch (get-buffer-create "*scratch*")))
        ;; Don't touch the buffer contents or mode unless we know that
        ;; we just created it.
        (with-current-buffer scratch
          (when initial-scratch-message
            (insert (substitute-command-keys initial-scratch-message))
            (set-buffer-modified-p nil))
          (funcall initial-major-mode)
          (when (eq initial-major-mode 'lisp-interaction-mode)
            (setq-local trusted-content :all)))
        scratch)))

;;;; frame.el

(defun framep-on-display (&optional terminal)
  "Return the type of frames on TERMINAL.
TERMINAL may be a terminal id, a display name or a frame.  If it
is a frame, its type is returned.  If TERMINAL is omitted or nil,
it defaults to the selected frame's terminal device.  All frames
on a given display are of the same type."
  (or (terminal-live-p terminal)
      (framep terminal)
      (framep (car (frames-on-display-list terminal)))))

(defun frame-height (&optional frame)
  "Return number of lines available for display on FRAME.
If FRAME is omitted, describe the currently selected frame.
Exactly what is included in the return value depends on the
window-system and toolkit in use - see `frame-pixel-height' for
more details.  The lines are in units of the default font height.

The result is roughly related to the frame pixel height via
height in pixels = height in lines * `frame-char-height'.
However, this is only approximate, and is complicated e.g. by the
fact that individual window lines and menu bar lines can have
differing font heights."
  (cdr (assq 'height (frame-parameters frame))))

(defun frame-width (&optional frame)
  "Return number of columns available for display on FRAME.
If FRAME is omitted, describe the currently selected frame."
  (cdr (assq 'width (frame-parameters frame))))

(defun frame-edges (&optional frame type)
  "Return coordinates of FRAME's edges.
FRAME must be a live frame and defaults to the selected one.  The
list returned has the form (LEFT TOP RIGHT BOTTOM) where all
values are in pixels relative to the origin - the position (0, 0)
- of FRAME's display.  For terminal frames all values are
relative to LEFT and TOP which are both zero.

Optional argument TYPE specifies the type of the edges.  TYPE
`outer-edges' means to return the outer edges of FRAME.  TYPE
`native-edges' (or nil) means to return the native edges of
FRAME.  TYPE `inner-edges' means to return the inner edges of
FRAME."
  (let* ((frame (window-normalize-frame frame))
	 (frame-type (framep-on-display frame)))
    (cond
     ((eq frame-type 'x)
      (x-frame-edges frame type))
     ((eq frame-type 'w32)
      (w32-frame-edges frame type))
     ((eq frame-type 'ns)
      (ns-frame-edges frame type))
     ((eq frame-type 'pgtk)
      (pgtk-frame-edges frame type))
     ((eq frame-type 'haiku)
      (haiku-frame-edges frame type))
     ((eq frame-type 'android)
      (android-frame-edges frame type))
     (t
      (tty-frame-edges frame type)))))


(defun display-graphic-p (&optional display)
  "Return non-nil if DISPLAY is a graphic display.
Graphical displays are those which are capable of displaying several
frames and several different fonts at once.  This is true for displays
that use a window system such as X, and false for text-only terminals.
DISPLAY can be a display name, a frame, or nil (meaning the selected
frame's display)."
  (not (null (memq (framep-on-display display) '(x w32 ns pgtk haiku
                                                   android)))))

(defun display-images-p (&optional display)
  "Return non-nil if DISPLAY can display images.

DISPLAY can be a display name, a frame, or nil (meaning the selected
frame's display)."
  (and (display-graphic-p display)
       (fboundp 'image-mask-p)
       (fboundp 'image-size)))

(defalias 'display-blink-cursor-p #'display-graphic-p)
(defalias 'display-multi-frame-p #'display-graphic-p)
(defalias 'display-multi-font-p #'display-graphic-p)
