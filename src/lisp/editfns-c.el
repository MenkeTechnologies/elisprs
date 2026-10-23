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


;;;; insdel.c — the change hook variables.  Insertions and deletions do not
;;;; run them yet (see BUGS.md); they exist so the code that binds and
;;;; inspects them, like subr.el's `combine-change-calls-1', runs.

(defvar before-change-functions nil
  "List of functions to call before each text change.")
(defvar after-change-functions nil
  "List of functions to call after each text change.")
(defvar first-change-hook nil
  "A list of functions to call before changing a buffer which is unmodified.")
