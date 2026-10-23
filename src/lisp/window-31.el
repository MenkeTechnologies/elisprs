;;; window-31.el --- window functions over the batch window model  -*- lexical-binding: t -*-

;; Verbatim definitions from GNU Emacs 31.1 lisp/window.el, lisp/subr.el and
;; lisp/simple.el, then lisp/frame.el, each in file order: buffer switching, burying,
;; `with-selected-window', `save-window-excursion', and the simple.el commands
;; that read or set window state (`end-of-buffer', `yank-pop',
;; `append-to-buffer').  The window and frame primitives are src/window.rs.

;;;; window.el

(defun window-normalize-buffer (buffer-or-name)
  "Return buffer specified by BUFFER-OR-NAME.
BUFFER-OR-NAME must be a live buffer, a string naming a live
buffer or nil which means to return the current buffer.

This function is commonly used to process the (usually optional)
\"BUFFER-OR-NAME\" argument of window related functions where nil
stands for the current buffer."
  (let ((buffer
         (cond
          ((not buffer-or-name)
           (current-buffer))
          ((bufferp buffer-or-name)
           buffer-or-name)
          ((stringp buffer-or-name)
           (get-buffer buffer-or-name))
          (t
           (error "No such buffer %s" buffer-or-name)))))
    (if (buffer-live-p buffer)
	buffer
      (error "No such live buffer %s" buffer-or-name))))

(defun window-normalize-frame (frame)
  "Return frame specified by FRAME.
FRAME must be a live frame or nil which means to return the
selected frame.

This function is commonly used to process the (usually optional)
\"FRAME\" argument of window and frame related functions where
nil stands for the selected frame."
  (if frame
      (if (frame-live-p frame)
	  frame
	(error "%s is not a live frame" frame))
    (selected-frame)))

(defun window-normalize-window (window &optional live-only)
  "Return window specified by WINDOW.
If WINDOW is nil, return the selected window.  Otherwise, if
WINDOW is a live or an internal window, return WINDOW; if
LIVE-ONLY is non-nil, return WINDOW for a live window only.
Otherwise, signal an error.

This function is commonly used to process the (usually optional)
\"WINDOW\" argument of window related functions where nil stands
for the selected window."
  (cond
   ((null window)
    (selected-window))
   (live-only
    (if (window-live-p window)
	window
      (error "%s is not a live window" window)))
   ((window-valid-p window)
    window)
   (t
    (error "%s is not a valid window" window))))

(defun window-combined-p (&optional window horizontal)
  "Return non-nil if WINDOW has siblings in a given direction.
WINDOW must be a valid window and defaults to the selected one.

HORIZONTAL determines a direction for the window combination.  If
HORIZONTAL is omitted or nil, return non-nil if WINDOW is part of
a vertical window combination.  If HORIZONTAL is non-nil, return
non-nil if WINDOW is part of a horizontal window combination."
  (setq window (window-normalize-window window))
  (let ((parent (window-parent window)))
    (and parent
	 (if horizontal
	     (window-left-child parent)
	   (window-top-child parent)))))

(defalias 'window-height 'window-total-height)

(defalias 'window-width 'window-body-width)

(defun get-buffer-window-list (&optional buffer-or-name minibuf all-frames indirect)
  "Return list of all windows displaying BUFFER-OR-NAME, or nil if none.
BUFFER-OR-NAME may be a buffer or the name of an existing buffer
and defaults to the current buffer.  If the selected window displays
BUFFER-OR-NAME, it will be the first in the resulting list.

MINIBUF t means include the minibuffer window even if the
minibuffer is not active.  MINIBUF nil or omitted means include
the minibuffer window only if the minibuffer is active.  Any
other value means do not include the minibuffer window even if
the minibuffer is active.

ALL-FRAMES nil or omitted means consider all windows on the
selected frame, plus the minibuffer window if specified by the
MINIBUF argument.  If the minibuffer counts, consider all windows
on all frames that share that minibuffer too.  The following
non-nil values of ALL-FRAMES have special meanings:

- t means consider all windows on all existing frames.

- `visible' means consider all windows on all visible frames on
  the current terminal.

- 0 (the number zero) means consider all windows on all visible
  and iconified frames on the current terminal.

- A frame means consider all windows on that frame only.

Anything else means consider all windows on the selected frame
and no others.

INDIRECT non-nil means to append to the list of windows showing
BUFFER-OR-NAME a list of all windows that are indirectly related to
BUFFER-OR-NAME, that is, windows for which `window-indirect-buffer-p'
with the window and the buffer specified by BUFFER-OR-NAME as arguments
returns non-nil."
  (let ((buffer (window-normalize-buffer buffer-or-name))
	(window-list (window-list-1 (selected-window) minibuf all-frames))
	windows)
    (dolist (window window-list)
      (when (eq (window-buffer window) buffer)
	(setq windows (cons window windows))))
    (when indirect
      (dolist (window window-list)
	(when (window-indirect-buffer-p window buffer)
	  (setq windows (cons window windows)))))
    (nreverse windows)))

(defun count-windows (&optional minibuf all-frames)
   "Return the number of live windows on the selected frame.

The optional argument MINIBUF specifies whether the minibuffer
window is included in the count.

If ALL-FRAMES is non-nil, count the windows in all frames instead
just the selected frame.

See `walk-windows' for the precise meaning of these arguments."
   (length (window-list-1 nil minibuf all-frames)))

(defun one-window-p (&optional nomini all-frames)
  "Return non-nil if the selected window is the only window.
Optional arg NOMINI non-nil means don't count the minibuffer
even if it is active.  Otherwise, the minibuffer is counted
when it is active.

Optional argument ALL-FRAMES specifies the set of frames to
consider, see also `next-window'.  ALL-FRAMES nil or omitted
means consider windows on the selected frame only, plus the
minibuffer window if specified by the NOMINI argument.  If the
minibuffer counts, consider all windows on all frames that share
that minibuffer too.  The remaining non-nil values of ALL-FRAMES
with a special meaning are:

- t means consider all windows on all existing frames.

- `visible' means consider all windows on all visible frames on
  the current terminal.

- 0 (the number zero) means consider all windows on all visible
  and iconified frames on the current terminal.

- A frame means consider all windows on that frame only.

Anything else means consider all windows on the selected frame
and no others."
  (declare (ftype (function (&optional t t) boolean))
           (side-effect-free error-free))
  (let ((base-window (selected-window)))
    (if (and nomini (eq base-window (minibuffer-window)))
	(setq base-window (next-window base-window)))
    (eq base-window
	(next-window base-window (if nomini 'arg) all-frames))))

(defun record-window-buffer (&optional window)
  "Record WINDOW's buffer.
Add the buffer currently shown in WINDOW to the list of WINDOW's
previous buffers.  WINDOW must be a live window and defaults to the
selected one.

If WINDOW is not a minibuffer window, do not record insignificant
buffers (buffers whose name starts with a space).  If WINDOW is a
minibuffer window, record its buffer if and only if that buffer is a
live minibuffer (`minibufferp' with LIVE argument non-nil must return
non-nil for it).

Run `buffer-list-update-hook' if and only if WINDOW is not a minibuffer
window."
  (let* ((window (window-normalize-window window t))
	 (mini (window-minibuffer-p window))
         (buffer (window-buffer window))
         (prev-buffers (window-prev-buffers window))
         (entry (assq buffer prev-buffers)))
    (when entry
      (setq prev-buffers (assq-delete-all buffer prev-buffers)))

    ;; Reset WINDOW's next buffers.  If needed, they are resurrected by
    ;; `switch-to-prev-buffer' and `switch-to-next-buffer'.
    (set-window-next-buffers window nil)

    ;; For minibuffer windows record live minibuffers only.  For normal
    ;; windows do not record insignificant buffers.
    (when (if mini
	      (minibufferp buffer t)
	    (not (eq (aref (buffer-name buffer) 0) ?\s)))
      (let ((start (window-start window))
            (point (window-point window)))
	(setq entry
              (cons buffer
                    (with-current-buffer buffer
                      (if entry
                          ;; We have an entry, update marker positions.
                          (list (set-marker (nth 1 entry) start)
				(set-marker (nth 2 entry) point))
			(list (copy-marker start)
                              (copy-marker
                               ;; Preserve window-point-insertion-type
                               ;; (Bug#12855)
                               point window-point-insertion-type))))))
	(set-window-prev-buffers window (cons entry prev-buffers))

	(unless mini
	  (run-hooks 'buffer-list-update-hook))))))

(defun bury-buffer (&optional buffer-or-name)
  "Put BUFFER-OR-NAME at the end of the list of all buffers.
There it is the least likely candidate for `other-buffer' to
return; thus, the least likely buffer for \\[switch-to-buffer] to
select by default.

You can specify a buffer name as BUFFER-OR-NAME, or an actual
buffer object.  If BUFFER-OR-NAME is nil or omitted, bury the
current buffer.  Also, if BUFFER-OR-NAME is nil or omitted,
remove the current buffer from the selected window if it is
displayed there."
  (interactive)
  (let* ((buffer (window-normalize-buffer buffer-or-name)))
    ;; If `buffer-or-name' is not on the selected frame we unrecord it
    ;; although it's not "here" (call it a feature).
    (bury-buffer-internal buffer)
    ;; Handle case where `buffer-or-name' is nil and the current buffer
    ;; is shown in the selected window.
    (cond
     ((or buffer-or-name
          (not (eq buffer (window-buffer)))
          ;; Don't try to delete the minibuffer window, undedicate it
          ;; or switch to a previous buffer in it.
          (window-minibuffer-p)))
     ((window--delete nil t))
     (t
      ;; Switch to another buffer in window.
      (set-window-dedicated-p nil nil)
      (switch-to-prev-buffer nil 'bury)))
    ;; Always return nil.
    nil))

(defun display-buffer-record-window (type window buffer)
  "Record information for window used by `display-buffer'.
WINDOW is the window used for or created by a buffer display
action function.  BUFFER is the buffer to display.  Note that
this function must be called before BUFFER is explicitly made
WINDOW's buffer (although WINDOW may show BUFFER already).

TYPE specifies the type of the calling operation and must be one
of the symbols `reuse' (meaning that WINDOW exists already and
will be used for displaying BUFFER), `window' (WINDOW was created
on an already existing frame), `frame' (WINDOW was created on a
new frame) or `tab' (WINDOW is the selected window and BUFFER was
created in a new tab).

This function installs or updates the `quit-restore' parameter of
WINDOW.  The `quit-restore' parameter is a list of four elements:
The first element is one of the symbols `window', `frame', `same'
or `other'.  The second element is either one of the symbols
`window' or `frame' or a list whose elements are the buffer
previously shown in the window, that buffer's window start and
window point, and the window's height.  The third element is the
window selected at the time the parameter was created.  The
fourth element is BUFFER.

If TYPE is `reuse', BUFFER is different from the one currently displayed
in WINDOW, and WINDOW already has a `quit-restore' parameter, install or
update a `quit-restore-prev' parameter for this window.  This allows for
quitting WINDOW in a similar fashion but also keeps the very first
`quit-restore' parameter stored for this window around.  Consequently,
WINDOW (or its frame) can be eventually deleted by `quit-restore-widow'
if that parameter's fourth element equals WINDOW's buffer."
  (cond
   ((eq type 'reuse)
    (let ((quit-restore (window-parameter window 'quit-restore)))
      (if (eq (window-buffer window) buffer)
	  ;; WINDOW shows BUFFER already.  Update WINDOW's quit-restore
	  ;; parameter, if any.
	  (when (consp quit-restore)
	    (setcar quit-restore 'same)
	    ;; The selected-window might have changed in
	    ;; between (Bug#20353).
	    (unless (or (eq window (selected-window))
			(eq window (nth 2 quit-restore)))
	      (setcar (cddr quit-restore) (selected-window))))
	;; WINDOW shows another buffer.
	(with-current-buffer (window-buffer window)
	  (set-window-parameter
	   window (if quit-restore 'quit-restore-prev 'quit-restore)
	   (list 'other
		 ;; A quadruple of WINDOW's buffer, start, point and height.
		 (list (current-buffer) (window-start window)
		       ;; Preserve window-point-insertion-type (Bug#12855).
		       (copy-marker
			(window-point window) window-point-insertion-type)
		       (if (window-combined-p window)
                           (window-total-height window)
			 (window-total-width window)))
		 (selected-window) buffer))))))
   ((eq type 'window)
    ;; WINDOW has been created on an existing frame.
    (set-window-parameter
     window 'quit-restore
     (list 'window 'window (selected-window) buffer)))
   ((eq type 'frame)
    ;; WINDOW has been created on a new frame.
    (set-window-parameter
     window 'quit-restore
     (list 'frame 'frame (selected-window) buffer)))
   ((eq type 'tab)
    ;; WINDOW has been created on a new tab.
    (set-window-parameter
     window 'quit-restore
     (list 'tab 'tab (selected-window) buffer)))))

(defun window-normalize-buffer-to-switch-to (buffer-or-name)
  "Normalize BUFFER-OR-NAME argument of buffer switching functions.
If BUFFER-OR-NAME is nil, return the buffer returned by
`other-buffer'.  Else, if a buffer specified by BUFFER-OR-NAME
exists, return that buffer.  If no such buffer exists, create a
buffer with the name BUFFER-OR-NAME and return that buffer."
  (pcase buffer-or-name
    ('nil (other-buffer))
    ("*scratch*" (get-scratch-buffer-create))
    (_ (or (get-buffer buffer-or-name)
	   (let ((buffer (get-buffer-create buffer-or-name)))
	     (set-buffer-major-mode buffer)
	     buffer)))))

(defcustom switch-to-buffer-preserve-window-point t
  "If non-nil, `switch-to-buffer' tries to preserve `window-point'.
If this is nil, `switch-to-buffer' displays the buffer at that
buffer's `point'.  If this is `already-displayed', it tries to
display the buffer at its previous position in the selected
window, provided the buffer is currently displayed in some other
window on any visible or iconified frame.  If this is t, it
unconditionally tries to display the buffer at its previous
position in the selected window.

This variable is ignored if the buffer is already displayed in
the selected window or never appeared in it before, or if
`switch-to-buffer' calls `pop-to-buffer' to display the buffer,
or non-nil `switch-to-buffer-obey-display-actions' displays it
in another window."
  :type '(choice
	  (const :tag "Never" nil)
	  (const :tag "If already displayed elsewhere" already-displayed)
	  (const :tag "Always" t))
  :group 'windows
  :version "26.1")

(defcustom switch-to-buffer-in-dedicated-window nil
  "Allow switching to buffer in strongly dedicated windows.
If non-nil, allow `switch-to-buffer' to proceed when called
interactively and the selected window is strongly dedicated to
its buffer.

The following values are recognized:

nil - disallow switching; signal an error

prompt - prompt user whether to allow switching

pop - perform `pop-to-buffer' instead

t - undedicate selected window and switch

When called non-interactively, `switch-to-buffer' always signals
an error when the selected window is dedicated to its buffer and
FORCE-SAME-WINDOW is non-nil."
  :type '(choice
	  (const :tag "Disallow" nil)
	  (const :tag "Prompt" prompt)
	  (const :tag "Pop" pop)
	  (const :tag "Allow" t))
  :group 'windows
  :version "25.1")

(defcustom switch-to-buffer-obey-display-actions nil
  "If non-nil, `switch-to-buffer' runs `pop-to-buffer-same-window' instead.
This means that when switching the buffer it respects display actions
specified by `display-buffer-overriding-action', `display-buffer-alist'
and other display related variables.  So `switch-to-buffer' will display
the buffer in the window specified by the rules from these variables."
  :type 'boolean
  :group 'windows
  :version "27.1")

(defun switch-to-buffer (buffer-or-name &optional norecord force-same-window)
  "Display buffer BUFFER-OR-NAME in the selected window.

WARNING: This is NOT the way to work on another buffer temporarily
within a Lisp program!  Use `set-buffer' instead.  That avoids
messing with the `window-buffer' correspondences.

If the selected window cannot display the specified buffer
because it is a minibuffer window or strongly dedicated to
another buffer, call `pop-to-buffer' to select the buffer in
another window.  In interactive use, if the selected window is
strongly dedicated to its buffer, the value of the option
`switch-to-buffer-in-dedicated-window' specifies how to proceed.

If called interactively, read the buffer name using `read-buffer'.
The variable `confirm-nonexistent-file-or-buffer' determines
whether to request confirmation before creating a new buffer.
See `read-buffer' for features related to input and completion
of buffer names.

BUFFER-OR-NAME may be a buffer, a string (a buffer name), or nil.
If BUFFER-OR-NAME is a string that does not identify an existing
buffer, create a buffer with that name.  If BUFFER-OR-NAME is
nil, switch to the buffer returned by `other-buffer'.

If optional argument NORECORD is non-nil, do not put the buffer
at the front of the buffer list, and do not make the window
displaying it the most recently selected one.

If optional argument FORCE-SAME-WINDOW is non-nil, the buffer
must be displayed in the selected window when called
non-interactively; if that is impossible, signal an error rather
than calling `pop-to-buffer'.  It has no effect when the option
`switch-to-buffer-obey-display-actions' is non-nil.

The option `switch-to-buffer-preserve-window-point' can be used
to make the buffer appear at its last position in the selected
window.

If the option `switch-to-buffer-obey-display-actions' is non-nil,
run the function `pop-to-buffer-same-window' instead.
This may display the buffer in another window as specified by
`display-buffer-overriding-action', `display-buffer-alist' and
other display related variables.  If this results in displaying
the buffer in the selected window, window start and point are adjusted
as prescribed by the option `switch-to-buffer-preserve-window-point'.
Otherwise, these are left alone.

In either case, call `display-buffer-record-window' to avoid disrupting
a sequence of `display-buffer' operations using this window.

Return the buffer switched to."
  (interactive
   (let ((force-same-window
          (unless switch-to-buffer-obey-display-actions
            (cond
             ((window-minibuffer-p) nil)
             ((not (eq (window-dedicated-p) t)) 'force-same-window)
             ((pcase switch-to-buffer-in-dedicated-window
                ('nil (user-error
                       "Cannot switch buffers in a dedicated window"))
                ('prompt
                 (if (y-or-n-p
                      (format "Window is dedicated to %s; undedicate it?"
                              (window-buffer)))
                     (progn
                       (set-window-dedicated-p nil nil)
                       'force-same-window)
                   (user-error
                    "Cannot switch buffers in a dedicated window")))
                ('pop nil)
                (_ (set-window-dedicated-p nil nil) 'force-same-window)))))))
     (list (read-buffer-to-switch "Switch to buffer: ") nil force-same-window)))
  (let ((buffer (window-normalize-buffer-to-switch-to buffer-or-name))
        (set-window-start-and-point (not switch-to-buffer-obey-display-actions)))
    (cond
     ;; Don't call set-window-buffer if it's not needed since it
     ;; might signal an error (e.g. if the window is dedicated).
     ((and (eq buffer (window-buffer))
           ;; pop-to-buffer-same-window might decide to display
           ;; the same buffer in another window
           (not switch-to-buffer-obey-display-actions)))
     ((and (window-minibuffer-p)
           (not switch-to-buffer-obey-display-actions))
      (if force-same-window
          (user-error "Cannot switch buffers in minibuffer window")
        (pop-to-buffer buffer norecord)))
     ((and (eq (window-dedicated-p) t)
           (not switch-to-buffer-obey-display-actions))
      (if force-same-window
          (user-error "Cannot switch buffers in a dedicated window")
        (pop-to-buffer buffer norecord)))
     (t
      (when switch-to-buffer-obey-display-actions
        (let* ((selected-window (selected-window))
	       (old-window-buffer (window-buffer selected-window)))
          (pop-to-buffer-same-window buffer norecord)
	  ;; Do not ask for setting start and point when showing the
	  ;; same buffer in the old selected window (Bug#71616).
          (when (and (eq (selected-window) selected-window)
		     (not (eq (window-buffer selected-window)
			      old-window-buffer)))
            (setq set-window-start-and-point t))))

      (when set-window-start-and-point
        (let* ((entry (assq buffer (window-prev-buffers)))
               (preserve-win-point
                (buffer-local-value 'switch-to-buffer-preserve-window-point
                                    buffer))
	       (displayed (and (eq preserve-win-point 'already-displayed)
			       (get-buffer-window buffer 0))))

	  ;; Make sure quitting the window works.
	  (unless switch-to-buffer-obey-display-actions
	    (display-buffer-record-window 'reuse (selected-window) buffer))

	  (set-window-buffer nil buffer)
	  (when (and entry (or (eq preserve-win-point t) displayed))
	    ;; Try to restore start and point of buffer in the selected
	    ;; window (Bug#4041).
	    (set-window-start (selected-window) (nth 1 entry) t)
	    (set-window-point nil (nth 2 entry)))))))

    (unless norecord
      (select-window (selected-window)))
    (set-buffer buffer)))

(defvar-local window-group-start-function nil)

(defun window-group-start (&optional window)
  "Return position at which display currently starts in the group of
windows containing WINDOW.  When a grouping mode (such as Follow Mode)
is not active, this function is identical to `window-start'.

WINDOW must be a live window and defaults to the selected one.
This is updated by redisplay or by calling `set-window*-start'."
  (if (functionp window-group-start-function)
      (funcall window-group-start-function window)
    (window-start window)))

(defvar-local window-group-end-function nil)

(defun window-group-end (&optional window update)
  "Return position at which display currently ends in the group of
windows containing WINDOW.  When a grouping mode (such as Follow Mode)
is not active, this function is identical to `window-end'.

WINDOW must be a live window and defaults to the selected one.
This is updated by redisplay, when it runs to completion.
Simply changing the buffer text or setting `window-group-start'
does not update this value.
Return nil if there is no recorded value.  (This can happen if the
last redisplay of WINDOW was preempted, and did not finish.)
If UPDATE is non-nil, compute the up-to-date position
if it isn't already recorded."
  (if (functionp window-group-end-function)
      (funcall window-group-end-function window update)
    (window-end window update)))

(defvar-local selected-window-group-function nil)

(defun selected-window-group ()
  "Return the list of windows in the group containing the selected window.
When a grouping mode (such as Follow Mode) is not active, the
result is a list containing only the selected window."
  (if (functionp selected-window-group-function)
      (funcall selected-window-group-function)
    (list (selected-window))))

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

