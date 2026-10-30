;;; editfns-c.el --- editfns.c fields and other C primitives the editing libraries call  -*- lexical-binding: t -*-

;; Ports of C functions and variables that the preloaded editing libraries
;; (paragraphs.el, indent.el, fill.el, simple.el's commands) reach for:
;; editfns.c's fields, minibuf.c's `minibufferp', character.c's
;; `char-script-table' and keyboard.c's overriding keymaps.

;;;; editfns.c — fields.

(defvar inhibit-field-text-motion nil
  "Non-nil means text motion commands don't notice fields.")

(defun editfns--find-field (pos merge-at-boundary beg-limit end-limit)
  "find_field: return (BEG . END) of the field surrounding POS.
BEG-LIMIT and END-LIMIT bound the scan; MERGE-AT-BOUNDARY is the C's
argument of the same name."
  (if (null pos)
      (setq pos (point))
    (unless (integer-or-marker-p pos)
      (signal 'wrong-type-argument (list 'integer-or-marker-p pos)))
    (setq pos (if (markerp pos) (marker-position pos) pos)))
  (let* ((after-field (car (get-char-property-and-overlay pos 'field)))
         (before-field
          (if (> pos (point-min))
              (car (get-char-property-and-overlay (1- pos) 'field))
            ;; Using nil here would fail when the buffer starts with a
            ;; non-sticky field.
            after-field))
         (at-field-start nil)
         (at-field-end nil)
         beg end)
    (unless merge-at-boundary
      (let ((field (get-pos-property pos 'field)))
        (unless (eq field after-field)
          (setq at-field-end t))
        (unless (eq field before-field)
          (setq at-field-start t))
        (when (and (null field) at-field-start at-field-end)
          ;; A nil field between non-nil text is a field not meant for
          ;; editing (such as comint's prompts), not a zero-length field.
          (setq at-field-end nil at-field-start nil))))
    (setq beg
          (if at-field-start
              pos
            (let ((p pos))
              (when (and merge-at-boundary (eq before-field 'boundary))
                (setq p (previous-single-char-property-change
                         p 'field nil beg-limit)))
              (setq p (previous-single-char-property-change
                       p 'field nil beg-limit))
              (or p (point-min)))))
    (setq end
          (if at-field-end
              pos
            (let ((p pos))
              (when (and merge-at-boundary (eq after-field 'boundary))
                (setq p (next-single-char-property-change
                         p 'field nil end-limit)))
              (setq p (next-single-char-property-change
                       p 'field nil end-limit))
              (or p (point-max)))))
    (cons beg end)))

(defun delete-field (&optional pos)
  "Delete the field surrounding POS.
A field is a region of text with the same `field' property.
If POS is nil, the value of point is used for POS."
  (let ((field (editfns--find-field pos nil nil nil)))
    (unless (= (car field) (cdr field))
      (delete-region (car field) (cdr field)))
    nil))

(defun field-string (&optional pos)
  "Return the contents of the field surrounding POS as a string.
A field is a region of text with the same `field' property.
If POS is nil, the value of point is used for POS."
  (let ((field (editfns--find-field pos nil nil nil)))
    (buffer-substring (car field) (cdr field))))

(defun field-string-no-properties (&optional pos)
  "Return the contents of the field around POS, without text properties.
A field is a region of text with the same `field' property.
If POS is nil, the value of point is used for POS."
  (let ((field (editfns--find-field pos nil nil nil)))
    (buffer-substring-no-properties (car field) (cdr field))))

(defun field-beginning (&optional pos escape-from-edge limit)
  "Return the beginning of the field surrounding POS.
A field is a region of text with the same `field' property.
If POS is nil, the value of point is used for POS.
If ESCAPE-FROM-EDGE is non-nil and POS is at the beginning of its
field, then the beginning of the *previous* field is returned.
If LIMIT is non-nil, it is a buffer position; if the beginning of the field
is before LIMIT, then LIMIT will be returned instead."
  (car (editfns--find-field pos escape-from-edge limit nil)))

(defun field-end (&optional pos escape-from-edge limit)
  "Return the end of the field surrounding POS.
A field is a region of text with the same `field' property.
If POS is nil, the value of point is used for POS.
If ESCAPE-FROM-EDGE is non-nil and POS is at the end of its field,
then the end of the *following* field is returned.
If LIMIT is non-nil, it is a buffer position; if the end of the field
is after LIMIT, then LIMIT will be returned instead."
  (cdr (editfns--find-field pos escape-from-edge nil limit)))

(defun constrain-to-field (new-pos old-pos &optional escape-from-edge
                                   only-in-line inhibit-capture-property)
  "Return the position closest to NEW-POS that is in the same field as OLD-POS.
A field is a region of text with the same `field' property.

If NEW-POS is nil, then use the current point instead, and move point
to the resulting constrained position, in addition to returning that
position.

If OLD-POS is at the boundary of two fields, then the allowable
positions for NEW-POS depends on the value of the optional argument
ESCAPE-FROM-EDGE: If ESCAPE-FROM-EDGE is nil, then NEW-POS is
constrained to the field that has the same `field' char-property
as any new characters inserted at OLD-POS, whereas if ESCAPE-FROM-EDGE
is non-nil, NEW-POS is constrained to the union of the two adjacent
fields.  Additionally, if two fields are separated by another field with
the special value `boundary', then any point within this special field is
also considered to be `on the boundary'.

If the optional argument ONLY-IN-LINE is non-nil and constraining
NEW-POS would move it to a different line, NEW-POS is returned
unconstrained.

If the optional argument INHIBIT-CAPTURE-PROPERTY is non-nil, and OLD-POS has
a non-nil property of that name, then any field boundaries are ignored.

Field boundaries are not noticed if `inhibit-field-text-motion' is non-nil."
  (let ((orig-point nil))
    (when (null new-pos)
      ;; Use the current point, and afterwards, set it.
      (setq orig-point (point)
            new-pos (point)))
    (unless (integer-or-marker-p new-pos)
      (signal 'wrong-type-argument (list 'integer-or-marker-p new-pos)))
    (unless (integer-or-marker-p old-pos)
      (signal 'wrong-type-argument (list 'integer-or-marker-p old-pos)))
    (when (markerp new-pos) (setq new-pos (marker-position new-pos)))
    (when (markerp old-pos) (setq old-pos (marker-position old-pos)))
    (let ((fwd (> new-pos old-pos))
          (prev-old (1- old-pos))
          (prev-new (1- new-pos)))
      (when (and (null inhibit-field-text-motion)
                 (not (eq new-pos old-pos))
                 (or (get-char-property new-pos 'field)
                     (get-char-property old-pos 'field)
                     ;; Field boundaries need the previous positions too:
                     ;; `get-pos-property' alone fails inside non-sticky
                     ;; fields (like comint prompts).
                     (and (> new-pos (point-min))
                          (get-char-property prev-new 'field))
                     (and (> old-pos (point-min))
                          (get-char-property prev-old 'field)))
                 (or (null inhibit-capture-property)
                     (and (null (get-pos-property old-pos
                                                  inhibit-capture-property))
                          (or (<= old-pos (point-min))
                              (null (get-char-property
                                     old-pos inhibit-capture-property))
                              (null (get-char-property
                                     prev-old inhibit-capture-property))))))
        ;; NEW-POS may not be within the same field as OLD-POS; try to move
        ;; it so that it is.
        (let ((field-bound (if fwd
                               (field-end old-pos escape-from-edge new-pos)
                             (field-beginning old-pos escape-from-edge new-pos))))
          (when (and
                 ;; ESCAPE-FROM-EDGE may have put FIELD-BOUND on the other
                 ;; side of NEW-POS, in which case NEW-POS is acceptable.
                 (if (< field-bound new-pos) fwd (not fwd))
                 ;; With ONLY-IN-LINE, constrain only when NEW-POS and
                 ;; FIELD-BOUND are on the same line (find_newline counts
                 ;; no newline between them).
                 (or (null only-in-line)
                     (save-excursion
                       (goto-char (min new-pos field-bound))
                       (not (search-forward "\n" (max new-pos field-bound) t)))))
            (setq new-pos field-bound))
          (when (and orig-point (/= new-pos orig-point))
            ;; NEW-POS was originally nil, so set point.
            (goto-char new-pos))))
      new-pos)))

;;;; minibuf.c

(defvar editfns--minibuffer-list (list (get-buffer " *Minibuf-0*"))
  "minibuf.c's `Vminibuffer_list': the minibuffers, innermost level last.
Batch Emacs reads nothing from a minibuffer, so only ` *Minibuf-0*' is
on it and `minibuf_level' is always 0.")

(defun minibufferp (&optional buffer live)
  "Return t if BUFFER is a minibuffer.
No argument or nil as argument means use current buffer as BUFFER.
BUFFER can be a buffer or a buffer name.  If LIVE is non-nil, then
return t only if BUFFER is an active minibuffer."
  (cond ((null buffer) (setq buffer (current-buffer)))
        ((stringp buffer) (setq buffer (get-buffer buffer)))
        ((not (bufferp buffer))
         (signal 'wrong-type-argument (list 'bufferp buffer))))
  ;; live_minibuffer_p: ` *Minibuf-0*' is never active, and with
  ;; `minibuf_level' 0 no other minibuffer is either.
  (and (not live) (memq buffer editfns--minibuffer-list) t))

(defun minibuffer-prompt-end ()
  "Return the buffer position of the end of the minibuffer prompt.
Return (point-min) if current buffer is not a minibuffer."
  (let ((beg (point-min)))
    (if (not (memq (current-buffer) editfns--minibuffer-list))
        beg
      (let ((end (field-end beg)))
        (if (and (= end (point-max)) (null (get-char-property beg 'field)))
            beg
          end)))))

;;;; character.c

(put 'char-script-table 'char-table-extra-slots 1)
(defvar char-script-table (make-char-table 'char-script-table nil)
  "Char table of script symbols.
It has one extra slot whose value is a list of script symbols.")

;;;; keyboard.c

(defvar overriding-terminal-local-map nil
  "Per-terminal keymap that takes precedence over all other keymaps.")
(defvar overriding-local-map nil
  "Keymap that replaces (overrides) local keymaps.")

;;;; charset.c — the charsets elisprs models: `ascii', `unicode',
;;;; `eight-bit' (raw bytes) and `emacs' (the rest of the code space).  The
;;;; ISO, national and legacy charsets of mule-conf.el are not loaded.

(defun editfns--char-charset (c)
  "CHAR_CHARSET under `emacs -Q's priority list, `(ascii unicode ...)'."
  (cond ((< c #x80) 'ascii)
        ((<= c #x10FFFF) 'unicode)
        ((>= c #x3FFF80) 'eight-bit)
        (t 'emacs)))

(defun editfns--charset-contains-p (charset c)
  "Whether CHARSET (one of the modeled charsets) encodes character C."
  (pcase charset
    ('ascii (< c #x80))
    ('unicode (<= c #x10FFFF))
    ('eight-bit (>= c #x3FFF80))
    ('emacs t)
    (_ (signal 'wrong-type-argument (list 'charsetp charset)))))

(defun char-charset (ch &optional restriction)
  "Return the charset of highest priority that contains CH.
ASCII characters are an exception: for them, this function always
returns `ascii'.
If optional 2nd arg RESTRICTION is non-nil, it is a list of charsets
from which to find the charset.  It may also be a coding system.  In
that case, find the charset in those supported by that coding system."
  (unless (characterp ch)
    (signal 'wrong-type-argument (list 'characterp ch)))
  (cond ((null restriction) (editfns--char-charset ch))
        ((consp restriction)
         (let ((found nil))
           (while (and (not found) (consp restriction))
             (when (editfns--charset-contains-p (car restriction) ch)
               (setq found (car restriction)))
             (setq restriction (cdr restriction)))
           found))
        (t (signal 'coding-system-error (list restriction)))))

(defun charset-after (&optional pos)
  "Return charset of a character in the current buffer at position POS.
If POS is nil, it defaults to the current point.
If POS is out of range, the value is nil."
  (let ((ch (char-after pos)))
    (if (fixnump ch)
        (editfns--char-charset ch)
      ch)))


;;;; insdel.c — the change hooks.  src/modhooks.rs calls the two signal
;;;; functions around every buffer-modifying primitive.

(defvar before-change-functions nil
  "List of functions to call before each text change.")
(defvar after-change-functions nil
  "List of functions to call after each text change.")
(defvar first-change-hook nil
  "A list of functions to call before changing a buffer which is unmodified.")

;;;; undo.c's variables (the recording itself is src/host.rs's).

(defvar undo-limit 160000
  "Keep no more undo information once it exceeds this size.")
(defvar undo-strong-limit 240000
  "Don't keep more than this much size of undo information.")
(defvar undo-outer-limit nil
  "Outer limit on size of undo information for one command.")
(defvar undo-outer-limit-function 'undo-outer-limit-truncate
  "Function to call when an undo list exceeds `undo-outer-limit'.")
(defvar undo-inhibit-record-point nil
  "Non-nil means do not record `point' in `buffer-undo-list'.")

;;;; fileio.c

(defun visited-file-modtime ()
  "Return the current buffer's recorded visited file modification time.
Return 0 if the buffer does not visit a file, which is every buffer here:
nothing records a visited file's modification time."
  0)

;;;; buffer.c

(defun buffer-base-buffer (&optional buffer)
  "Return the base buffer of indirect buffer BUFFER.
If BUFFER is not indirect, return nil.  There are no indirect buffers
here, so the answer is always nil.
BUFFER defaults to the current buffer."
  (unless (or (null buffer) (bufferp buffer))
    (signal 'wrong-type-argument (list 'bufferp buffer)))
  nil)

(defun find-buffer (variable value)
  "Return the first buffer with VARIABLE `equal' to VALUE.
VARIABLE's buffer-local value is compared in each live buffer, in the
order of `buffer-list'; nil if no buffer has it."
  (catch 'found
    (dolist (buf (buffer-list))
      (when (equal value (buffer-local-value variable buf))
        (throw 'found buf)))))

(defun get-file-buffer (filename)
  "Return the buffer visiting file FILENAME (a string).
The buffer's `buffer-file-name' must match exactly the expansion of
FILENAME.  If there is no such live buffer, return nil."
  (unless (stringp filename)
    (signal 'wrong-type-argument (list 'stringp filename)))
  (setq filename (expand-file-name filename))
  (catch 'found
    (dolist (buf (buffer-list))
      (let ((name (buffer-local-value 'buffer-file-name buf)))
        (when (and (stringp name) (string-equal name filename))
          (throw 'found buf))))))

(defun delete-all-overlays (&optional buffer)
  "Delete all overlays of BUFFER.
BUFFER omitted or nil means delete all overlays of the current buffer."
  (unless (or (null buffer) (bufferp buffer))
    (signal 'wrong-type-argument (list 'bufferp buffer)))
  (with-current-buffer (or buffer (current-buffer))
    (save-restriction
      (widen)
      (mapc #'delete-overlay (overlays-in (point-min) (point-max)))))
  nil)

;;;; data.c

(defun vector-or-char-table-p (object)
  "Return t if OBJECT is a char-table or vector."
  (if (or (vectorp object) (char-table-p object)) t nil))

;;;; xdisp.c

(defun editfns--invisible-prop (propval list)
  "invisible_prop: 1 if PROPVAL (or an element of it) is in LIST, 2 if it is
the car of a (ATOM . NON-NIL) element there (ellipsis), else 0."
  (catch 'found
    (dolist (tem list)
      (cond ((eq propval tem) (throw 'found 1))
            ((and (consp tem) (eq propval (car tem)))
             (throw 'found (if (cdr tem) 2 1)))))
    (when (consp propval)
      (dolist (propelt propval)
        (dolist (tem list)
          (cond ((eq propelt tem) (throw 'found 1))
                ((and (consp tem) (eq propelt (car tem)))
                 (throw 'found (if (cdr tem) 2 1)))))))
    0))

(defun invisible-p (pos-or-prop)
  "Non-nil if text properties at POS cause text there to be currently invisible.
POS should be a marker or a buffer position; the value of the `invisible'
property at that position in the current buffer is examined.
POS can also be the actual value of the `invisible' text or overlay
property of the text of interest, in which case the value itself is
examined.

The non-nil value returned can be t for currently invisible text that is
entirely hidden on display, or some other non-nil, non-t value if the
text is replaced by an ellipsis."
  (let* ((prop (if (or (natnump pos-or-prop) (markerp pos-or-prop))
                   (get-char-property pos-or-prop 'invisible)
                 pos-or-prop))
         (invis (if (eq buffer-invisibility-spec t)
                    (if prop 1 0)
                  (editfns--invisible-prop prop buffer-invisibility-spec))))
    (cond ((= invis 0) nil)
          ((= invis 1) t)
          (t invis))))

;;;; keyboard.c

(defvar unread-command-events nil
  "List of events to be read as the command input.")
(defvar timer-list nil
  "List of active timers in order of increasing time.")
(defvar timer-idle-list nil
  "List of active idle-time timers in order of increasing time.")

(defmacro editfns--run-change-hook (hook &rest args)
  "Run HOOK with ARGS, setting HOOK to nil if a function signals an error.
This is insdel.c's `reset_var_on_error' around `Frun_hook_with_args'."
  `(let ((done nil))
     (unwind-protect
         (progn (run-hook-with-args ',hook ,@args)
                (setq done t))
       (unless done
         (setq ,hook nil)))))

(defvar editfns--interval-insert-behind-hooks nil
  "textprop.c `interval_insert_behind_hooks', chosen before an insertion.")
(defvar editfns--interval-insert-in-front-hooks nil
  "textprop.c `interval_insert_in_front_hooks', chosen before an insertion.")
(defvar editfns--last-overlay-modification-hooks nil
  "buffer.c `last_overlay_modification_hooks': (FUNCTIONS . OVERLAY) pairs
collected before a change, run again after it.")

(defun editfns--textget (pos prop)
  "textget: PROP of the character at POS, through its `category'."
  (let ((plist (text-properties-at pos)))
    (if (plist-member plist prop)
        (plist-get plist prop)
      (let ((cat (plist-get plist 'category)))
        (and cat (symbolp cat) (get cat prop))))))

(defun editfns--text-read-only (propval)
  "text_read_only: signal `text-read-only', with PROPVAL when it is a string."
  (signal 'text-read-only (and (stringp propval) (list propval))))

(defun editfns--tmem (sym set)
  "TMEM: SET is t, or a list containing SYM."
  (if (consp set) (memq sym set) set))

(defun editfns--verify-interval-modification (start end)
  "textprop.c `verify_interval_modification' for the text START..END:
signal `text-read-only' for read-only text, run the `modification-hooks'
text properties of a changed span, and choose the `insert-behind-hooks' and
`insert-in-front-hooks' an insertion runs afterwards."
  (setq editfns--interval-insert-behind-hooks nil
        editfns--interval-insert-in-front-hooks nil)
  (when (> start end)
    (setq start (prog1 end (setq end start))))
  (if (= start end)
      (let* ((has-after (< start (point-max)))
             (has-before (> start (point-min)))
             (same (and has-after has-before (elisprs--inside-interval-p start))))
        (when (or (null inhibit-read-only) (consp inhibit-read-only))
          (let ((after (and has-after (editfns--textget start 'read-only)))
                (before (and has-before (editfns--textget (1- start) 'read-only))))
            (if (not same)
                (progn
                  (when (and after (not (memq after inhibit-read-only)))
                    (let ((tem (editfns--textget start 'front-sticky)))
                      (when (or (editfns--tmem 'read-only tem)
                                (and (null (plist-get (text-properties-at start) 'read-only))
                                     (editfns--tmem 'category tem)))
                        (editfns--text-read-only after))))
                  (when (and before (not (memq before inhibit-read-only)))
                    (let ((tem (editfns--textget (1- start) 'rear-nonsticky)))
                      (when (and (not (editfns--tmem 'read-only tem))
                                 (or (plist-get (text-properties-at (1- start)) 'read-only)
                                     (not (editfns--tmem 'category tem))))
                        (editfns--text-read-only before)))))
              (when (and after (not (memq after inhibit-read-only)))
                (let ((tem (editfns--textget start 'front-sticky)))
                  (when (or (editfns--tmem 'read-only tem)
                            (and (null (plist-get (text-properties-at start) 'read-only))
                                 (editfns--tmem 'category tem)))
                    (editfns--text-read-only after))
                  (setq tem (editfns--textget (1- start) 'rear-nonsticky))
                  (when (and (not (editfns--tmem 'read-only tem))
                             (or (plist-get (text-properties-at (1- start)) 'read-only)
                                 (not (editfns--tmem 'category tem))))
                    (editfns--text-read-only after)))))))
        (when has-before
          (setq editfns--interval-insert-behind-hooks
                (editfns--textget (1- start) 'insert-behind-hooks)))
        (when has-after
          (setq editfns--interval-insert-in-front-hooks
                (editfns--textget start 'insert-in-front-hooks))))
    (let ((buffer-ro (and buffer-read-only (null inhibit-read-only)))
          (p start) (last 0) (hooks nil) (prev-mod-hooks nil))
      (while (progn
               (let* ((read-only (editfns--textget p 'read-only))
                      (implied (or (null read-only)
                                   (and inhibit-read-only (not (consp inhibit-read-only)))))
                      (express (or (editfns--textget p 'inhibit-read-only)
                                   (and read-only (consp inhibit-read-only)
                                        (memq read-only inhibit-read-only)))))
                 (cond ((and (not implied) (not express))
                        (editfns--text-read-only read-only))
                       ((and (not express) buffer-ro)
                        (signal 'buffer-read-only (list (current-buffer)))))
                 (unless inhibit-modification-hooks
                   (let ((mod-hooks (editfns--textget p 'modification-hooks)))
                     (when (and mod-hooks (not (eq mod-hooks prev-mod-hooks)))
                       (push mod-hooks hooks)
                       (setq prev-mod-hooks mod-hooks))))
                 (setq last (or (next-property-change p) (point-max))
                       p last))
               (< p end)))
      (when (and (< last end) buffer-ro)
        (signal 'buffer-read-only (list (current-buffer))))
      (unless inhibit-modification-hooks
        (let ((inhibit-modification-hooks t))
          (dolist (h (nreverse hooks))
            (dolist (fn h)
              (funcall fn start end))))))))

(defun editfns--report-overlay-modification (beg end after &optional len)
  "buffer.c `report_overlay_modification'.  Before a change of BEG..END,
collect the `insert-in-front-hooks', `insert-behind-hooks' and
`modification-hooks' of the overlays it touches; before and after, call
them with (OVERLAY AFTER BEG END [LEN])."
  (unless after
    (let ((insertion (= beg end)) (collected nil))
      (dolist (ov (sort (overlays-in (point-min) (point-max))
                        (lambda (a b) (< (overlay-start a) (overlay-start b)))))
        (let ((ob (overlay-start ov)) (oe (overlay-end ov)))
          (when (and insertion (or (= beg ob) (= end ob)))
            (let ((prop (overlay-get ov 'insert-in-front-hooks)))
              (when prop (push (cons prop ov) collected))))
          (when (and insertion (or (= beg oe) (= end oe)))
            (let ((prop (overlay-get ov 'insert-behind-hooks)))
              (when prop (push (cons prop ov) collected))))
          (when (and (> end ob) (< beg oe))
            (let ((prop (overlay-get ov 'modification-hooks)))
              (when prop (push (cons prop ov) collected))))))
      (setq editfns--last-overlay-modification-hooks (nreverse collected))))
  (dolist (entry (copy-sequence editfns--last-overlay-modification-hooks))
    (let ((ov (cdr entry)))
      (when (eq (overlay-buffer ov) (current-buffer))
        (dolist (fn (car entry))
          (if len
              (funcall fn ov after beg end len)
            (funcall fn ov after beg end)))))))

(defun elisprs--prepare-to-modify (beg end &optional buffer)
  "insdel.c `prepare_to_modify_buffer' from the undo hook on: the text from
BEG to END of BUFFER is about to change."
  (with-current-buffer (or buffer (current-buffer))
    (unless (eq buffer-undo-list t)
      (undo-auto--undoable-change))
    (editfns--verify-interval-modification beg end)
    (unless inhibit-modification-hooks
      (elisprs--signal-before-change beg end))))

(defun elisprs--signal-before-change (beg end &optional buffer)
  "insdel.c `signal_before_change' for the text from BEG to END of BUFFER.
`first-change-hook' runs first when the buffer is unmodified, then
`before-change-functions', then the overlays' hooks, with
`inhibit-modification-hooks' bound to t."
  (with-current-buffer (or buffer (current-buffer))
    (let ((inhibit-modification-hooks t))
      (when (and first-change-hook (not (buffer-modified-p)))
        (run-hooks 'first-change-hook))
      (when before-change-functions
        (editfns--run-change-hook before-change-functions beg end))
      (when (overlays-in (point-min) (point-max))
        (editfns--report-overlay-modification beg end nil)))))

(defun elisprs--signal-after-change (beg end oldlen &optional buffer)
  "insdel.c `signal_after_change': the text from BEG to END of BUFFER replaced
OLDLEN characters.  `after-change-functions', the overlays' hooks and, after
an insertion, the inserted text's `insert-behind-hooks' and
`insert-in-front-hooks' run, with `inhibit-modification-hooks' bound to t."
  (with-current-buffer (or buffer (current-buffer))
    (unless inhibit-modification-hooks
      (let ((inhibit-modification-hooks t)
            (behind editfns--interval-insert-behind-hooks)
            (in-front editfns--interval-insert-in-front-hooks))
        (when after-change-functions
          (editfns--run-change-hook after-change-functions beg end oldlen))
        (when (overlays-in (point-min) (point-max))
          (editfns--report-overlay-modification beg end t oldlen))
        (when (= oldlen 0)
          (dolist (fn behind)
            (funcall fn beg end))
          (unless (eq in-front behind)
            (dolist (fn in-front)
              (funcall fn beg end))))))))

;;;; filelock.c

(defun unlock-buffer ()
  "Unlock the file visited in the current buffer.
There are no file locks here (no buffer's file is ever locked), so this
does nothing and returns nil."
  nil)

(defun set-buffer-major-mode (buffer)
  "Set an appropriate major mode for BUFFER.
For the *scratch* buffer, use `initial-major-mode', otherwise choose a mode
according to the default value of `major-mode'.
Use this function before selecting the buffer, since it may need to inspect
the current buffer's major mode."
  (unless (bufferp buffer)
    (signal 'wrong-type-argument (list 'bufferp buffer)))
  (let ((function
         (if (equal (buffer-name buffer) "*scratch*")
             (and (boundp 'initial-major-mode) initial-major-mode)
           (let ((default (default-value 'major-mode)))
             (if (and (null default) (null (get major-mode 'mode-class)))
                 major-mode
               default)))))
    (when function
      (with-current-buffer buffer
        (funcall function)))))

;;;; window.c / buffer.c variables

(defvar window-point-insertion-type nil
  "Insertion type of marker to use for `window-point'.")
(defvar buffer-list-update-hook nil
  "Hook run when the buffer list changes.")

(defun delete-auto-save-file-if-necessary (&optional force)
  "Delete auto-save file for current buffer if `delete-auto-save-files' is t.
Normally delete only if the file was written by this Emacs since
the last real save, but optional arg FORCE non-nil means delete anyway.
No buffer here has an auto-save file (`buffer-auto-save-file-name' is
never set), so there is nothing to delete."
  (ignore force)
  nil)

;;;; dispnew.c

(defun redisplay (&optional force)
  "Perform redisplay.
There is no display in batch mode; the value is t, as Emacs answers there."
  (ignore force)
  t)

;;;; lread.c — `eval-buffer' and `eval-region' (readevalloop).

(defvar eval-buffer-list nil
  "List of buffers being read from by calls to `eval-buffer' and `eval-region'.")
;; `values' is a global C variable that is not special (`special-variable-p'
;; is nil for it), so a `let' of it in lexical code does not catch the values.
(setq values nil)
(defvar load-read-function 'read
  "Function used for reading expressions.")

(defun editfns--lexical-cookie (buffer)
  "lisp_file_lexical_cookie: `lex', `dyn' or nil from BUFFER's first line
\(after a `#!' line), which must be a comment."
  (with-current-buffer buffer
    (save-excursion
      (save-restriction
        (widen)
        (goto-char (point-min))
        (when (looking-at "#!")
          (forward-line 1))
        (when (and (eq (char-after) ?\;)
                   (looking-at
                    "[^\n]*-\\*-[^\n]*lexical-binding:[ \t]*\\([^ \t\n;]+\\)[^\n]*-\\*-"))
          (if (equal (match-string 1) "nil") 'dyn 'lex))))))

(defun editfns--next-sexp-p (buffer)
  "readevalloop's skip: move BUFFER's point past whitespace and `;'
comments; non-nil when an expression follows."
  (with-current-buffer buffer
    (let ((done nil))
      (while (not done)
        (skip-chars-forward " \t\n\f\r ")
        (if (eq (char-after) ?\;)
            (progn (skip-chars-forward "^\n") (unless (eobp) (forward-char 1)))
          (setq done t)))
      (not (eobp)))))

(defun editfns--readevalloop (buffer printflag readfun start end)
  "lread.c `readevalloop' over BUFFER.  Without START, read from BUFFER's
point; with START, from START (narrowed to END), restoring the current
buffer, point and restriction around each read.  Each form is evaluated
with the lexical environment `lexical-binding' asked for at the start, and
with PRINTFLAG pushed onto `values' and printed."
  (let ((lexical lexical-binding)
        (standard-input buffer)
        (continue t)
        (position start))
    (while continue
      (unless (buffer-live-p buffer)
        (error "Reading from killed buffer"))
      (let ((form
             (catch 'editfns--readevalloop-done
               (if position
                   (save-excursion
                     (set-buffer buffer)
                     (save-excursion
                       (save-restriction
                         (goto-char position)
                         (when end
                           (narrow-to-region (point-min) end))
                         (unless (editfns--next-sexp-p buffer)
                           (throw 'editfns--readevalloop-done 'editfns--eof))
                         (prog1 (if readfun (funcall readfun buffer)
                                  (funcall (or load-read-function #'read) buffer))
                           (setq position (point-marker))))))
                 (unless (editfns--next-sexp-p buffer)
                   (throw 'editfns--readevalloop-done 'editfns--eof))
                 (if readfun
                     (prog1 (funcall readfun buffer)
                       (with-current-buffer buffer
                         (when (= (point) (point-max))
                           (setq continue nil))))
                   (funcall (or load-read-function #'read) buffer))))))
        (if (eq form 'editfns--eof)
            (setq continue nil)
          (let ((val (eval form lexical)))
            (when printflag
              (push val values)
              (if (eq standard-output t)
                  (prin1 val)
                (print val)))))))))

(defun eval-buffer (&optional buffer printflag filename unibyte do-allow-print)
  "Execute the accessible portion of current buffer as Lisp code.
You can use \\[narrow-to-region] to limit the part of buffer to be evaluated.
When called from a Lisp program (i.e., not interactively), this
function accepts up to five optional arguments:
BUFFER is the buffer to evaluate (nil means use current buffer),
 or a name of a buffer (a string).
PRINTFLAG controls printing of output by any output functions in the
 evaluated code, such as `print', `princ', and `prin1':
  a value of nil means discard it; anything else is the stream to print to.
FILENAME specifies the file name to use for `load-history'.
UNIBYTE, if non-nil, specifies `load-convert-to-unibyte' for this
 invocation.
DO-ALLOW-PRINT, if non-nil, specifies that output functions in the
 evaluated code should work normally even if PRINTFLAG is nil, in
 which case the output is displayed in the echo area.

This function preserves the position of point."
  (ignore filename unibyte)
  (let ((buf (if buffer (get-buffer buffer) (current-buffer))))
    (unless buf
      (error "No such buffer"))
    (let* ((cookie (editfns--lexical-cookie buf))
           (eval-buffer-list (cons buf eval-buffer-list))
           (standard-output (if (and (null printflag) (null do-allow-print))
                                'symbolp
                              printflag))
           (body (lambda ()
                   (let ((saved (with-current-buffer buf (point))))
                     (unwind-protect
                         (progn
                           (with-current-buffer buf (goto-char (point-min)))
                           (editfns--readevalloop buf printflag nil nil nil))
                       (when (buffer-live-p buf)
                         (with-current-buffer buf (goto-char saved))))))))
      (if (local-variable-p 'lexical-binding buf)
          (funcall body)
        (let ((lexical-binding (cond ((eq cookie 'lex) t)
                                     ((eq cookie 'dyn) nil)
                                     (t (default-toplevel-value 'lexical-binding)))))
          (funcall body)))
      nil)))

(defun eval-region (start end &optional printflag read-function)
  "Execute the region as Lisp code.
When called from programs, expects two arguments,
giving starting and ending indices in the current buffer
of the text to be executed.
Programs can pass third argument PRINTFLAG which controls output:
 a value of nil means discard it; anything else is stream for printing it.
 See Info node `(elisp)Output Streams' for details on streams.
Also the fourth argument READ-FUNCTION, if non-nil, is used
instead of `read' to read each expression.  It gets one argument
which is the input stream for reading characters.

This function does not move point."
  (let ((cbuf (current-buffer)))
    (let ((standard-output (if (null printflag) 'symbolp printflag))
          (eval-buffer-list (cons cbuf eval-buffer-list)))
      (editfns--readevalloop cbuf printflag read-function start
                             (and end (if (integerp end)
                                          (copy-marker end)
                                        end)))
      nil)))
