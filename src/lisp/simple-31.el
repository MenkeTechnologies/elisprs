;;; simple-31.el --- simple.el, lisp.el and text-mode.el commands the prelude lacked  -*- lexical-binding: t -*-

;; Verbatim definitions from GNU Emacs 31.1 lisp/simple.el, then
;; lisp/emacs-lisp/lisp.el and lisp/textmodes/text-mode.el, each in file order.
;; `delete-forward-char' (it needs compositions) is not here; the commands
;; that read window state are in window-31.el.

;;;; simple.el

(defun split-line (&optional arg)
  "Split current line, moving portion beyond point vertically down.
If the current line starts with `fill-prefix', insert it on the new
line as well.  With prefix ARG, don't insert `fill-prefix' on new line.

When called from Lisp code, ARG may be a prefix string to copy."
  (interactive "*P")
  (skip-chars-forward " \t")
  (let* ((col (current-column))
	 (pos (point))
	 ;; What prefix should we check for (nil means don't).
	 (prefix (cond ((stringp arg) arg)
		       (arg nil)
		       (t fill-prefix)))
	 ;; Does this line start with it?
	 (have-prfx (and prefix
			 (save-excursion
			   (beginning-of-line)
			   (looking-at (regexp-quote prefix))))))
    (newline 1)
    (if have-prfx (insert-and-inherit prefix))
    (indent-to col 0)
    (goto-char pos)))

(defun region-modifiable-p (start end)
  "Return non-nil if the region contains no read-only text."
  (and (not (get-text-property start 'read-only))
       (eq end (next-single-property-change start 'read-only nil end))))

(defun delete-trailing-whitespace-if-possible ()
  "Call `delete-trailing-whitespace' unless the buffer is read-only."
  (unless buffer-read-only (delete-trailing-whitespace)))

(defun newline-and-indent (&optional arg)
  "Insert a newline, then indent according to major mode.
Indentation is done using the value of `indent-line-function'.
In programming language modes, this is the same as TAB.
In some text modes, where TAB inserts a tab, this command indents to the
column specified by the function `current-left-margin'.

With ARG, perform this action that many times.

Also see `open-line' (bound to \\[open-line]) for a command that
just inserts a newline without doing any indentation."
  (interactive "*p")
  (delete-horizontal-space t)
  (unless arg
    (setq arg 1))
  (let ((electric-indent-mode nil))
    (dotimes (_ arg)
      (newline nil t)
      (indent-according-to-mode))))

(defun reindent-then-newline-and-indent ()
  "Reindent current line, insert newline, then indent the new line.
Indentation of both lines is done according to the current major mode,
which means calling the current value of `indent-line-function'.
In programming language modes, this is the same as TAB.
In some text modes, where TAB inserts a tab, this indents to the
column specified by the function `current-left-margin'."
  (interactive "*")
  ;; Use a marker because the call to 'newline' below could insert some
  ;; text, e.g., if 'abbrev-mode' is turned on.
  (let ((pos (point-marker))
        (electric-indent-mode nil))
    ;; Be careful to insert the newline before indenting the line.
    ;; Otherwise, the indentation might be wrong.
    (newline)
    (save-excursion
      (goto-char pos)
      ;; We are at EOL before the call to indent-according-to-mode, and
      ;; after it we usually are as well, but not always.  We tried to
      ;; address it with `save-excursion' but that uses a normal marker
      ;; whereas we need `move after insertion', so we do the save/restore
      ;; by hand.
      (setq pos (copy-marker pos t))
      (indent-according-to-mode)
      (goto-char pos)
      ;; Remove the trailing white-space after indentation because
      ;; indentation may introduce the whitespace.
      (delete-horizontal-space t))
    (indent-according-to-mode)))

(defun forward-to-indentation (&optional arg)
  "Move forward ARG lines and position at first nonblank character."
  (interactive "^p")
  (forward-line (or arg 1))
  (skip-chars-forward " \t"))

(defun backward-to-indentation (&optional arg)
  "Move backward ARG lines and position at first nonblank character."
  (interactive "^p")
  (forward-line (- (or arg 1)))
  (skip-chars-forward " \t"))

(defun delete-all-space (&optional backward-only)
  "Delete all spaces, tabs, and newlines around point.
If BACKWARD-ONLY is non-nil, delete them only before point."
  (interactive "*P")
  (delete-space--internal " \t\r\n" backward-only))

(defun delete-space--internal (chars backward-only)
  "Delete CHARS around point.
If BACKWARD-ONLY is non-nil, delete them only before point."
  (let ((orig-pos (point)))
    (delete-region
     (if backward-only
         orig-pos
       (progn
         (skip-chars-forward chars)
         (constrain-to-field nil orig-pos t)))
     (progn
       (skip-chars-backward chars)
       (constrain-to-field nil orig-pos)))))

(defvar cycle-spacing--context nil
  "Stored context used in consecutive calls to `cycle-spacing' command.
The value is a property list with the following elements:
- `:orig-pos'    The original position of point when starting the
                 sequence.
- `:whitespace-string' All whitespace characters around point
                       including newlines.
- `:n'            The prefix arg given to the initial invocation
                  which is reused for all actions in this cycle.
- `:last-action'  The last action performed in the cycle.")

(defcustom cycle-spacing-actions
  '( just-one-space
     delete-all-space
     restore)
  "List of actions cycled through by `cycle-spacing'.
Supported values are:
- `just-one-space'      Delete all but N (prefix arg) spaces.
                        See that command's docstring for details.
- `delete-space-after'  Delete spaces after point keeping only N.
- `delete-space-before' Delete spaces before point keeping only N.
- `delete-all-space'    Delete all spaces around point.
- `restore'             Restore the original spacing.

All actions make use of the prefix arg given to `cycle-spacing'
in the initial invocation, i.e., `just-one-space' keeps this
amount of spaces deleting surplus ones.  `just-one-space' and all
other actions have the contract that a positive prefix arg (or
zero) only deletes tabs and spaces whereas a negative prefix arg
also deletes newlines.

The `delete-space-before' and `delete-space-after' actions handle
the prefix arg \\[negative-argument] without a number provided
specially: all spaces before/after point are deleted (as if N was
0) including newlines (as if N was negative).

In addition to the predefined actions listed above, any function
which accepts one argument is allowed.  It receives the raw
prefix arg of this cycle.

In addition, an action may take the form (ACTION ARG) where
ACTION is one of the predefined actions (except for `restore')
and ARG is either
- an integer with the meaning that ACTION should always use this
  fixed integer instead of the actual prefix arg or
- the symbol `inverted-arg' with the meaning that ACTION should
  be performed with the inverted actual prefix arg.
- the symbol `-' with the meaning that ACTION should include
  newlines but it's up to the ACTION to decide how to interpret
  it as a number, e.g., `delete-space-before' and
  `delete-space-after' treat it like 0 whereas `just-one-space'
  treats it like -1 as is usual."
  :group 'editing-basics
  :type (let ((actions
               '((const :tag "Just N (prefix arg) spaces" just-one-space)
                 (const :tag "Delete spaces after point" delete-space-after)
                 (const :tag "Delete spaces before point" delete-space-before)
                 (const :tag "Delete all spaces around point" delete-all-space)
                 (function :tag "Function receiving a numeric arg"))))
          `(repeat
            (choice
             ,@actions
             (list :tag "Action with modified arg"
                   (choice ,@actions)
                   (choice (const :tag "Inverted prefix arg" inverted-arg)
                           (integer :tag "Fixed numeric arg")
                           (const :tag "Negative arg" -)))
             (const :tag "Restore the original spacing" restore))))
  :version "29.1")

(defun cycle-spacing (&optional n)
  "Manipulate whitespace around point in a smart way.
Repeated calls perform the actions in `cycle-spacing-actions' one
after the other, wrapping around after the last one.

All actions are amendable using a prefix arg N.  In general, a
zero or positive prefix arg allows only for deletion of tabs and
spaces whereas a negative prefix arg also allows for deleting
newlines.

The prefix arg given at the first invocation starting a cycle is
provided to all following actions, i.e.,
    \\[negative-argument] \\[cycle-spacing] \\[cycle-spacing] \\[cycle-spacing]
is equivalent to
    \\[negative-argument] \\[cycle-spacing] \\[negative-argument] \\[cycle-spacing] \\[negative-argument] \\[cycle-spacing].

A new sequence can be started by providing a different prefix arg
than provided at the initial invocation (except for 1), or by
doing any other command before the next \\[cycle-spacing]."
  (interactive "*P")
  ;; Initialize `cycle-spacing--context' if needed.
  (when (or (not (equal last-command this-command))
            (not cycle-spacing--context)
            ;; With M-5 M-SPC M-SPC... we pass the prefix arg 5 to
            ;; each action and only start a new cycle when a different
            ;; prefix arg is given and which is not the default value
            ;; 1.
            (and n (not (equal (plist-get cycle-spacing--context :n)
                               n))))
    (let ((orig-pos (point))
          (skip-characters " \t\n\r"))
      (save-excursion
        (skip-chars-backward skip-characters)
        (constrain-to-field nil orig-pos)
        (let ((start (point))
              (end   (progn
                       (skip-chars-forward skip-characters)
                       (constrain-to-field nil orig-pos t))))
          (setq cycle-spacing--context  ;; Save for later.
                (list :orig-pos orig-pos
                      :whitespace-string (buffer-substring start end)
                      :n n
                      :last-action nil))))))

  ;; Cycle through the actions in `cycle-spacing-actions'.
  (when cycle-spacing--context
    (cl-labels ((next-action ()
                  (let* ((l cycle-spacing-actions)
                         (elt (plist-get cycle-spacing--context
                                         :last-action)))
                    (if (null elt)
                        (car cycle-spacing-actions)
                      (catch 'found
                        (while l
                          (cond
                           ((null (cdr l))
                            (throw 'found
                                   (when (eq elt (car l))
                                     (car cycle-spacing-actions))))
                           ((and (eq elt (car l))
                                 (cdr l))
                            (throw 'found (cadr l)))
                           (t (setq l (cdr l)))))))))
                (skip-chars (chars max-dist direction)
                  (if (eq direction 'forward)
                      (skip-chars-forward
                       chars
                       (and max-dist (+ (point) max-dist)))
                    (skip-chars-backward
                     chars
                     (and max-dist (- (point) max-dist)))))
                (delete-space (n include-newlines direction)
                  (let ((orig-point (point))
                        (chars (if include-newlines
                                   " \t\r\n"
                                 " \t")))
                    (when (or (zerop n)
                              (= n (abs (skip-chars chars n direction))))
                      (let ((start (point))
                            (end (progn
                                   (skip-chars chars nil direction)
                                   (point))))
                        (unless (= start end)
                          (delete-region start end))
                        (goto-char (if (eq direction 'forward)
                                       orig-point
                                     (+ n end)))))))
                (restore ()
                  (delete-all-space)
                  (insert (plist-get cycle-spacing--context
                                     :whitespace-string))
                  (goto-char (plist-get cycle-spacing--context
                                        :orig-pos))))
      (let ((action (next-action)))
        (atomic-change-group
          (restore)
          (unless (eq action 'restore)
            ;; action can be some-action or (some-action <arg>) where
            ;; arg is either an integer, the arg to be always used for
            ;; this action or - to use the inverted context n for this
            ;; action.
            (let* ((actual-action (if (listp action)
                                      (car action)
                                    action))
                   (arg (when (listp action)
                          (nth 1 action)))
                   (context-n (plist-get cycle-spacing--context :n))
                   (actual-n (cond
                              ((integerp arg) arg)
                              ((eq 'inverted-arg arg)
                               (* -1 (prefix-numeric-value context-n)))
                              ((eq '- arg) '-)
                              (t context-n)))
                   (numeric-n (prefix-numeric-value actual-n))
                   (include-newlines (or (eq actual-n '-)
                                         (and (integerp actual-n)
                                              (< actual-n 0)))))
              (cond
               ((eq actual-action 'just-one-space)
                (just-one-space numeric-n))
               ((eq actual-action 'delete-space-after)
                (delete-space (if (eq actual-n '-) 0 (abs numeric-n))
                              include-newlines 'forward))
               ((eq actual-action 'delete-space-before)
                (delete-space (if (eq actual-n '-) 0 (abs numeric-n))
                              include-newlines 'backward))
               ((eq actual-action 'delete-all-space)
                (if include-newlines
                    (delete-all-space)
                  (delete-horizontal-space)))
               ((functionp actual-action)
                (funcall actual-action actual-n))
               (t
                (error "Don't know how to handle action %S" action)))))
          (setf (plist-get cycle-spacing--context :last-action)
                action))))))

(defun beginning-of-buffer (&optional arg)
  "Move point to the beginning of the buffer.
With numeric arg N, put point N/10 of the way from the beginning.
If the buffer is narrowed, this command uses the beginning of the
accessible part of the buffer.

Push mark at previous position, unless either a \\[universal-argument] prefix
is supplied, or Transient Mark mode is enabled and the mark is active."
  (declare (interactive-only "use `(goto-char (point-min))' instead."))
  (interactive "^P")
  (or (consp arg)
      (region-active-p)
      (push-mark))
  (let ((size (- (point-max) (point-min))))
    (goto-char (if (and arg (not (consp arg)))
		   (+ (point-min) 1
		      (/ (* size (prefix-numeric-value arg)) 10))
		 (point-min))))
  (if (and arg (not (consp arg))) (forward-line 1)))

(defcustom delete-active-region t
  "Whether single-char deletion commands delete an active region.
This has an effect only if Transient Mark mode is enabled, and
affects `delete-forward-char' and `delete-backward-char', though
not `delete-char'.

If the value is the symbol `kill', the active region is killed
instead of deleted."
  :type '(choice (const :tag "Delete active region" t)
                 (const :tag "Kill active region" kill)
                 (const :tag "Do ordinary deletion" nil))
  :group 'killing
  :version "24.1")

(defun delete-backward-char (n &optional killflag)
  "Delete the previous N characters (following if N is negative).
If Transient Mark mode is enabled, the mark is active, and N is 1,
delete the text in the region and deactivate the mark instead.
To disable this, set option `delete-active-region' to nil.

Optional second arg KILLFLAG, if non-nil, means to kill (save in
kill ring) instead of delete.  If called interactively, a numeric
prefix argument specifies N, and KILLFLAG is also set if a prefix
argument is used.

When killing, the killed text is filtered by
`filter-buffer-substring' before it is saved in the kill ring, so
the actual saved text might be different from what was killed.

In Overwrite mode, single character backward deletion may replace
tabs with spaces so as to back over columns, unless point is at
the end of the line."
  (declare (interactive-only delete-char))
  (interactive "p\nP")
  (unless (integerp n)
    (signal 'wrong-type-argument (list 'integerp n)))
  (cond ((and (use-region-p)
	      delete-active-region
	      (= n 1))
	 ;; If a region is active, kill or delete it.
	 (if (eq delete-active-region 'kill)
	     (kill-region (region-beginning) (region-end) 'region)
           (funcall region-extract-function 'delete-only)))
	;; In Overwrite mode, maybe untabify while deleting
	((null (or (null overwrite-mode)
		   (<= n 0)
		   (memq (char-before) '(?\t ?\n))
		   (eobp)
		   (eq (char-after) ?\n)))
	 (let ((ocol (current-column)))
           (delete-char (- n) killflag)
	   (save-excursion
	     (insert-char ?\s (- ocol (current-column)) nil))))
	;; Otherwise, do simple deletion.
	(t (delete-char (- n) killflag))))

(defun mark-whole-buffer ()
  "Put point at beginning and mark at end of buffer.
Also push mark at point before pushing mark at end of buffer.
If narrowing is in effect, uses only the accessible part of the buffer.
You probably should not use this function in Lisp programs;
it is usually a mistake for a Lisp function to use any subroutine
that uses or sets the mark."
  (declare (interactive-only t))
  (interactive)
  (push-mark)
  (push-mark (point-max) nil t)
  ;; This is really `point-min' in most cases, but if we're in the
  ;; minibuffer, this is at the end of the prompt.
  (goto-char (minibuffer-prompt-end)))

(defcustom goto-line-history-local nil
  "If this option is nil, `goto-line-history' is shared between all buffers.
If it is non-nil, each buffer has its own value of this history list.

Note that on changing from non-nil to nil, the former contents of
`goto-line-history' for each buffer are discarded on use of
`goto-line' in that buffer."
  :group 'editing
  :type 'boolean
  :safe #'booleanp
  :version "28.1")

(defvar goto-line-history nil
  "History of values entered with `goto-line'.")

(defun goto-line-read-args (&optional relative)
  "Read arguments for `goto-line' related commands."
  (if (and current-prefix-arg (not (consp current-prefix-arg)))
      (list (prefix-numeric-value current-prefix-arg))
    ;; Look for a default, a number in the buffer at point.
    (let* ((number (number-at-point))
           (default (and (natnump number) number))
           ;; Decide if we're switching buffers.
           (buffer
            (if (consp current-prefix-arg)
                (other-buffer (current-buffer) t)))
           (buffer-prompt
            (if buffer
                (concat " in " (buffer-name buffer))
              "")))
      ;; Has the buffer locality of `goto-line-history' changed?
      (cond ((and goto-line-history-local (not (local-variable-p 'goto-line-history)))
             (make-local-variable 'goto-line-history))
            ((and (not goto-line-history-local) (local-variable-p 'goto-line-history))
             (kill-local-variable 'goto-line-history)))
      ;; Read the argument, offering that number (if any) as default.
      (list (read-number (format "Goto%s line%s: "
                                 (if (buffer-narrowed-p)
                                     (if relative " relative" " absolute")
                                   "")
                                 buffer-prompt)
                         (list default (if (or relative (not (buffer-narrowed-p)))
                                           (line-number-at-pos)
                                         (save-restriction
                                           (widen)
                                           (line-number-at-pos))))
                         'goto-line-history)
            buffer))))

(defun goto-line (line &optional buffer relative interactive)
  "Go to LINE, counting from line 1 at beginning of buffer.
If called interactively, a numeric prefix argument specifies
LINE; without a numeric prefix argument, read LINE from the
minibuffer.

If called interactively with \\[universal-argument], switch to the
most recently selected other buffer and move to line LINE there.

If optional argument RELATIVE is non-nil, counting starts at the beginning
of the accessible portion of the (potentially narrowed) buffer.

If the variable `widen-automatically' is non-nil, cancel narrowing and
leave all lines accessible.  If `widen-automatically' is nil, just move
point to the edge of visible portion and don't change the buffer bounds.

Prior to moving point, this function sets the mark (without
activating it), unless Transient Mark mode is enabled and the
mark is already active.

A non-nil INTERACTIVE argument pushes the mark and switches the buffer
if optional argument BUFFER is non-nil.

This function is usually the wrong thing to use in a Lisp program.
What you probably want instead is something like:
  (goto-char (point-min))
  (forward-line (1- N))
If at all possible, an even better solution is to use char counts
rather than line counts."
  (interactive (append (goto-line-read-args) '(nil t)))
  ;; Switch to the desired buffer, one way or another.
  (when interactive
    (when buffer
      (let ((window (get-buffer-window buffer)))
	(if window (select-window window)
	  (switch-to-buffer-other-window buffer))))
    ;; Leave mark at previous position
    (or (region-active-p) (push-mark)))
  ;; Move to the specified line number in that buffer.
  (let ((pos (save-restriction
               (unless relative (widen))
               (goto-char (point-min))
               (if (eq selective-display t)
                   (re-search-forward "[\n\C-m]" nil 'end (1- line))
                 (forward-line (1- line)))
               (point))))
    (when (and (not relative)
               (buffer-narrowed-p)
               widen-automatically
               ;; Position is outside narrowed part of buffer
               (or (> (point-min) pos) (> pos (point-max))))
      (widen))
    (goto-char pos)))

(defun goto-line-relative (line &optional buffer interactive)
  "Go to LINE, counting from line at (point-min).
The line number is relative to the accessible portion of the narrowed
buffer.  The arguments BUFFER and INTERACTIVE are the same as in the
function `goto-line'."
  (interactive (append (goto-line-read-args t) t))
  (goto-line line buffer t interactive))

(defun count-words-region (start end &optional arg)
  "Count the number of words in the region.
If called interactively, print a message reporting the number of
lines, words, and characters in the region (whether or not the
region is active); with prefix ARG, report for the entire buffer
rather than the region.

If called from Lisp, return the number of words between positions
START and END."
  (interactive (if current-prefix-arg
		   (list nil nil current-prefix-arg)
		 (list (region-beginning) (region-end) nil)))
  (cond ((not (called-interactively-p 'any))
	 (count-words start end))
	(arg
	 (message "%s" (count-words--buffer-format)))
	(t
	 (message "%s" (count-words--format "Region" start end)))))

(defun count-words--buffer-format ()
  (count-words--format
   (if (buffer-narrowed-p) "Narrowed part of buffer" "Buffer")
   (point-min) (point-max)))

(defun count-words--format (str start end)
  (let ((lines (count-lines start end))
	(sentences (count-sentences start end))
	(words (count-words start end))
	(chars (- end start)))
    (format "%s has %d line%s, %d sentence%s, %d word%s, and %d character%s"
	     str
	     lines (if (= lines 1) "" "s")
	     sentences (if (= sentences 1) "" "s")
	     words (if (= words 1) "" "s")
	     chars (if (= chars 1) "" "s"))))

(defun what-line ()
  "Print the current buffer line number and narrowed line number of point."
  (interactive)
  (let ((start (point-min))
	(n (line-number-at-pos)))
    (if (= start 1)
	(message "Line %d" n)
      (save-excursion
	(save-restriction
	  (widen)
	  (message "line %d (narrowed line %d)"
		   (+ n (line-number-at-pos start) -1) n))))))

(defun append-next-kill (&optional interactive)
  "Cause following command, if it kills, to add to previous kill.
If the next command kills forward from point, the kill is
appended to the previous killed text.  If the command kills
backward, the kill is prepended.  Kill commands that act on the
region, such as `kill-region', are regarded as killing forward if
point is after mark, and killing backward if point is before
mark.

If the next command is not a kill command, `append-next-kill' has
no effect.

The argument is used for internal purposes; do not supply one."
  (interactive "p")
  ;; We don't use (interactive-p), since that breaks kbd macros.
  (if interactive
      (progn
	(setq this-command 'kill-region)
	(message "If the next command is a kill, it will append"))
    (setq last-command 'kill-region)))

(defun kill-forward-chars (arg)
  (if (listp arg) (setq arg (car arg)))
  (if (eq arg '-) (setq arg -1))
  (kill-region (point) (+ (point) arg)))

(defun kill-backward-chars (arg)
  (if (listp arg) (setq arg (car arg)))
  (if (eq arg '-) (setq arg -1))
  (kill-region (point) (- (point) arg)))

(defcustom backward-delete-char-untabify-method 'untabify
  "The method for untabifying when deleting backward.
Can be `untabify' -- turn a tab to many spaces, then delete one space;
       `hungry' -- delete all whitespace, both tabs and spaces;
       `all' -- delete all whitespace, including tabs, spaces and newlines;
       nil -- just delete one character."
  :type '(choice (const untabify) (const hungry) (const all) (const nil))
  :version "20.3"
  :group 'killing)

(defun backward-delete-char-untabify (arg &optional killp)
  "Delete characters backward, changing tabs into spaces.
The exact behavior depends on `backward-delete-char-untabify-method'.

Delete ARG chars, and kill (save in kill ring) if KILLP is non-nil.

If Transient Mark mode is enabled, the mark is active, and ARG is 1,
delete the text in the region and deactivate the mark instead.
To disable this, set option `delete-active-region' to nil.

Interactively, ARG is the prefix arg (default 1)
and KILLP is t if a prefix arg was specified."
  (interactive "*p\nP")
  (if (and (use-region-p)
           delete-active-region
           (= arg 1))
      ;; Delete the text of the region and deactivate the mark, in the
      ;; same way that `delete-backward-char' does.
      (with-no-warnings (delete-backward-char 1 killp))
    (when (eq backward-delete-char-untabify-method 'untabify)
      (let ((count arg))
        (save-excursion
	  (while (and (> count 0) (not (bobp)))
	    (if (= (preceding-char) ?\t)
	        (let ((col (current-column)))
		  (forward-char -1)
		  (setq col (- col (current-column)))
		  (insert-char ?\s col)
		  (delete-char 1)))
	    (forward-char -1)
	    (setq count (1- count))))))
    (let* ((skip (cond ((eq backward-delete-char-untabify-method 'hungry) " \t")
                       ((eq backward-delete-char-untabify-method 'all)
                        " \t\n\r")))
           (n (if skip
                  (let* ((oldpt (point))
                         (wh (- oldpt (save-excursion
                                        (skip-chars-backward skip)
                                        (constrain-to-field nil oldpt)))))
                    (+ arg (if (zerop wh) 0 (1- wh))))
                arg)))
      ;; Avoid warning about delete-backward-char
      (with-no-warnings (delete-backward-char n killp)))))

(defun insert-buffer (buffer)
  "Insert after point the contents of BUFFER.
Puts mark after the inserted text.
BUFFER may be a buffer or a buffer name."
  (declare (interactive-only insert-buffer-substring))
  (interactive
   (list
    (progn
      (barf-if-buffer-read-only)
      (read-buffer "Insert buffer: "
		   (if (eq (selected-window) (next-window))
		       (other-buffer (current-buffer))
		     (window-buffer (next-window)))
		   t))))
  (push-mark
   (save-excursion
     (insert-buffer-substring (get-buffer buffer))
     (point)))
  nil)

(defun prepend-to-buffer (buffer start end)
  "Prepend to specified BUFFER the text of the region.
The text is inserted into that buffer after its point.
BUFFER can be a buffer or the name of a buffer; this
function will create BUFFER if it doesn't already exist.

When calling from a program, give three arguments:
BUFFER (or buffer name), START and END.
START and END specify the portion of the current buffer to be copied."
  (interactive "BPrepend to buffer: \nr")
  (let ((oldbuf (current-buffer)))
    (with-current-buffer (get-buffer-create buffer)
      (barf-if-buffer-read-only)
      (save-excursion
	(insert-buffer-substring oldbuf start end)))))

(defun copy-to-buffer (buffer start end)
  "Copy to specified BUFFER the text of the region.
The text is inserted into that buffer, replacing existing text there.
BUFFER can be a buffer or the name of a buffer; this
function will create BUFFER if it doesn't already exist.

When calling from a program, give three arguments:
BUFFER (or buffer name), START and END.
START and END specify the portion of the current buffer to be copied."
  (interactive "BCopy to buffer: \nr")
  (let ((oldbuf (current-buffer)))
    (with-current-buffer (get-buffer-create buffer)
      (barf-if-buffer-read-only)
      (erase-buffer)
      (save-excursion
	(insert-buffer-substring oldbuf start end)))))

(defvar widen-automatically t
  "Non-nil means it is ok for commands to call `widen' when they want to.
Some commands will do this in order to go to positions outside
the current accessible part of the buffer.

If `widen-automatically' is nil, these commands will do something else
as a fallback, and won't change the buffer bounds.")

(defun transpose-sexps-default-function (arg)
  "Default method to locate a pair of points for `transpose-sexps'."
  ;; Here we should try to simulate the behavior of
  ;; (cons (progn (forward-sexp x) (point))
  ;;       (progn (forward-sexp (- x)) (point)))
  ;; Except that we don't want to rely on the second forward-sexp
  ;; putting us back to where we want to be, since forward-sexp-function
  ;; might do funny things like infix-precedence.
  (if (if (> arg 0)
	  (looking-at "\\sw\\|\\s_")
	(and (not (bobp))
	     (save-excursion
               (forward-char -1)
               (looking-at "\\sw\\|\\s_"))))
      ;; Jumping over a symbol.  We might be inside it, mind you.
      (progn (funcall (if (> arg 0)
			  #'skip-syntax-backward #'skip-syntax-forward)
		      "w_")
	     (cons (save-excursion (forward-sexp arg) (point)) (point)))
    ;; Otherwise, we're between sexps.  Take a step back before jumping
    ;; to make sure we'll obey the same precedence no matter which
    ;; direction we're going.
    (funcall (if (> arg 0) #'skip-syntax-backward #'skip-syntax-forward)
             " .")
    (cons (save-excursion (forward-sexp arg) (point))
	  (progn (while (or (forward-comment (if (> arg 0) 1 -1))
			    (not (zerop (funcall (if (> arg 0)
						     #'skip-syntax-forward
						   #'skip-syntax-backward)
						 ".")))))
		 (point)))))

(defvar transpose-sexps-function #'transpose-sexps-default-function
  "If non-nil, `transpose-sexps' delegates to this function.

This function takes one argument ARG, a number.  Its expected
return value is a position pair, which is a cons (BEG . END),
where BEG and END are buffer positions.")

(defun transpose-sexps (arg &optional interactive)
  "Like \\[transpose-chars] (`transpose-chars'), but applies to sexps.
Unlike `transpose-words', point must be between the two sexps and not
in the middle of a sexp to be transposed.
With non-zero prefix arg ARG, effect is to take the sexp before point
and drag it forward past ARG other sexps (backward if ARG is negative).
If ARG is zero, the sexps ending at or after point and at or after mark
are interchanged.
If INTERACTIVE is non-nil, as it is interactively,
report errors as appropriate for this kind of usage."
  (interactive "*p\nd")
  (if interactive
      (condition-case nil
          (transpose-sexps arg nil)
        (scan-error (user-error "Not between two complete sexps")))
    (transpose-subr transpose-sexps-function arg 'special)))

(defun forward-unix-word (n &optional delim)
  "Move forward N Unix-words.
A Unix-word is whitespace-delimited.
A negative N means go backwards to the beginning of Unix-words.

Unix-words differ from Emacs words in that they are always delimited by
whitespace, regardless of the buffer's syntax table.  This function
emulates how C-w at the Unix terminal or shell identifies words.

Optional argument DELIM specifies what characters are considered
whitespace.  It is a string as might be passed to `skip-chars-forward'.
The default is \"\\s\\f\\n\\r\\t\\v\".  Do not prefix a `^' character."
  (when (string-prefix-p "^" delim)
    (error "DELIM argument must not begin with `^'"))
  (unless (zerop n)
    ;; We do skip over newlines by default because `backward-word' does.
    (let* ((delim (or delim "\s\f\n\r\t\v"))
           (ndelim (format "^%s" delim))
           (start (point))
           (fun (if (> n 0)
                    #'skip-chars-forward
                  #'skip-chars-backward)))
      (dotimes (_ (abs n))
        (funcall fun delim)
        (funcall fun ndelim))
      (constrain-to-field nil start))))

(defun unix-word-rubout (arg)
  "Kill ARG Unix-words backwards.
A Unix-word is whitespace-delimited.
Interactively, ARG is the numeric prefix argument, defaulting to 1.
A negative ARG means to kill forwards.

Unix-words differ from Emacs words in that they are always delimited by
whitespace, regardless of the buffer's syntax table.
Thus, this command emulates C-w at the Unix terminal or shell.
See also this command's namesake in Info node
`(readline)Commands For Killing'."
  (interactive "^p")
  (let ((start (point)))
    (forward-unix-word (- arg))
    (kill-region start (point))))

(defun unix-filename-rubout (arg)
  "Kill ARG Unix-words backwards, also treating slashes as word delimiters.
A Unix-word is whitespace-delimited.
Interactively, ARG is the numeric prefix argument, defaulting to 1.
A negative ARG means to kill forwards.

This is like `unix-word-rubout' (which see), but `/' and `\\' are also
treated as delimiting words.  See this command's namesake in Info node
`(readline)Commands For Killing'."
  (interactive "^p")
  (let ((start (point)))
    (forward-unix-word (- arg) "\\\\/\s\f\n\r\t\v")
    (kill-region start (point))))

(defcustom auto-fill-inhibit-regexp nil
  "Regexp to match lines that should not be auto-filled."
  :type '(choice (const :tag "None" nil)
		 regexp)
  :group 'fill)

(defun do-auto-fill ()
  "The default value for `normal-auto-fill-function'.
This is the default auto-fill function, some major modes use a different one.
Returns t if it really did any work."
  (let (fc justify give-up
	   (fill-prefix fill-prefix))
    (if (or (not (setq justify (current-justification)))
	    (null (setq fc (current-fill-column)))
	    (and (eq justify 'left)
		 (<= (current-column) fc))
	    (and auto-fill-inhibit-regexp
		 (save-excursion (beginning-of-line)
				 (looking-at auto-fill-inhibit-regexp))))
	nil ;; Auto-filling not required
      (if (memq justify '(full center right))
	  (save-excursion (unjustify-current-line)))

      ;; Choose a fill-prefix automatically.
      (when (and adaptive-fill-mode
		 (or (null fill-prefix) (string= fill-prefix "")))
	(let ((prefix
	       (fill-context-prefix
		(save-excursion (fill-forward-paragraph -1) (point))
		(save-excursion (fill-forward-paragraph 1) (point)))))
	  (and prefix (not (equal prefix ""))
	       ;; Use auto-indentation rather than a guessed empty prefix.
	       (not (and fill-indent-according-to-mode
			 (string-match "\\`[ \t]*\\'" prefix)))
	       (setq fill-prefix prefix))))

      (while (and (not give-up) (> (current-column) fc))
        ;; Determine where to split the line.
        (let ((fill-point
               (save-excursion
                 (beginning-of-line)
                 ;; Don't split earlier in the line than the length of the
                 ;; fill prefix, since the resulting line would be longer.
                 (when fill-prefix
                   (move-to-column (string-width fill-prefix)))
                 (let ((after-prefix (point)))
                    (move-to-column (1+ fc))
                    (fill-move-to-break-point after-prefix)
                    (point)))))

	  ;; See whether the place we found is any good.
	  (if (save-excursion
		(goto-char fill-point)
		(or (bolp)
		    ;; There is no use breaking at end of line.
		    (save-excursion (skip-chars-forward " ") (eolp))
		    ;; Don't split right after a comment starter
		    ;; since we would just make another comment starter.
		    (and comment-start-skip
			 (let ((limit (point)))
			   (beginning-of-line)
			   (and (re-search-forward comment-start-skip
						   limit t)
				(eq (point) limit))))))
	      ;; No good place to break => stop trying.
	      (setq give-up t)
	    ;; Ok, we have a useful place to break the line.  Do it.
	    (let ((prev-column (current-column)))
	      ;; If point is at the fill-point, do not `save-excursion'.
	      ;; Otherwise, if a comment prefix or fill-prefix is inserted,
	      ;; point will end up before it rather than after it.
	      (if (save-excursion
		    (skip-chars-backward " \t")
		    (= (point) fill-point))
		  (default-indent-new-line t)
		(save-excursion
		  (goto-char fill-point)
		  (default-indent-new-line t)))
	      ;; Now do justification, if required
	      (if (not (eq justify 'left))
		  (save-excursion
		    (end-of-line 0)
		    (justify-current-line justify nil t)))
	      ;; If making the new line didn't reduce the hpos of
	      ;; the end of the line, then give up now;
	      ;; trying again will not help.
	      (if (>= (current-column) prev-column)
		  (setq give-up t))))))
      ;; Justify last line.
      (justify-current-line justify t t)
      t)))

(defvar comment-line-break-function 'comment-indent-new-line
  "Mode-specific function that line breaks and continues a comment.
This function is called during auto-filling when a comment syntax
is defined.
The function should take a single optional argument, which is a flag
indicating whether it should use soft newlines.")

(defun default-indent-new-line (&optional soft force)
  "Break line at point and indent.
If a comment syntax is defined, call `comment-line-break-function'.

The inserted newline is marked hard if variable `use-hard-newlines' is true,
unless optional argument SOFT is non-nil."
  (interactive (list nil t))
  (if comment-start
      ;; Force breaking the line when called interactively.
      (if force
          (let ((comment-auto-fill-only-comments nil))
            (funcall comment-line-break-function soft))
        (funcall comment-line-break-function soft))
    ;; Insert the newline before removing empty space so that markers
    ;; get preserved better.
    (if soft (insert-and-inherit ?\n) (newline 1))
    (save-excursion (forward-char -1) (delete-horizontal-space))
    (delete-horizontal-space)

    (if (and fill-prefix (not adaptive-fill-mode))
	;; Blindly trust a non-adaptive fill-prefix.
	(progn
	  (indent-to-left-margin)
	  (insert-before-markers-and-inherit fill-prefix))

      (cond
       ;; If there's an adaptive prefix, use it unless we're inside
       ;; a comment and the prefix is not a comment starter.
       (fill-prefix
	(indent-to-left-margin)
	(insert-and-inherit fill-prefix))
       ;; If we're not inside a comment, just try to indent.
       (t (indent-according-to-mode))))))

(defvar inhibit-auto-fill nil
  "Non-nil means to do as if `auto-fill-mode' was disabled.")

(defun internal-auto-fill ()
  "The function called by `self-insert-command' to perform auto-filling."
  (unless (or inhibit-auto-fill
              (and comment-start
                   comment-auto-fill-only-comments
                   (not (nth 4 (syntax-ppss)))))
    (funcall auto-fill-function)))

(defvar normal-auto-fill-function 'do-auto-fill
  "The function to use for `auto-fill-function' if Auto Fill mode is turned on.
Some major modes set this.")

(define-minor-mode auto-fill-mode
  "Toggle automatic line breaking (Auto Fill mode).

When Auto Fill mode is enabled, inserting a space at a column
beyond `current-fill-column' automatically breaks the line at a
previous space.

When `auto-fill-mode' is on, the `auto-fill-function' variable is
non-nil.

The value of `normal-auto-fill-function' specifies the function to use
for `auto-fill-function' when turning Auto Fill mode on."
  :variable (auto-fill-function
             . (lambda (v) (setq auto-fill-function
                            (if v normal-auto-fill-function)))))

(defun turn-on-auto-fill ()
  "Unconditionally turn on Auto Fill mode."
  (auto-fill-mode 1))

(defun turn-off-auto-fill ()
  "Unconditionally turn off Auto Fill mode."
  (auto-fill-mode -1))

(defun set-fill-column (arg)
  "Set `fill-column' to specified argument.
Use \\[universal-argument] followed by a number to specify a column.
Just \\[universal-argument] as argument means to use the current column."
  (interactive
   (list (or current-prefix-arg
             ;; We used to use current-column silently, but C-x f is too easily
             ;; typed as a typo for C-x C-f, so we turned it into an error and
             ;; now an interactive prompt.
             (read-number (format "Change fill-column from %s to: " fill-column)
                          (current-column)))))
  (if (consp arg)
      (setq arg (current-column)))
  (if (not (integerp arg))
      ;; Disallow missing argument; it's probably a typo for C-x C-f.
      (error "set-fill-column requires an explicit argument")
    (message "Fill column set to %d (was %d)" arg fill-column)
    (setq fill-column arg)))

(defcustom blink-matching-delay 1
  "Time in seconds to delay after showing a matching paren."
  :type 'number
  :group 'paren-blinking)

;;;; emacs-lisp/lisp.el

(defcustom defun-prompt-regexp nil
  "If non-nil, a regexp to ignore before a defun.
This is only necessary if the opening paren or brace is not in column 0.
See function `beginning-of-defun'."
  :type '(choice (const nil)
		 regexp)
  :local t
  :group 'lisp)

(defcustom parens-require-spaces t
  "If non-nil, add whitespace as needed when inserting parentheses.
This affects `insert-parentheses' and `insert-pair'."
  :type 'boolean
  :group 'lisp)

(defun forward-list-default-function (&optional arg)
  "Default function for `forward-list-function'."
  (goto-char (or (scan-lists (point) arg 0) (buffer-end arg))))

(defvar forward-list-function #'forward-list-default-function
  "If non-nil, `forward-list' delegates to this function.
Should take the same arguments and behave similarly to `forward-list'.")

(defun down-list-default-function (&optional arg)
  "Default function for `down-list-function'."
  (let ((inc (if (> arg 0) 1 -1)))
    (while (/= arg 0)
      (goto-char (or (scan-lists (point) inc -1) (buffer-end arg)))
      (setq arg (- arg inc)))))

(defvar down-list-function nil
  "If non-nil, `down-list' delegates to this function.
Should take the same arguments and behave similarly to `down-list'.")

(defvar up-list-function nil
  "If non-nil, `up-list' delegates to this function.
Should take the same arguments and behave similarly to `up-list'.")

(defun up-list-default-function (&optional arg escape-strings no-syntax-crossing)
  "Default function for `up-list-function'."
  (or arg (setq arg 1))
  (let ((inc (if (> arg 0) 1 -1))
        (pos nil))
    (while (/= arg 0)
      (condition-case err
          (save-restriction
            ;; If we've been asked not to cross string boundaries
            ;; and we're inside a string, narrow to that string so
            ;; that scan-lists doesn't find a match in a different
            ;; string.
            (when no-syntax-crossing
              (let* ((syntax (syntax-ppss))
                     (string-comment-start (nth 8 syntax)))
                (when string-comment-start
                  (save-excursion
                    (goto-char string-comment-start)
                    (narrow-to-region
                     (point)
                     (if (nth 3 syntax) ; in string
                         (condition-case nil
                             (progn (forward-sexp) (point))
                           (scan-error (point-max)))
                       (forward-comment 1)
                       (point)))))))
            ;; FIXME: Comparing functions is a code smell.
            (if (memq forward-sexp-function
                      '(nil forward-sexp-default-function))
                (goto-char (or (scan-lists (point) inc 1)
                               (buffer-end arg)))
              (condition-case err
                  (while (progn (setq pos (point))
                                (forward-sexp inc)
                                (/= (point) pos)))
                (scan-error (goto-char (nth (if (> arg 0) 3 2) err))))
              (if (= (point) pos)
                  (signal 'scan-error
                          (list "Unbalanced parentheses" (point) (point))))))
        (scan-error
         (let ((syntax nil))
           (or
            ;; If we bumped up against the end of a list, see whether
            ;; we're inside a string: if so, just go to the beginning
            ;; or end of that string.
            (and escape-strings
                 (or syntax (setf syntax (syntax-ppss)))
                 (nth 3 syntax)
                 (goto-char (nth 8 syntax))
                 (progn (when (> inc 0)
                          (forward-sexp))
                        t))
            ;; If we narrowed to a comment above and failed to escape
            ;; it, the error might be our fault, not an indication
            ;; that we're out of syntax.  Try again from beginning or
            ;; end of the comment.
            (and no-syntax-crossing
                 (or syntax (setf syntax (syntax-ppss)))
                 (nth 4 syntax)
                 (goto-char (nth 8 syntax))
                 (or (< inc 0)
                     (forward-comment 1))
                 (setf arg (+ arg inc)))
            (if no-syntax-crossing
                ;; Assume called interactively; don't signal an error.
                (user-error "At top level")
              (signal err))))))
      (setq arg (- arg inc)))))

(defun kill-backward-up-list (&optional arg)
  "Kill the form containing the current sexp, leaving the sexp itself.
A prefix argument ARG causes the relevant number of surrounding
forms to be removed.
This command assumes point is not in a string or comment."
  (interactive "*p")
  (let ((current-sexp (thing-at-point 'sexp)))
    (if current-sexp
        (save-excursion
          (backward-up-list arg)
          (kill-sexp)
          (insert current-sexp))
      (user-error "Not at a sexp"))))

(defvar beginning-of-defun-function nil
  "If non-nil, function for `beginning-of-defun-raw' to call.
This is used to find the beginning of the defun instead of using the
normal recipe (see `beginning-of-defun').  Major modes can define this
if defining `defun-prompt-regexp' is not sufficient to handle the mode's
needs.

The function takes the same argument as `beginning-of-defun' and should
behave similarly, returning non-nil if it found the beginning of a defun.
Ideally it should move to a point right before an open-paren which encloses
the body of the defun.")

(defun beginning-of-defun (&optional arg)
  "Move backward to the beginning of a defun.
With ARG, do it that many times.  Negative ARG means move forward
to the ARGth following beginning of defun.

If search is successful, return t; point ends up at the beginning
of the line where the search succeeded.  Otherwise, return nil.

When `open-paren-in-column-0-is-defun-start' is non-nil, a defun
is assumed to start where there is a char with open-parenthesis
syntax at the beginning of a line.  If `defun-prompt-regexp' is
non-nil, then a string which matches that regexp may also precede
the open-parenthesis.  If `defun-prompt-regexp' and
`open-paren-in-column-0-is-defun-start' are both nil, this
function instead finds an open-paren at the outermost level.

If the variable `beginning-of-defun-function' is non-nil, its
value is called as a function, with argument ARG, to find the
defun's beginning.

Regardless of the values of `defun-prompt-regexp' and
`beginning-of-defun-function', point always moves to the
beginning of the line whenever the search is successful."
  (interactive "^p")
  (or (not (eq this-command 'beginning-of-defun))
      (eq last-command 'beginning-of-defun)
      (and transient-mark-mode mark-active)
      (push-mark))
  (and (beginning-of-defun-raw arg)
       (progn (beginning-of-line) t)))

(defun beginning-of-defun-raw (&optional arg)
  "Move point to the character that starts a defun.
This is identical to function `beginning-of-defun', except that point
does not move to the beginning of the line when `defun-prompt-regexp'
is non-nil.

If variable `beginning-of-defun-function' is non-nil, its value
is called as a function to find the defun's beginning.

Return non-nil if this function successfully found the beginning
of a defun, nil if it failed to find one."
  (interactive "^p")   ; change this to "P", maybe, if we ever come to pass ARG
                      ; to beginning-of-defun-function.
  (unless arg (setq arg 1))
  (cond
   (beginning-of-defun-function
    (condition-case nil
        (funcall beginning-of-defun-function arg)
      ;; We used to define beginning-of-defun-function as taking no argument
      ;; but that makes it impossible to implement correct forward motion:
      ;; we used to use end-of-defun for that, but it's not supposed to do
      ;; the same thing (it moves to the end of a defun not to the beginning
      ;; of the next).
      ;; In case the beginning-of-defun-function uses the old calling
      ;; convention, fallback on the old implementation.
      (wrong-number-of-arguments
       (if (> arg 0)
           (dotimes (_ arg)
             (funcall beginning-of-defun-function))
	 (dotimes (_ (- arg))
	   (funcall end-of-defun-function))))))

   ((or defun-prompt-regexp open-paren-in-column-0-is-defun-start)
    (and (< arg 0) (not (eobp)) (forward-char 1))
    (and (let (found)
           (while
               (and (setq found
                          (re-search-backward
                           (if defun-prompt-regexp
			       (concat (if open-paren-in-column-0-is-defun-start
					   "^\\s(\\|" "")
				       "\\(?:" defun-prompt-regexp "\\)\\s(")
			     "^\\s(")
			                      nil 'move arg))
                    (save-match-data
                      (nth 8 (syntax-ppss)))))
           found)
	 (progn (goto-char (1- (match-end 0)))
                t)))

   ;; If open-paren-in-column-0-is-defun-start and defun-prompt-regexp
   ;; are both nil, column 0 has no significance - so scan forward
   ;; from BOB to see how nested point is, then carry on from there.
   ;;
   ;; It is generally not a good idea to land up here, because the
   ;; call to scan-lists below can be extremely slow.  This is because
   ;; back_comment in syntax.c may have to scan from bob to find the
   ;; beginning of each comment.  Fixing this is not trivial -- cyd.

   ((eq arg 0))
   (t
    (let ((floor (point-min))
	  (ceiling (point-max))
	  (arg-+ve (> arg 0)))
      (save-restriction
	(widen)
	(let ((ppss (with-suppressed-warnings ((obsolete syntax-begin-function))
                      (let (syntax-begin-function)
		        (syntax-ppss))))
	      ;; position of least enclosing paren, or nil.
	      encl-pos)
	  ;; Back out of any comment/string, so that encl-pos will always
	  ;; become nil if we're at top-level.
	  (when (nth 8 ppss)
	    (goto-char (nth 8 ppss))
	    (setq ppss (syntax-ppss)))	; should be fast, due to cache.
	  (setq encl-pos (syntax-ppss-toplevel-pos ppss))
	  (if encl-pos (goto-char encl-pos))

	  (and encl-pos arg-+ve (setq arg (1- arg)))
	  (and (not encl-pos) (not arg-+ve) (not (looking-at "\\s("))
	       (setq arg (1+ arg)))

	  (condition-case nil   ; to catch crazy parens.
	      (progn
		(goto-char (scan-lists (point) (- arg) 0))
		(if arg-+ve
		    (if (>= (point) floor)
			t
		      (goto-char floor)
		      nil)
		  ;; forward to next (, or trigger the c-c
		  (goto-char (1- (scan-lists (point) 1 -1)))
		  (if (<= (point) ceiling)
		      t
		    (goto-char ceiling)
		    nil)))
	    (error
	     (goto-char (if arg-+ve floor ceiling))
	     nil))))))))

(defun beginning-of-defun--in-emptyish-line-p ()
  "Return non-nil if the point is in an \"emptyish\" line.
This means a line that consists entirely of comments and/or
whitespace."
;; See https://lists.gnu.org/r/help-gnu-emacs/2016-08/msg00141.html
  (save-excursion
    (forward-line 0)
    (let ((ppss (syntax-ppss)))
      (and (null (nth 3 ppss))
           (< (line-end-position)
              (progn (when (nth 4 ppss)
                       (goto-char (nth 8 ppss)))
                     (forward-comment (point-max))
                     (point)))))))

(defun beginning-of-defun-comments (&optional arg)
  "Move to the beginning of ARGth defun, including comments."
  (interactive "^p")
  (unless arg (setq arg 1))
  (beginning-of-defun arg)
  (let (first-line-p)
    (while (let ((ppss (progn (setq first-line-p (= (forward-line -1) -1))
                              (syntax-ppss (line-end-position)))))
             (while (and (nth 4 ppss) ; If eol is in a line-spanning comment,
                         (< (nth 8 ppss) (line-beginning-position)))
               (goto-char (nth 8 ppss)) ; skip to comment start.
               (setq ppss (syntax-ppss (line-end-position))))
             (and (not first-line-p)
                  (progn (skip-syntax-backward
                          "-" (line-beginning-position))
                         (not (bolp))) ; Check for blank line.
                  (beginning-of-defun--in-emptyish-line-p)))) ; Check for non-comment text.
    (forward-line (if first-line-p 0 1))))

(defvar end-of-defun-function
  (lambda () (forward-sexp 1))
  "Function for `end-of-defun' to call.
This is used to find the end of the defun at point.
It is called with no argument, right after calling `beginning-of-defun-raw'.
So the function can assume that point is at the beginning of the defun body.
It should move point to the first position after the defun.")

(defvar end-of-defun-moves-to-eol t
  "Whether `end-of-defun' moves to eol before doing anything else.
Set this to nil if this movement adversely affects the buffer's
major mode's decisions about context.")

(defun end-of-defun (&optional arg interactive)
  "Move forward to next end of defun.
With argument, do it that many times.
Negative argument -N means move back to Nth preceding end of defun.

An end of a defun occurs right after the close-parenthesis that
matches the open-parenthesis that starts a defun; see function
`beginning-of-defun'.

If variable `end-of-defun-function' is non-nil, its value
is called as a function to find the defun's end.

If INTERACTIVE is non-nil, as it is interactively,
report errors as appropriate for this kind of usage."
  (interactive "^p\nd")
  (if interactive
      (condition-case e
          (end-of-defun arg nil)
        (scan-error (user-error (cadr e))))
    (or (not (eq this-command 'end-of-defun))
        (eq last-command 'end-of-defun)
        (and transient-mark-mode mark-active)
        (push-mark))
    (if (or (null arg) (= arg 0)) (setq arg 1))
    (let ((pos (point))
          (success nil)
          (beg (progn (when end-of-defun-moves-to-eol
                        (end-of-line 1))
                      (beginning-of-defun-raw 1) (point)))
	  (skip (lambda ()
		  ;; When comparing point against pos, we want to consider that
		  ;; if point was right after the end of the function, it's
		  ;; still considered as "in that function".
		  ;; E.g. `eval-defun' from right after the last close-paren.
		  (unless (bolp)
		    (skip-chars-forward " \t")
		    (if (looking-at "\\s<\\|\n")
		        (forward-line 1))))))
      (funcall end-of-defun-function)
      (when (<= arg 1)
        (funcall skip))
      (cond
       ((> arg 0)
        ;; Moving forward.
        (if (> (point) pos)
            ;; We already moved forward by one because we started from
            ;; within a function.
            (setq arg (1- arg))
          ;; We started from after the end of the previous function.
          (goto-char pos))
        ;; At this point, point either didn't move (because we started
        ;; in between two defun's), or is at the end of a defun
        ;; (because we started in the middle of a defun).
        (unless (zerop arg)
          (when (setq success (beginning-of-defun-raw (- arg)))
            (funcall end-of-defun-function))))
       ((< arg 0)
        ;; Moving backward.
        (if (< (point) pos)
            ;; We already moved backward because we started from between
            ;; two functions.
            (setq arg (1+ arg))
          ;; We started from inside a function.
          (goto-char beg))
        (unless (zerop arg)
          (when (setq success (beginning-of-defun-raw (- arg)))
            (setq beg (point))
            (funcall end-of-defun-function)))))
      (funcall skip)
      (while (and (< arg 0) (>= (point) pos) success)
        ;; We intended to move backward, but this ended up not doing so:
        ;; Try harder!
        (goto-char beg)
        (setq success (beginning-of-defun-raw (- arg)))
        ;; If we successfully moved pass point, or there is no further
        ;; defun beginnings anymore, stop.
        (if (or (>= (point) beg) (not success))
	    (setq arg 0)
	  (setq beg (point))
          (funcall end-of-defun-function)
	  (funcall skip))))))

(defun mark-defun (&optional arg interactive)
  "Put mark at end of this defun, point at beginning.
The defun marked is the one that contains point or follows point.
With positive ARG, mark this and that many next defuns; with negative
ARG, change the direction of marking.

If the mark is active, it marks the next or previous defun(s) after
the one(s) already marked.

If INTERACTIVE is non-nil, as it is interactively,
report errors as appropriate for this kind of usage."
  (interactive "p\nd")
  (if interactive
      (condition-case e
          (mark-defun arg nil)
        (scan-error (user-error (cadr e))))
    (setq arg (or arg 1))
    ;; There is no `mark-defun-back' function - see
    ;; https://lists.gnu.org/r/bug-gnu-emacs/2016-11/msg00079.html
    ;; for explanation
    (when (eq last-command 'mark-defun-back)
      (setq arg (- arg)))
    (when (< arg 0)
      (setq this-command 'mark-defun-back))
    (cond ((use-region-p)
           (if (>= arg 0)
               (set-mark
                (save-excursion
                  (goto-char (mark))
                  ;; change the dotimes below to (end-of-defun arg)
                  ;; once bug #24427 is fixed
                  (dotimes (_ignore arg)
                    (end-of-defun))
                  (point)))
             (beginning-of-defun-comments (- arg))))
          (t
           (let ((opoint (point))
                 beg end)
             (push-mark opoint)
             ;; Try first in this order for the sake of languages with nested
             ;; functions where several can end at the same place as with the
             ;; offside rule, e.g. Python.
             (beginning-of-defun-comments)
             (setq beg (point))
             (end-of-defun)
             (setq end (point))
             (when (or (and (<= (point) opoint)
                            (> arg 0))
                       (= beg (point-min))) ; we were before the first defun!
               ;; beginning-of-defun moved back one defun so we got the wrong
               ;; one.  If ARG < 0, however, we actually want to go back.
               (goto-char opoint)
               (end-of-defun)
               (setq end (point))
               (beginning-of-defun-comments)
               (setq beg (point)))
             (goto-char beg)
             (cond ((> arg 0)
                    ;; change the dotimes below to (end-of-defun arg)
                    ;; once bug #24427 is fixed
                    (dotimes (_ignore arg)
                      (end-of-defun))
                    (setq end (point))
                    (push-mark end nil t)
                    (goto-char beg))
                   (t
                    (goto-char beg)
                    (unless (= arg -1)
                      ;; beginning-of-defun behaves strange with zero arg - see
                      ;; lists.gnu.org/r/bug-gnu-emacs/2017-02/msg00196.html
                      (beginning-of-defun (1- (- arg))))
                    (push-mark end nil t))))))
    (skip-chars-backward "[:space:]\n")
    (unless (bobp)
      (forward-line 1))))

(defvar narrow-to-defun-include-comments nil
  "If non-nil, `narrow-to-defun' will also show comments preceding the defun.")

(defun narrow-to-defun (&optional include-comments)
  "Make text outside current defun invisible.
The current defun is the one that contains point or follows point.
Preceding comments are included if INCLUDE-COMMENTS is non-nil.
Interactively, the behavior depends on `narrow-to-defun-include-comments'."
  (interactive (list narrow-to-defun-include-comments))
  (save-excursion
    (widen)
    (let ((opoint (point))
	  beg end)
      ;; Try first in this order for the sake of languages with nested
      ;; functions where several can end at the same place as with
      ;; the offside rule, e.g. Python.

      ;; Finding the start of the function is a bit problematic since
      ;; `beginning-of-defun' when we are on the first character of
      ;; the function might go to the previous function.
      ;;
      ;; Therefore we first move one character forward and then call
      ;; `beginning-of-defun'.  However now we must check that we did
      ;; not move into the next function.
      (let ((here (point)))
        (unless (eolp)
	  (forward-char))
        (beginning-of-defun)
        (when (< (point) here)
          (goto-char here)
          (beginning-of-defun)))
      (setq beg (point))
      (end-of-defun)
      (setq end (point))
      (while (looking-at "^\n")
	(forward-line 1))
      (unless (> (point) opoint)
	;; beginning-of-defun moved back one defun
	;; so we got the wrong one.
	(goto-char opoint)
	(end-of-defun)
	(setq end (point))
	(beginning-of-defun)
	(setq beg (point)))
      (when include-comments
	(goto-char beg)
	;; Move back past all preceding comments (and whitespace).
	(when (forward-comment -1)
	  (while (forward-comment -1))
	  ;; Move forwards past any page breaks within these comments.
	  (when (and page-delimiter (not (string= page-delimiter "")))
	    (while (re-search-forward page-delimiter beg t)))
	  ;; Lastly, move past any empty lines.
	  (skip-chars-forward "[:space:]\n")
	  (beginning-of-line)
	  (setq beg (point))))
      (goto-char end)
      (re-search-backward "^\n" (- (point) 1) t)
      (narrow-to-region beg end))))

(defcustom insert-pair-alist
  '((?\( ?\)) (?\[ ?\]) (?\{ ?\}) (?\< ?\>) (?\" ?\") (?\' ?\') (?\` ?\'))
  "Alist of paired characters inserted by `insert-pair'.
Each element looks like (OPEN-CHAR CLOSE-CHAR) or (COMMAND-CHAR
OPEN-CHAR CLOSE-CHAR).  The characters OPEN-CHAR and CLOSE-CHAR
of the pair whose key is equal to the last input character with
or without modifiers, are inserted by `insert-pair'.

If COMMAND-CHAR is specified, it is a character that triggers the
insertion of the open/close pair, and COMMAND-CHAR itself isn't
inserted."
  :type '(repeat (choice (list :tag "Pair"
                               (character :tag "Open")
                               (character :tag "Close"))
                         (list :tag "Triple"
                               (character :tag "Command")
                               (character :tag "Open")
                               (character :tag "Close"))))
  :group 'lisp
  :version "27.1")

(defun insert-pair (&optional arg open close)
  "Enclose following ARG sexps in a pair of OPEN and CLOSE characters.
Leave point after the first character.
A negative ARG encloses the preceding ARG sexps instead.
No argument is equivalent to zero: just insert characters
and leave point between.
If `parens-require-spaces' is non-nil, this command also inserts a space
before and after, depending on the surrounding characters.
If region is active, insert enclosing characters at region boundaries.

If arguments OPEN and CLOSE are nil, the character pair is found
from the variable `insert-pair-alist' according to the last input
character with or without modifiers.  If no character pair is
found in the variable `insert-pair-alist', then the last input
character is inserted ARG times.

This command assumes point is not in a string or comment."
  (interactive "P")
  (if (not (and open close))
      (let ((pair (or (assq last-command-event insert-pair-alist)
                      (assq (event-basic-type last-command-event)
                            insert-pair-alist))))
        (if pair
            (if (nth 2 pair)
                (setq open (nth 1 pair) close (nth 2 pair))
              (setq open (nth 0 pair) close (nth 1 pair))))))
  (if (and open close)
      (if (and transient-mark-mode mark-active)
          (progn
            (save-excursion
              (goto-char (region-end))
              (insert close))
            (goto-char (region-beginning))
            (insert open))
        (if arg (setq arg (prefix-numeric-value arg))
          (setq arg 0))
        (cond ((> arg 0) (skip-chars-forward " \t"))
              ((< arg 0) (forward-sexp arg) (setq arg (- arg))))
        (and parens-require-spaces
             (not (bobp))
             (memq (char-syntax (preceding-char)) (list ?w ?_ (char-syntax close)))
             (insert " "))
        (insert open)
        (save-excursion
          (or (eq arg 0) (forward-sexp arg))
          (insert close)
          (and parens-require-spaces
               (not (eobp))
               (memq (char-syntax (following-char)) (list ?w ?_ (char-syntax open)))
               (insert " "))))
    (insert-char (event-basic-type last-command-event)
                 (prefix-numeric-value arg))))

(defun insert-parentheses (&optional arg)
  "Enclose following ARG sexps in parentheses.
Leave point after open-paren.
A negative ARG encloses the preceding ARG sexps instead.
No argument is equivalent to zero: just insert `()' and leave point between.
If `parens-require-spaces' is non-nil, this command also inserts a space
before and after, depending on the surrounding characters.
If region is active, insert enclosing characters at region boundaries.

This command assumes point is not in a string or comment."
  (interactive "P")
  (insert-pair arg ?\( ?\)))

(defcustom delete-pair-blink-delay blink-matching-delay
  "Time in seconds to delay after showing a paired character to delete.
It's used by the command `delete-pair'.  The value 0 disables blinking."
  :type 'number
  :group 'lisp
  :version "28.1")

(defcustom delete-pair-push-mark nil
  "Non-nil means `delete-pair' pushes mark at end of delimited region."
  :type 'boolean
  :group 'lisp
  :version "31.1")

(defun delete-pair (&optional arg)
  "Delete a pair of characters enclosing ARG sexps that follow point.
A negative ARG deletes a pair around the preceding ARG sexps instead.
The option `delete-pair-blink-delay' can disable blinking.  With
`delete-pair-push-mark' enabled, pushes a mark at the end of the
enclosed region."
  (interactive "P")
  (if arg
      (setq arg (prefix-numeric-value arg))
    (setq arg 1))
  (if (< arg 0)
      (save-excursion
	(skip-chars-backward " \t")
	(save-excursion
	  (let ((close-char (char-before)))
	    (forward-sexp arg)
	    (unless (member (list (char-after) close-char)
			    (mapcar (lambda (p)
				      (if (= (length p) 3) (cdr p) p))
				    insert-pair-alist))
	      (error "Not after matching pair"))
	    (when (and (numberp delete-pair-blink-delay)
		       (> delete-pair-blink-delay 0))
	      (sit-for delete-pair-blink-delay))
	    (delete-char 1)))
	(delete-char -1))
    (save-excursion
      (skip-chars-forward " \t")
      (save-excursion
	(let ((open-char (char-after)))
	  (forward-sexp arg)
	  (unless (member (list open-char (char-before))
			  (mapcar (lambda (p)
				    (if (= (length p) 3) (cdr p) p))
				  insert-pair-alist))
	    (error "Not before matching pair"))
	  (when (and (numberp delete-pair-blink-delay)
		     (> delete-pair-blink-delay 0))
	    (sit-for delete-pair-blink-delay))
	  (delete-char -1)
	  (when delete-pair-push-mark
	    (push-mark))))
      (delete-char 1))))

(defun raise-sexp (&optional n)
  "Raise N sexps one level higher up the tree.

This function removes the sexp enclosing the form which follows
point, and then re-inserts N sexps that originally followed point,
thus raising those N sexps one level up.

Interactively, N is the numeric prefix argument, and defaults to 1.

For instance, if you have:

  (let ((foo 2))
    (progn
      (setq foo 3)
      (zot)
      (+ foo 2)))

and point is before (zot), \\[raise-sexp] will give you

  (let ((foo 2))
    (zot))"
  (interactive "p")
  (let ((s (if (and transient-mark-mode mark-active)
               (buffer-substring (region-beginning) (region-end))
             (buffer-substring
              (point)
              (save-excursion (forward-sexp n) (point))))))
    (backward-up-list 1)
    (delete-region (point) (save-excursion (forward-sexp 1) (point)))
    (save-excursion (insert s))))

(defun move-past-close-and-reindent ()
  "Move past next `)', delete indentation before it, then indent after it."
  (interactive)
  (up-list 1)
  (forward-char -1)
  (while (save-excursion		; this is my contribution
	   (let ((before-paren (point)))
	     (back-to-indentation)
	     (and (= (point) before-paren)
		  (progn
		    ;; Move to end of previous line.
		    (beginning-of-line)
		    (forward-char -1)
		    ;; Verify it doesn't end within a string or comment.
		    (let ((end (point))
			  state)
		      (beginning-of-line)
		      ;; Get state at start of line.
		      (setq state  (list 0 nil nil
					 (null (calculate-lisp-indent))
					 nil nil nil nil
					 nil))
		      ;; Parse state across the line to get state at end.
		      (setq state (parse-partial-sexp (point) end nil nil
						      state))
		      ;; Check not in string or comment.
		      (and (not (elt state 3)) (not (elt state 4))))))))
    (delete-indentation))
  (forward-char 1)
  (newline-and-indent))

(defun check-parens ()			; lame name?
  "Check for unbalanced parentheses in the current buffer.
More accurately, check the narrowed part of the buffer for unbalanced
expressions (\"sexps\") in general.  This is done according to the
current syntax table and will find unbalanced brackets or quotes as
appropriate.  (See Info node `(emacs)Parentheses'.)  If imbalance is
found, an error is signaled and point is left at the first unbalanced
character."
  (interactive)
  (condition-case data
      ;; Buffer can't have more than (point-max) sexps.
      (scan-sexps (point-min) (point-max))
    (scan-error (push-mark)
		(goto-char (nth 2 data))
		;; Could print (nth 1 data), which is either
		;; "Containing expression ends prematurely" or
		;; "Unbalanced parentheses", but those may not be so
		;; accurate/helpful, e.g. quotes may actually be
		;; mismatched.
  		(user-error "Unmatched bracket or quote"))))

;;;; textmodes/text-mode.el

(defun center-paragraph ()
  "Center each nonblank line in the paragraph at or after point.
See `center-line' for more info."
  (interactive)
  (save-excursion
    (forward-paragraph)
    (or (bolp) (newline 1))
    (let ((end (point)))
      (backward-paragraph)
      (center-region (point) end))))

(defun center-region (from to)
  "Center each nonblank line starting in the region.
See `center-line' for more info."
  (interactive "r")
  (if (> from to)
      (let ((tem to))
	(setq to from from tem)))
  (save-excursion
    (save-restriction
      (narrow-to-region from to)
      (goto-char from)
      (while (not (eobp))
	(or (save-excursion (skip-chars-forward " \t") (eolp))
	    (center-line))
	(forward-line 1)))))

(defun center-line (&optional nlines)
  "Center the line point is on, within the width specified by `fill-column'.
This means adjusting the indentation so that it equals
the distance between the end of the text and `fill-column'.
The argument NLINES says how many lines to center."
  (interactive "P")
  (if nlines (setq nlines (prefix-numeric-value nlines)))
  (while (not (eq nlines 0))
    (save-excursion
      (let ((lm (current-left-margin))
            space)
	(beginning-of-line)
	(delete-horizontal-space)
	(end-of-line)
	(delete-horizontal-space)
        (setq space (- fill-column lm (current-column)))
        (if (> space 0)
            (indent-line-to (+ lm (/ space 2))))))
    (cond ((null nlines)
	   (setq nlines 0))
	  ((> nlines 0)
	   (setq nlines (1- nlines))
	   (forward-line 1))
	  ((< nlines 0)
	   (setq nlines (1+ nlines))
	   (forward-line -1)))))

