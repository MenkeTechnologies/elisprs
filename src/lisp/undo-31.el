;;; undo-31.el --- simple.el's undo commands and subr.el's change groups  -*- lexical-binding: t -*-

;; Verbatim definitions from GNU Emacs 31.1 lisp/simple.el, then lisp/subr.el,
;; in file order.  The undo records they read are made by src/host.rs
;; (`record_insert', `record_delete', ...), a port of undo.c.

;;;; simple.el

(defvar amalgamating-undo-limit 20
  "The maximum number of changes to possibly amalgamate when undoing changes.
The `undo' command will normally consider \"similar\" changes
(like inserting characters) to be part of the same change.  This
is called \"amalgamating\" the changes.  This variable says what
the maximum number of changes considered is when amalgamating.  A
value of 1 means that nothing is amalgamated.")

(defconst undo-equiv-table (make-hash-table :test 'eq :weakness t)
  "Table mapping redo records to the corresponding undo one.
A redo record for an undo in region maps to `undo-in-region'.
A redo record for ordinary undo maps to the following (earlier) undo.
A redo record that undoes to the beginning of the undo list maps to t.
In the rare case where there are (erroneously) consecutive nil's in
`buffer-undo-list', `undo' maps the previous valid undo record to
`empty', if the previous record is a redo record, `undo' doesn't change
its mapping.

To be clear, a redo record is just an undo record, the only difference
is that it is created by an undo command (instead of an ordinary buffer
edit).  Since a record used to undo ordinary change is called undo
record, a record used to undo an undo is called redo record.

`undo' uses this table to make sure the previous command is `undo'.
`undo-redo' uses this table to set the correct `pending-undo-list'.

When you undo, `pending-undo-list' shrinks and `buffer-undo-list'
grows, and Emacs maps the tip of `buffer-undo-list' to the tip of
`pending-undo-list' in this table.

For example, consider this undo list where each node represents an
undo record: if we undo from 4, `pending-undo-list' will be at 3,
`buffer-undo-list' at 5, and 5 will map to 3.

    |
    3  5
    | /
    |/
    4")

(defvar undo-in-region nil
  "Non-nil if `pending-undo-list' is not just a tail of `buffer-undo-list'.")

(defcustom undo-no-redo nil
  "If t, `undo' doesn't go through redo entries."
  :type 'boolean
  :group 'undo)

(defvar pending-undo-list nil
  "Within a run of consecutive undo commands, list remaining to be undone.
If t, we undid all the way to the end of it.")

(defun undo--last-change-was-undo-p (undo-list)
  (while (and (consp undo-list) (eq (car undo-list) nil))
    (setq undo-list (cdr undo-list)))
  (gethash undo-list undo-equiv-table))

(defun undo (&optional arg)
  "Undo some previous changes.
Repeat this command to undo more changes.
A numeric ARG serves as a repeat count.

In Transient Mark mode when the mark is active, undo changes only within
the current region.  Similarly, when not in Transient Mark mode, just \\[universal-argument]
as an argument limits undo to changes within the current region."
  (interactive "*P")
  ;; Make last-command indicate for the next command that this was an undo.
  ;; That way, another undo will undo more.
  ;; If we get to the end of the undo history and get an error,
  ;; another undo command will find the undo history empty
  ;; and will get another error.  To begin undoing the undos,
  ;; you must type some other command.
  (let* ((modified (buffer-modified-p))
	 ;; For an indirect buffer, look in the base buffer for the
	 ;; auto-save data.
	 (base-buffer (or (buffer-base-buffer) (current-buffer)))
	 (recent-save (with-current-buffer base-buffer
			(recent-auto-save-p)))
         ;; Allow certain commands to inhibit an immediately following
         ;; undo-in-region.
         (inhibit-region (and (symbolp last-command)
                              (get last-command 'undo-inhibit-region)))
	 message)
    ;; If we get an error in undo-start,
    ;; the next command should not be a "consecutive undo".
    ;; So set `this-command' to something other than `undo'.
    (setq this-command 'undo-start)
    ;; Here we decide whether to break the undo chain.  If the
    ;; previous command is `undo', we don't call `undo-start', i.e.,
    ;; don't break the undo chain.
    (unless (and (eq last-command 'undo)
		 (or (eq pending-undo-list t)
		     ;; If something (a timer or filter?) changed the buffer
		     ;; since the previous command, don't continue the undo seq.
		     (undo--last-change-was-undo-p buffer-undo-list)))
      (setq undo-in-region
	    (and (or (region-active-p) (and arg (not (numberp arg))))
                 (not inhibit-region)))
      (if undo-in-region
	  (undo-start (region-beginning) (region-end))
	(undo-start))
      ;; get rid of initial undo boundary
      (undo-more 1))
    ;; If we got this far, the next command should be a consecutive undo.
    (setq this-command 'undo)
    ;; Check to see whether we're hitting a redo record, and if
    ;; so, ask the user whether she wants to skip the redo/undo pair.
    (let ((equiv (gethash pending-undo-list undo-equiv-table)))
      (or (eq (selected-window) (minibuffer-window))
	  (setq message (format "%s%s"
                                (if (or undo-no-redo (not equiv))
                                    "Undo" "Redo")
                                (if undo-in-region " in region" ""))))
      (when (and (consp equiv) undo-no-redo)
	;; The equiv entry might point to another redo record if we have done
	;; undo-redo-undo-redo-... so skip to the very last equiv.
	(while (let ((next (gethash equiv undo-equiv-table)))
		 (if next (setq equiv next))))
	(setq pending-undo-list (if (consp equiv) equiv t))))
    (undo-more
     (if (numberp arg)
	 (prefix-numeric-value arg)
       1))
    ;; Record the fact that the just-generated undo records come from an
    ;; undo operation--that is, they are redo records.
    ;; In the ordinary case (not within a region), map the redo
    ;; record to the following undos.
    ;; I don't know how to do that in the undo-in-region case.
    (let ((list buffer-undo-list))
      ;; Strip any leading undo boundaries there might be, like we do
      ;; above when checking.
      (while (eq (car list) nil)
	(setq list (cdr list)))
      (puthash list
               (cond
                (undo-in-region 'undo-in-region)
                ;; Prevent identity mapping.  This can happen if
                ;; consecutive nils are erroneously in undo list.  It
                ;; has to map to _something_ so that the next `undo'
                ;; command recognizes that the previous command is
                ;; `undo' and doesn't break the undo chain.
                ((eq list pending-undo-list)
                 (or (gethash list undo-equiv-table)
                     'empty))
                (t pending-undo-list))
	       undo-equiv-table))
    ;; Don't specify a position in the undo record for the undo command.
    ;; Instead, undoing this should move point to where the change is.
    (let ((tail buffer-undo-list)
	  (prev nil))
      (while (car tail)
	(when (integerp (car tail))
	  (let ((pos (car tail)))
	    (if prev
		(setcdr prev (cdr tail))
	      (setq buffer-undo-list (cdr tail)))
	    (setq tail (cdr tail))
	    (while (car tail)
	      (if (eq pos (car tail))
		  (if prev
		      (setcdr prev (cdr tail))
		    (setq buffer-undo-list (cdr tail)))
		(setq prev tail))
	      (setq tail (cdr tail)))
	    (setq tail nil)))
	(setq prev tail tail (cdr tail))))
    ;; Record what the current undo list says,
    ;; so the next command can tell if the buffer was modified in between.
    (and modified (not (buffer-modified-p))
	 (with-current-buffer base-buffer
	   (delete-auto-save-file-if-necessary recent-save)))
    ;; Display a message announcing success.
    (if message
	(message "%s" message))))

(defun undo-ignore-read-only (&optional arg)
  "Perform `undo', ignoring the buffer's read-only status.
A numeric ARG serves as a repeat count."
  (interactive "P")
  (let ((inhibit-read-only t))
    (undo arg)))

(defun undo-only (&optional arg)
  "Undo some previous changes.
Repeat this command to undo more changes.
A numeric ARG serves as a repeat count.
Contrary to `undo', this will not redo a previous undo."
  (interactive "*p")
  (let ((undo-no-redo t)) (undo arg)))

(defun undo-redo (&optional arg)
  "Undo the last ARG undos, i.e., redo the last ARG changes.
Interactively, ARG is the prefix numeric argument and defaults to 1."
  (interactive "*p")
  (cond
   ((not (undo--last-change-was-undo-p buffer-undo-list))
    (user-error "No undone changes to redo"))
   (t
    (let* ((ul buffer-undo-list)
           (new-ul
            (let ((undo-in-progress t))
              (while (and (consp ul) (eq (car ul) nil))
                (setq ul (cdr ul)))
              (primitive-undo (or arg 1) ul)))
           (new-pul (undo--last-change-was-undo-p new-ul)))
      (message "Redo%s" (if undo-in-region " in region" ""))
      (setq this-command 'undo)
      (setq pending-undo-list new-pul)
      (setq buffer-undo-list new-ul)))))

(defvar undo-in-progress nil
  "Non-nil while performing an undo.
Some change-hooks test this variable to do something different.")

(defun undo-more (n)
  "Undo back N undo-boundaries beyond what was already undone recently.
Call `undo-start' to get ready to undo recent changes,
then call `undo-more' one or more times to undo them."
  (or (listp pending-undo-list)
      (user-error (concat "No further undo information"
                          (and undo-in-region " for region"))))
  (let ((undo-in-progress t))
    ;; Note: The following, while pulling elements off
    ;; `pending-undo-list' will call primitive change functions which
    ;; will push more elements onto `buffer-undo-list'.
    (setq pending-undo-list (primitive-undo n pending-undo-list))
    (if (null pending-undo-list)
	(setq pending-undo-list t))))

(defun primitive-undo (n list)
  "Undo N records from the front of the list LIST.
Return what remains of the list."

  ;; This is a good feature, but would make undo-start
  ;; unable to do what is expected.
  ;;(when (null (car (list)))
  ;;  ;; If the head of the list is a boundary, it is the boundary
  ;;  ;; preceding this command.  Get rid of it and don't count it.
  ;;  (setq list (cdr list))))

  (let ((arg n)
        ;; In a writable buffer, enable undoing read-only text that is
        ;; so because of text properties.
        (inhibit-read-only t)
        ;; We use oldlist only to check for EQ.  ++kfs
        (oldlist buffer-undo-list)
        (did-apply nil)
        (next nil))
    (while (> arg 0)
      (while (setq next (pop list))     ;Exit inner loop at undo boundary.
        ;; Handle an integer by setting point to that value.
        (pcase next
          ((pred integerp) (goto-char next))
          ;; Element (t . TIME) records previous modtime.
          ;; Preserve any flag of NONEXISTENT_MODTIME_NSECS or
          ;; UNKNOWN_MODTIME_NSECS.
          (`(t . ,time)
           ;; If this records an obsolete save
           ;; (not matching the actual disk file)
           ;; then don't mark unmodified.
           (let ((visited-file-time (visited-file-modtime)))
             ;; Indirect buffers don't have a visited file, so their
             ;; file-modtime can be bogus.  In that case, use the
             ;; modtime of the base buffer instead.
             (if (and (numberp visited-file-time)
                      (= visited-file-time 0)
                      (buffer-base-buffer))
                 (setq visited-file-time
                      (with-current-buffer (buffer-base-buffer)
                        (visited-file-modtime))))
	     (when (time-equal-p time visited-file-time)
               (unlock-buffer)
               (set-buffer-modified-p nil))))
          ;; Element (nil PROP VAL BEG . END) is property change.
          (`(nil . ,(or `(,prop ,val ,beg . ,end) pcase--dontcare))
           (when (or (> (point-min) beg) (< (point-max) end))
             (error "Changes to be undone are outside visible portion of buffer"))
           (put-text-property beg end prop val))
          ;; Element (BEG . END) means range was inserted.
          (`(,(and beg (pred integerp)) . ,(and end (pred integerp)))
           ;; (and `(,beg . ,end) `(,(pred integerp) . ,(pred integerp)))
           ;; Ideally: `(,(pred integerp beg) . ,(pred integerp end))
           (when (or (> (point-min) beg) (< (point-max) end))
             (error "Changes to be undone are outside visible portion of buffer"))
           ;; Set point first thing, so that undoing this undo
           ;; does not send point back to where it is now.
           (goto-char beg)
           (delete-region beg end))
          ;; Element (apply FUN . ARGS) means call FUN to undo.
          (`(apply . ,fun-args)
           (let ((currbuff (current-buffer)))
             (if (integerp (car fun-args))
                 ;; Long format: (apply DELTA START END FUN . ARGS).
                 (pcase-let* ((`(,delta ,start ,end ,fun . ,args) fun-args)
                              (start-mark (copy-marker start nil))
                              (end-mark (copy-marker end t)))
                   (when (or (> (point-min) start) (< (point-max) end))
                     (error "Changes to be undone are outside visible portion of buffer"))
                   (apply fun args) ;; Use `save-current-buffer'?
                   ;; Check that the function did what the entry
                   ;; said it would do.
                   (unless (and (= start start-mark)
                                (= (+ delta end) end-mark))
                     (error "Changes undone by function are different from the announced ones"))
                   (set-marker start-mark nil)
                   (set-marker end-mark nil))
               (apply fun-args))
             (unless (eq currbuff (current-buffer))
               (error "Undo function switched buffer"))
             (setq did-apply t)))
          ;; Element (STRING . POS) means STRING was deleted.
          (`(,(and string (pred stringp)) . ,(and pos (pred integerp)))
           (let ((valid-marker-adjustments nil)
                 (apos (abs pos)))
             (when (or (< apos (point-min)) (> apos (point-max)))
               (error "Changes to be undone are outside visible portion of buffer"))
             ;; Check that marker adjustments which were recorded
             ;; with the (STRING . POS) record are still valid, ie
             ;; the markers haven't moved.  We check their validity
             ;; before reinserting the string so as we don't need to
             ;; mind marker insertion-type.
             (while (and (markerp (car-safe (car list)))
                         (integerp (cdr-safe (car list))))
               (let* ((marker-adj (pop list))
                      (m (car marker-adj)))
                 (and (eq (marker-buffer m) (current-buffer))
                      (= apos m)
                      (push marker-adj valid-marker-adjustments))))
             ;; Insert string and adjust point
             (if (< pos 0)
                 (progn
                   (goto-char (- pos))
                   (insert string))
               (goto-char pos)
               (insert string)
               (goto-char pos))
             ;; Adjust the valid marker adjustments
             (dolist (adj valid-marker-adjustments)
               ;; Insert might have invalidated some of the markers
               ;; via modification hooks.  Update only the currently
               ;; valid ones (bug#25599).
               (if (marker-buffer (car adj))
                   (set-marker (car adj)
                               (- (car adj) (cdr adj)))))))
          ;; (MARKER . OFFSET) means a marker MARKER was adjusted by OFFSET.
          (`(,(and marker (pred markerp)) . ,(and offset (pred integerp)))
           (warn "Encountered %S entry in undo list with no matching (TEXT . POS) entry"
                 next)
           ;; Even though these elements are not expected in the undo
           ;; list, adjust them to be conservative for the 24.4
           ;; release.  (Bug#16818)
           (when (marker-buffer marker)
             (set-marker marker
                         (- marker offset)
                         (marker-buffer marker))))
          (_ (error "Unrecognized entry in undo list %S" next))))
      (setq arg (1- arg)))
    ;; Make sure an apply entry produces at least one undo entry,
    ;; so the test in `undo' for continuing an undo series
    ;; will work right.
    (if (and did-apply
             (eq oldlist buffer-undo-list))
        (setq buffer-undo-list
              (cons (list 'apply 'cdr nil) buffer-undo-list))))
  list)

(defun undo-copy-list (list)
  "Make a copy of undo list LIST."
  (mapcar 'undo-copy-list-1 list))

(defun undo-copy-list-1 (elt)
  (if (consp elt)
      (cons (car elt) (undo-copy-list-1 (cdr elt)))
    elt))

(defun undo-start (&optional beg end)
  "Set `pending-undo-list' to the front of the undo list.
The next call to `undo-more' will undo the most recently made change.
If BEG and END are specified, then undo only elements
that apply to text between BEG and END are used; other undo elements
are ignored.  If BEG and END are nil, all undo elements are used."
  (if (eq buffer-undo-list t)
      (user-error "No undo information in this buffer"))
  (setq pending-undo-list
	(if (and beg end (not (= beg end)))
	    (undo-make-selective-list (min beg end) (max beg end))
	  buffer-undo-list)))

(defun undo-make-selective-list (start end)
  "Return a list of undo elements for the region START to END.
The elements come from `buffer-undo-list', but we keep only the
elements inside this region, and discard those outside this
region.  The elements' positions are adjusted so as the returned
list can be applied to the current buffer."
  (let ((ulist buffer-undo-list)
        ;; A list of position adjusted undo elements in the region.
        (selective-list (list nil))
        ;; A list of undo-deltas for out of region undo elements.
        undo-deltas
        undo-elt)
    (while ulist
      (when undo-no-redo
        (while (consp (gethash ulist undo-equiv-table))
          (setq ulist (gethash ulist undo-equiv-table))))
      (setq undo-elt (car ulist))
      (cond
       ((null undo-elt)
        ;; Don't put two nils together in the list
        (when (car selective-list)
          (push nil selective-list)))
       ((and (consp undo-elt) (eq (car undo-elt) t))
        ;; This is a "was unmodified" element.  Keep it
        ;; if we have kept everything thus far.
        (when (not undo-deltas)
          (push undo-elt selective-list)))
       ;; Skip over marker adjustments, instead relying
       ;; on finding them after (TEXT . POS) elements
       ((markerp (car-safe undo-elt))
        nil)
       (t
        (let ((adjusted-undo-elt (undo-adjust-elt undo-elt
                                                  undo-deltas)))
          (if (undo-elt-in-region adjusted-undo-elt start end)
              (progn
                (setq end (+ end (cdr (undo-delta adjusted-undo-elt))))
                (push adjusted-undo-elt selective-list)
                ;; Keep (MARKER . ADJUSTMENT) if their (TEXT . POS) was
                ;; kept.  primitive-undo may discard them later.
                (when (and (stringp (car-safe adjusted-undo-elt))
                           (integerp (cdr-safe adjusted-undo-elt)))
                  (let ((list-i (cdr ulist)))
                    (while (markerp (car-safe (car list-i)))
                      (push (pop list-i) selective-list)))))
            (let ((delta (undo-delta undo-elt)))
              (when (/= 0 (cdr delta))
                (push delta undo-deltas)))))))
      (pop ulist))
    (nreverse selective-list)))

(defun undo-elt-in-region (undo-elt start end)
  "Determine whether UNDO-ELT falls inside the region START ... END.
If it crosses the edge, we return nil.

Generally this function is not useful for determining
whether (MARKER . ADJUSTMENT) undo elements are in the region,
because markers can be arbitrarily relocated.  Instead, pass the
marker adjustment's corresponding (TEXT . POS) element."
  (cond ((integerp undo-elt)
         (<= start undo-elt end))
	((eq undo-elt nil)
	 t)
	((atom undo-elt)
	 nil)
	((stringp (car undo-elt))
	 ;; (TEXT . POSITION)
	 (<= start (abs (cdr undo-elt)) end))
	((and (consp undo-elt) (markerp (car undo-elt)))
	 ;; (MARKER . ADJUSTMENT)
         (<= start (car undo-elt) end))
	((null (car undo-elt))
	 ;; (nil PROPERTY VALUE BEG . END)
	 (let ((tail (nthcdr 3 undo-elt)))
	   (and (>= (car tail) start)
		(<= (cdr tail) end))))
	((integerp (car undo-elt))
	 ;; (BEGIN . END)
	 (and (>= (car undo-elt) start)
	      (<= (cdr undo-elt) end)))))

(defun undo-elt-crosses-region (undo-elt start end)
  "Test whether UNDO-ELT crosses one edge of that region START ... END.
This assumes we have already decided that UNDO-ELT
is not *inside* the region START...END."
  (declare (obsolete nil "25.1"))
  (cond ((atom undo-elt) nil)
	((null (car undo-elt))
	 ;; (nil PROPERTY VALUE BEG . END)
	 (let ((tail (nthcdr 3 undo-elt)))
	   (and (< (car tail) end)
		(> (cdr tail) start))))
	((integerp (car undo-elt))
	 ;; (BEGIN . END)
	 (and (< (car undo-elt) end)
	      (> (cdr undo-elt) start)))))

(defun undo-adjust-elt (elt deltas)
  "Return adjustment of undo element ELT by the undo DELTAS list."
  (pcase elt
    ;; POSITION
    ((pred integerp)
     (undo-adjust-pos elt deltas))
    ;; (BEG . END)
    (`(,(and beg (pred integerp)) . ,(and end (pred integerp)))
     (undo-adjust-beg-end beg end deltas))
    ;; (TEXT . POSITION)
    (`(,(and text (pred stringp)) . ,(and pos (pred integerp)))
     (cons text (* (if (< pos 0) -1 1)
                   (undo-adjust-pos (abs pos) deltas))))
    ;; (nil PROPERTY VALUE BEG . END)
    (`(nil . ,(or `(,prop ,val ,beg . ,end) pcase--dontcare))
     `(nil ,prop ,val . ,(undo-adjust-beg-end beg end deltas)))
    ;; (apply DELTA START END FUN . ARGS)
    ;; FIXME
    ;; All others return same elt
    (_ elt)))

(defun undo-adjust-beg-end (beg end deltas)
  "Return cons of adjustments to BEG and END by the undo DELTAS list."
  (let ((adj-beg (undo-adjust-pos beg deltas)))
    ;; Note: option 2 above would be like (cons (min ...) adj-end)
    (cons adj-beg
          (max adj-beg (undo-adjust-pos end deltas t)))))

(defun undo-adjust-pos (pos deltas &optional use-<)
  "Return adjustment of POS by the undo DELTAS list, comparing
with < or <= based on USE-<."
  (dolist (d deltas pos)
    (when (if use-<
              (< (car d) pos)
            (<= (car d) pos))
      (setq pos
            ;; Don't allow pos to become less than the undo-delta
            ;; position.  This edge case is described in the overview
            ;; comments.
            (max (car d) (- pos (cdr d)))))))

(defun undo-delta (undo-elt)
  (if (consp undo-elt)
      (cond ((stringp (car undo-elt))
	     ;; (TEXT . POSITION)
	     (cons (abs (cdr undo-elt)) (length (car undo-elt))))
	    ((integerp (car undo-elt))
	     ;; (BEGIN . END)
	     (cons (car undo-elt) (- (car undo-elt) (cdr undo-elt))))
	    ;; (apply DELTA BEG END FUNC . ARGS)
	    ((and (eq (car undo-elt) 'apply) (integerp (nth 1 undo-elt)))
	     (cons (nth 2 undo-elt) (nth 1 undo-elt)))
	    (t
	     '(0 . 0)))
    '(0 . 0)))

(defvar-local undo-auto--last-boundary-cause nil
  "Describe the cause of the last `undo-boundary'.

If `explicit', the last boundary was caused by an explicit call to
`undo-boundary', that is one not called by the code in this
section.

If it is equal to `timer', then the last boundary was inserted
by `undo-auto--boundary-timer'.

If it is equal to `command', then the last boundary was inserted
automatically after a command, that is by the code defined in
this section.

If it is equal to a list, then the last boundary was inserted by
an amalgamating command.  The car of the list is the number of
times an amalgamating command has been called, and the cdr are the
buffers that were changed during the last command.")

(defvar undo-auto-current-boundary-timer nil
  "Current timer which will run `undo-auto--boundary-timer' or nil.

If set to non-nil, this will effectively disable the timer.")

(defvar undo-auto--this-command-amalgamating nil
  "Non-nil if `this-command' should be amalgamated.
This variable is set to nil by `undo-auto--boundaries' and is set
by `undo-auto-amalgamate'." )

(defun undo-auto--needs-boundary-p ()
  "Return non-nil if `buffer-undo-list' needs a boundary at the start."
  (car-safe buffer-undo-list))

(defun undo-auto--last-boundary-amalgamating-number ()
  "Return the number of amalgamating last commands or nil.
Amalgamating commands are, by default, either
`self-insert-command' and `delete-char', but can be any command
that calls `undo-auto-amalgamate'."
  (car-safe undo-auto--last-boundary-cause))

(defun undo-auto--ensure-boundary (cause)
  "Add an `undo-boundary' to the current buffer if needed.
REASON describes the reason that the boundary is being added; see
`undo-auto--last-boundary-cause' for more information."
  (when (and
         (undo-auto--needs-boundary-p))
    (let ((last-amalgamating
           (undo-auto--last-boundary-amalgamating-number)))
      (undo-boundary)
      (setq undo-auto--last-boundary-cause
            (if (eq 'amalgamate cause)
                (cons
                 (if last-amalgamating (1+ last-amalgamating) 0)
                 undo-auto--undoably-changed-buffers)
              cause)))))

(defun undo-auto--boundaries (cause)
  "Check recently changed buffers and add a boundary if necessary.
REASON describes the reason that the boundary is being added; see
`undo-auto--last-boundary-cause' for more information."
  ;; (Bug #23785) All commands should ensure that there is an undo
  ;; boundary whether they have changed the current buffer or not.
  (when (eq cause 'command)
    (add-to-list 'undo-auto--undoably-changed-buffers (current-buffer)))
  (dolist (b undo-auto--undoably-changed-buffers)
          (when (buffer-live-p b)
            (with-current-buffer b
              (undo-auto--ensure-boundary cause))))
  (setq undo-auto--undoably-changed-buffers nil))

(defun undo-auto--boundary-timer ()
  "Timer function run by `undo-auto-current-boundary-timer'."
  (setq undo-auto-current-boundary-timer nil)
  (undo-auto--boundaries 'timer))

(defun undo-auto--boundary-ensure-timer ()
  "Ensure that the `undo-auto-current-boundary-timer' is set."
  (unless undo-auto-current-boundary-timer
    (setq undo-auto-current-boundary-timer
          (run-at-time 10 nil #'undo-auto--boundary-timer))))

(defvar undo-auto--undoably-changed-buffers nil
  "List of buffers that have changed recently.

This list is maintained by `undo-auto--undoable-change' and
`undo-auto--boundaries' and can be affected by changes to their
default values.")

(defun undo-auto--add-boundary ()
  "Add an `undo-boundary' in appropriate buffers."
  (undo-auto--boundaries
   (let ((amal undo-auto--this-command-amalgamating))
     (setq undo-auto--this-command-amalgamating nil)
     (if amal
         'amalgamate
       'command))))

(defun undo-auto-amalgamate ()
  "Amalgamate undo if necessary.
This function can be called before an amalgamating command.  It
removes the previous `undo-boundary' if a series of such calls
have been made.  `self-insert-command' and `delete-char' are the
most common amalgamating commands, although this function can be
called by any command which desires this behavior.
`analyze-text-conversion' (which see) is also an amalgamating
command in most circumstances."
  (let ((last-amalgamating-count
         (undo-auto--last-boundary-amalgamating-number)))
    (setq undo-auto--this-command-amalgamating t)
    (when last-amalgamating-count
      (if (and (< last-amalgamating-count amalgamating-undo-limit)
               (eq this-command last-command))
          ;; Amalgamate all buffers that have changed.
          ;; This may be needed for example if some *-change-functions
          ;; reflected these changes in some other buffer.
          (dolist (b (cdr undo-auto--last-boundary-cause))
            (when (buffer-live-p b)
              (with-current-buffer
                  b
                (when (and (consp buffer-undo-list)
                           ;; `car-safe' doesn't work because
                           ;; `buffer-undo-list' need not be a list!
                           (null (car buffer-undo-list)))
                  ;; The head of `buffer-undo-list' is nil.
                  (setq buffer-undo-list
                        (cdr buffer-undo-list))))))
        (setq undo-auto--last-boundary-cause 0)))))

(defun undo-auto--undoable-change ()
  "Called after every undoable buffer change."
  (unless (memq (current-buffer) undo-auto--undoably-changed-buffers)
    (let ((bufs undo-auto--undoably-changed-buffers))
      ;; Drop dead buffers from the list, to avoid memory leak in
      ;; (while t (with-temp-buffer (setq buffer-undo-list nil) (insert "a")))
      (while bufs
        (let ((next (cdr bufs)))
          (if (or (buffer-live-p (car bufs)) (null next))
              (setq bufs next)
            (setcar bufs (car next))
            (setcdr bufs (cdr next))))))
    (push (current-buffer) undo-auto--undoably-changed-buffers))
  (undo-auto--boundary-ensure-timer))

(defun undo-amalgamate-change-group (handle)
  "Amalgamate changes in change-group since HANDLE.
Remove all undo boundaries between the state of HANDLE and now.
HANDLE is as returned by `prepare-change-group'."
  (dolist (elt handle)
    (with-current-buffer (car elt)
      (setq elt (cdr elt))
      (when (consp buffer-undo-list)
        (let ((old-car (car-safe elt))
              (old-cdr (cdr-safe elt)))
          (unwind-protect
              (progn
                ;; Temporarily truncate the undo log at ELT.
                (when (consp elt)
                  (setcar elt t) (setcdr elt nil))
                (when
                    (or (null elt)        ;The undo-log was empty.
                        ;; `elt' is still in the log: normal case.
                        (eq elt (last buffer-undo-list))
                        ;; `elt' is not in the log any more, but that's because
                        ;; the log is "all new", so we should remove all
                        ;; boundaries from it.
                        (not (eq (last buffer-undo-list) (last old-cdr))))
                  (cl-callf (lambda (x) (delq nil x))
                      (if (car buffer-undo-list)
                          buffer-undo-list
                        ;; Preserve the undo-boundaries at either ends of the
                        ;; change-groups.
                        (cdr buffer-undo-list)))))
            ;; Reset the modified cons cell ELT to its original content.
            (when (consp elt)
              (setcar elt old-car)
              (setcdr elt old-cdr))))))))

(defcustom undo-ask-before-discard nil
  "If non-nil ask about discarding undo info for the current command.
Normally, Emacs discards the undo info for the current command if
it exceeds `undo-outer-limit'.  But if you set this option
non-nil, it asks in the echo area whether to discard the info.
If you answer no, there is a slight risk that Emacs might crash, so
do it only if you really want to undo the command.

This option is mainly intended for debugging.  You have to be
careful if you use it for other purposes.  Garbage collection is
inhibited while the question is asked, meaning that Emacs might
leak memory.  So you should make sure that you do not wait
excessively long before answering the question."
  :type 'boolean
  :group 'undo
  :version "22.1")

(defvar-local undo-extra-outer-limit nil
  "If non-nil, an extra level of size that's ok in an undo item.
We don't ask the user about truncating the undo list until the
current item gets bigger than this amount.

This variable matters only if `undo-ask-before-discard' is non-nil.")

(defun undo-outer-limit-truncate (size)
  (if undo-ask-before-discard
      (when (or (null undo-extra-outer-limit)
		(> size undo-extra-outer-limit))
	;; Don't ask the question again unless it gets even bigger.
	;; This applies, in particular, if the user quits from the question.
	;; Such a quit quits out of GC, but something else will call GC
	;; again momentarily.  It will call this function again,
	;; but we don't want to ask the question again.
	(setq undo-extra-outer-limit (+ size 50000))
	(if (let (use-dialog-box track-mouse executing-kbd-macro )
	      (yes-or-no-p (format-message
                            "Buffer `%s' undo info is %d bytes long; discard it? "
                            (buffer-name) size)))
	    (progn (setq buffer-undo-list nil)
		   (setq undo-extra-outer-limit nil)
		   t)
	  nil))
    (display-warning '(undo discard-info)
		     (concat
		      (format-message
                       "Buffer `%s' undo info was %d bytes long.\n"
                       (buffer-name) size)
		      "The undo info was discarded because it exceeded \
`undo-outer-limit'.

This is normal if you executed a command that made a huge change
to the buffer.  In that case, to prevent similar problems in the
future, set `undo-outer-limit' to a value that is large enough to
cover the maximum size of normal changes you expect a single
command to make, but not so large that it might exceed the
maximum memory allotted to Emacs.

If you did not execute any such command, the situation is
probably due to a bug and you should report it.

You can disable the popping up of this buffer by adding the entry
\(undo discard-info) to the user option `warning-suppress-types',
which is defined in the `warnings' library.\n")
		     :warning)
    (setq buffer-undo-list nil)
    t))

;;;; subr.el

(defmacro atomic-change-group (&rest body)
  "Like `progn' but perform BODY as an atomic change group.
This means that if BODY exits abnormally,
all of its changes to the current buffer are undone.
This works regardless of whether undo is enabled in the buffer.

Do not call functions which edit the undo list within BODY; see
`prepare-change-group'.

This mechanism is transparent to ordinary use of undo;
if undo is enabled in the buffer and BODY succeeds, the
user can undo the change normally."
  (declare (indent 0) (debug t))
  (let ((handle (make-symbol "--change-group-handle--"))
	(success (make-symbol "--change-group-success--")))
    `(let ((,handle (prepare-change-group))
	   ;; Don't truncate any undo data in the middle of this.
	   (undo-outer-limit nil)
	   (undo-limit most-positive-fixnum)
	   (undo-strong-limit most-positive-fixnum)
	   (,success nil))
       (unwind-protect
	   (progn
	     ;; This is inside the unwind-protect because
	     ;; it enables undo if that was disabled; we need
	     ;; to make sure that it gets disabled again.
	     (activate-change-group ,handle)
	     (prog1 ,(macroexp-progn body)
	       (setq ,success t)))
	 ;; Either of these functions will disable undo
	 ;; if it was disabled before.
	 (if ,success
	     (accept-change-group ,handle)
	   (cancel-change-group ,handle))))))

(defmacro with-undo-amalgamate (&rest body)
  "Like `progn' but perform BODY with amalgamated undo barriers.

This allows multiple operations to be undone in a single step.
When undo is disabled this behaves like `progn'."
  (declare (indent 0) (debug t))
  (let ((handle (make-symbol "--change-group-handle--")))
    `(let ((,handle (prepare-change-group))
           ;; Don't truncate any undo data in the middle of this,
           ;; otherwise Emacs might truncate part of the resulting
           ;; undo step: we want to mimic the behavior we'd get if the
           ;; undo-boundaries were never added in the first place.
           (undo-outer-limit nil)
           (undo-limit most-positive-fixnum)
           (undo-strong-limit most-positive-fixnum))
       (unwind-protect
           (progn
             (activate-change-group ,handle)
             ,@body)
         (progn
           (accept-change-group ,handle)
           (undo-amalgamate-change-group ,handle))))))

(defun prepare-change-group (&optional buffer)
  "Return a handle for the current buffer's state, for a change group.
If you specify BUFFER, make a handle for BUFFER's state instead.

Pass the handle to `activate-change-group' afterward to initiate
the actual changes of the change group.

To finish the change group, call either `accept-change-group' or
`cancel-change-group' passing the same handle as argument.  Call
`accept-change-group' to accept the changes in the group as final;
call `cancel-change-group' to undo them all.  You should use
`unwind-protect' to make sure the group is always finished.  The call
to `activate-change-group' should be inside the `unwind-protect'.
Once you finish the group, don't use the handle again--don't try to
finish the same group twice.  For a simple example of correct use, see
the source code of `atomic-change-group'.

As long as this handle is still in use, do not call functions
which edit the undo list: if it no longer contains its current
value, Emacs will not be able to cancel the change group.  This
includes any \"amalgamating\" commands, such as `delete-char',
which call `undo-auto-amalgamate'.

The handle records only the specified buffer.  To make a multibuffer
change group, call this function once for each buffer you want to
cover, then use `nconc' to combine the returned values, like this:

  (nconc (prepare-change-group buffer-1)
         (prepare-change-group buffer-2))

You can then activate that multibuffer change group with a single
call to `activate-change-group' and finish it with a single call
to `accept-change-group' or `cancel-change-group'."

  (if buffer
      (list (cons buffer (with-current-buffer buffer buffer-undo-list)))
    (list (cons (current-buffer) buffer-undo-list))))

(defun activate-change-group (handle)
  "Activate a change group made with `prepare-change-group' (which see)."
  (dolist (elt handle)
    (with-current-buffer (car elt)
      (if (eq buffer-undo-list t)
	  (setq buffer-undo-list nil)
	;; Add a boundary to make sure the upcoming changes won't be
	;; merged/combined with any previous changes (bug#33341).
	;; We're not supposed to introduce a real (visible)
        ;; `undo-boundary', tho, so we have to push something else
        ;; that acts like a boundary w.r.t preventing merges while
	;; being harmless.
        ;; We use for that an "empty insertion", but in order to be harmless,
        ;; it has to be at a harmless position.  Currently only
        ;; insertions are ever merged/combined, so we use such a "boundary"
        ;; only when the last change was an insertion and we use the position
        ;; of the last insertion.
        (when (numberp (car-safe (car buffer-undo-list)))
          (push (cons (caar buffer-undo-list) (caar buffer-undo-list))
                buffer-undo-list))))))

(defun accept-change-group (handle)
  "Finish a change group made with `prepare-change-group' (which see).
This finishes the change group by accepting its changes as final."
  (dolist (elt handle)
    (with-current-buffer (car elt)
      (if (eq (cdr elt) t)
	  (setq buffer-undo-list t)))))

(defun cancel-change-group (handle)
  "Finish a change group made with `prepare-change-group' (which see).
This finishes the change group by reverting all of its changes."
  (dolist (elt handle)
    (with-current-buffer (car elt)
      (setq elt (cdr elt))
      (save-restriction
	;; Widen buffer temporarily so if the buffer was narrowed within
	;; the body of `atomic-change-group' all changes can be undone.
	(widen)
	(let ((old-car (car-safe elt))
	      (old-cdr (cdr-safe elt))
	      ;; Use `pending-undo-list' temporarily since `undo-more' needs
	      ;; it, but restore it afterwards so as not to mess with an
	      ;; ongoing sequence of `undo's.
	      (pending-undo-list
	       ;; Use `buffer-undo-list' unconditionally (bug#39680).
	       buffer-undo-list))
          (unwind-protect
              (progn
                ;; Temporarily truncate the undo log at ELT.
                (when (consp elt)
                  (setcar elt nil) (setcdr elt nil))
                ;; Make sure there's no confusion.
                (when (and (consp elt) (not (eq elt (last pending-undo-list))))
                  (error "Undoing to some unrelated state"))
                ;; Undo it all.
                (save-excursion
                  (while (listp pending-undo-list) (undo-more 1)))
                ;; Revert the undo info to what it was when we grabbed
                ;; the state.
                (setq buffer-undo-list elt))
            ;; Reset the modified cons cell ELT to its original content.
            (when (consp elt)
              (setcar elt old-car)
              (setcdr elt old-cdr))))))))

(defun undo--wrap-and-run-primitive-undo (beg end list)
  "Call `primitive-undo' on the undo elements in LIST.

This function is intended to be called purely by `undo' as the
function in an \(apply DELTA BEG END FUNNAME . ARGS) undo
element.  It invokes `before-change-functions' and
`after-change-functions' once each for the entire region \(BEG
END) rather than once for each individual change.

Additionally the fresh \"redo\" elements which are generated on
`buffer-undo-list' will themselves be \"enclosed\" in
`undo--wrap-and-run-primitive-undo'.

Undo elements of this form are generated by the macro
`combine-change-calls'."
  (combine-change-calls beg end
			(while list
			  (setq list (primitive-undo 1 list)))))

