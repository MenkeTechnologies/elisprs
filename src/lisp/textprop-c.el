;;; textprop-c.el --- textprop.c / editfns.c queries that read overlays  -*- lexical-binding: t -*-

;; Ports of the C functions that combine a position's overlays with its text
;; properties.  The interval walks themselves (`next-property-change' & co.)
;; are subrs in src/builtins.rs.

(defun get-char-property-and-overlay (position prop &optional object)
  "Return the value of POSITION's property PROP, in OBJECT, with its overlay.
Return a cons (VALUE . OVERLAY), where OVERLAY is the overlay the value came
from, or nil when it is a text property.  Overlays take precedence over text
properties, and among overlays the one `overlays-at' sorts first wins."
  (if (and (bufferp (or object (current-buffer)))
           (not (stringp object)))
      (let ((result nil))
        (with-current-buffer (or object (current-buffer))
          (unless (<= (point-min) position (point-max))
            (signal 'args-out-of-range (list position)))
          (dolist (ov (overlays-at position t))
            (let ((val (overlay-get ov prop)))
              (when (and val (not result) (>= (overlay-end ov) (1+ position)))
                (setq result (cons val ov))))))
        (or result (cons (get-text-property position prop object) nil)))
    (cons (get-text-property position prop object) nil)))

(defun get-char-property (position prop &optional object)
  "Return the value of POSITION's property PROP, in OBJECT.
Both overlay properties and text properties are checked; overlay properties
take precedence."
  (car (get-char-property-and-overlay position prop object)))

(defun next-char-property-change (position &optional limit)
  "Return the position of next text property or overlay change.
This scans characters forward in the current buffer from POSITION till
it finds a change in some text property, or the beginning or end of an
overlay, and returns the position of that.
If none is found, and LIMIT is nil, return the end of the accessible
part of the buffer.  If LIMIT is non-nil, return LIMIT in that case."
  (let ((temp (next-overlay-change position)))
    (when (and limit (< limit temp))
      (setq temp limit))
    (next-property-change position nil temp)))

(defun previous-char-property-change (position &optional limit)
  "Return the position of previous text property or overlay change.
Scans characters backward in the current buffer from POSITION till it
finds a change in some text property, or the beginning or end of an
overlay, and returns the position of that.
If none is found, and LIMIT is nil, return the beginning of the
accessible part of the buffer.  If LIMIT is non-nil, return LIMIT
in that case."
  (let ((temp (previous-overlay-change position)))
    (when (and limit (> limit temp))
      (setq temp limit))
    (previous-property-change position nil temp)))

(defun next-single-char-property-change (position prop &optional object limit)
  "Return the position of next text property or overlay change for a specific property.
Scans characters forward from POSITION till it finds
a change in the PROP property, then returns the position of the change.
If the optional third argument OBJECT is a buffer (or nil, which means
the current buffer), POSITION is a buffer position (integer or marker).
If OBJECT is a string, POSITION is a 0-based index into it.
If the property is constant all the way to the end of OBJECT, return the
last valid position in OBJECT.
If the optional fourth argument LIMIT is non-nil, don't search
past position LIMIT; return LIMIT if nothing is found before LIMIT."
  (if (stringp object)
      (let ((pos (next-single-property-change position prop object limit)))
        (cond (pos pos)
              ((null limit) (length object))
              (t (unless (integerp limit)
                   (signal 'wrong-type-argument (list 'fixnump limit)))
                 limit)))
    (with-current-buffer (or object (current-buffer))
      (setq position (if (markerp position) (marker-position position) position))
      (let ((initial (get-char-property position prop object))
            (lim (cond ((null limit) (point-max))
                       ((markerp limit) (marker-position limit))
                       (t limit)))
            (done nil))
        (if (>= position lim)
            (setq position (min lim (point-max)))
          (while (not done)
            (setq position (next-char-property-change position lim))
            (cond ((>= position lim) (setq position lim done t))
                  ((not (eq (get-char-property position prop object) initial))
                   (setq done t))
                  ((>= position (point-max)) (setq done t)))))
        position))))

(defun previous-single-char-property-change (position prop &optional object limit)
  "Return the position of previous text property or overlay change for a specific property.
Scans characters backward from POSITION till it finds
a change in the PROP property, then returns the position of the change.
If the optional third argument OBJECT is a buffer (or nil, which means
the current buffer), POSITION is a buffer position (integer or marker).
If OBJECT is a string, POSITION is a 0-based index into it.
If the property is constant all the way to the start of OBJECT, return the
first valid position in OBJECT.
If the optional fourth argument LIMIT is non-nil, don't search back past
position LIMIT; return LIMIT if nothing is found before reaching LIMIT."
  (if (stringp object)
      (let ((pos (previous-single-property-change position prop object limit)))
        (cond (pos pos)
              ((null limit) 0)
              (t limit)))
    (with-current-buffer (or object (current-buffer))
      (setq position (if (markerp position) (marker-position position) position))
      (let ((lim (cond ((null limit) (point-min))
                       ((markerp limit) (marker-position limit))
                       (t limit)))
            (done nil))
        (if (<= position lim)
            (setq position (max lim (point-min)))
          (let ((initial (get-char-property (1- position) prop object)))
            (while (not done)
              (setq position (previous-char-property-change position lim))
              (cond ((<= position lim) (setq position lim done t))
                    ((not (eq (get-char-property (1- position) prop object)
                              initial))
                     (setq done t))
                    ((<= position (point-min)) (setq done t))))))
        position))))

(defun keymap--text-property-stickiness (prop pos buffer)
  "text_property_stickiness: -1 rear-sticky, 1 front-sticky, 0 neither."
  (let* ((prev (1- pos))
         (defalt (assq prop text-property-default-nonsticky))
         (ignore-previous (<= pos (with-current-buffer buffer (point-min))))
         (rear t) (front nil))
    (if (or ignore-previous (and (consp defalt) (cdr defalt)))
        (setq rear nil)
      (let ((rns (get-text-property prev 'rear-nonsticky buffer)))
        (when (if (consp rns) (memq prop rns) rns)
          (setq rear nil))))
    (let ((fs (get-text-property pos 'front-sticky buffer)))
      (when (or (eq fs t) (and (consp fs) (memq prop fs)))
        (setq front t)))
    (cond ((and rear (not front)) -1)
          ((and (not rear) front) 1)
          ((and (not rear) (not front)) 0)
          ((or ignore-previous (null (get-text-property prev prop buffer))) 1)
          (t -1))))

(defun get-pos-property (position prop &optional object)
  "Return the value of POSITION's property PROP, in OBJECT.
Almost identical to `get-char-property' except for the following difference:
Whereas `get-char-property' returns the property of the char at (i.e. right
after) POSITION, this pays attention to properties's stickiness and overlays's
advancement settings, in order to find the property of POSITION itself,
i.e. the property that a char would inherit if it were inserted
at POSITION."
  (if (or (stringp object) (not (bufferp (or object (current-buffer)))))
      (get-text-property position prop object)
    (let ((buf (or object (current-buffer))) (result nil) (found nil))
      (with-current-buffer buf
        (setq position (if (markerp position) (marker-position position) position))
        (unless (<= (point-min) position (point-max))
          (signal 'args-out-of-range (list position)))
        (dolist (ov (overlays-in (max (point-min) (if (< position (point-max)) position (1- position)))
                                 (min (point-max) (if (> position (point-min)) position (1+ position)))))
          (let ((val (overlay-get ov prop)))
            (when (and val (not found)
                       (<= (overlay-start ov) position)
                       (>= (overlay-end ov) position)
                       ;; (rear-advance overlays are not reachable from Lisp)
                       (not (= (overlay-end ov) position)))
              (setq found t result val))))
        (if found
            result
          (let ((stickiness (keymap--text-property-stickiness prop position buf)))
            (cond ((> stickiness 0) (get-text-property position prop buf))
                  ((and (< stickiness 0) (> position (point-min)))
                   (get-text-property (1- position) prop buf))
                  (t nil))))))))

;;;; Stickiness (intervals.c).

(defun keymap--tmem (sym set)
  (if (consp set) (and (memq sym set) t) (and set t)))

(defun merge-properties-sticky (pleft pright)
  "merge_properties_sticky: the properties a character inserted between PLEFT
and PRIGHT inherits, with the `front-sticky' / `rear-nonsticky' bookkeeping."
  (let ((props nil) (front nil) (rear nil)
        (lfront (plist-get pleft 'front-sticky))
        (lrear (plist-get pleft 'rear-nonsticky))
        (rfront (plist-get pright 'front-sticky))
        (rrear (plist-get pright 'rear-nonsticky)))
    ;; Each property of PRIGHT.
    (let ((tail1 pright))
      (while (consp tail1)
        (let ((sym (car tail1)))
          (unless (memq sym '(rear-nonsticky front-sticky))
            (let* ((rval (car (cdr tail1)))
                   (tail2 (plist-member pleft sym))
                   (lpresent (and tail2 t))
                   (lval (and tail2 (car (cdr tail2))))
                   (tmp (assq sym text-property-default-nonsticky))
                   (use-left (and lpresent
                                  (not (or (keymap--tmem sym lrear)
                                           (and (consp tmp) (cdr tmp))))))
                   (use-right (or (keymap--tmem sym rfront)
                                  (and (consp tmp) (null (cdr tmp))))))
              (when (and use-left use-right)
                (cond ((null lval) (setq use-left nil))
                      ((null rval) (setq use-right nil))))
              (cond
               (use-left
                (setq props (cons lval (cons sym props)))
                (when (keymap--tmem sym lfront) (push sym front))
                (when (keymap--tmem sym lrear) (push sym rear)))
               (use-right
                (setq props (cons rval (cons sym props)))
                (when (keymap--tmem sym rfront) (push sym front))
                (when (keymap--tmem sym rrear) (push sym rear)))))))
        (setq tail1 (cdr (cdr tail1)))))
    ;; Each property of PLEFT.
    (let ((tail2 pleft))
      (while (consp tail2)
        (let ((sym (car tail2)))
          (unless (or (memq sym '(rear-nonsticky front-sticky))
                      (plist-member pright sym))
            (let ((lval (car (cdr tail2)))
                  (tmp (assq sym text-property-default-nonsticky)))
              (cond
               ((not (or (keymap--tmem sym lrear) (and (consp tmp) (cdr tmp))))
                (setq props (cons lval (cons sym props)))
                (when (keymap--tmem sym lfront) (push sym front)))
               ((or (keymap--tmem sym rfront) (and (consp tmp) (null (cdr tmp))))
                (push sym front)
                (when (keymap--tmem sym rrear) (push sym rear)))))))
        (setq tail2 (cdr (cdr tail2)))))
    (setq props (nreverse props))
    (when rear
      (setq props (cons 'rear-nonsticky (cons (nreverse rear) props))))
    (let ((cat (plist-get props 'category)))
      (when (and front
                 (not (and cat (symbolp cat) (eq (get cat 'front-sticky) t))))
        (setq props (cons 'front-sticky (cons (nreverse front) props)))))
    props))

(defun insert-and-inherit (&rest args)
  "Insert the arguments at point, inheriting properties from adjoining text.
Point and after-insertion markers move forward to end up after the
inserted text.
Any other markers at the point of insertion remain before the text.

If the current buffer is multibyte, unibyte strings are converted
to multibyte for insertion (see `string-make-multibyte').
If the current buffer is unibyte, multibyte strings are converted
to unibyte for insertion (see `string-make-unibyte').

When operating on binary data, it may be necessary to preserve the
original bytes of a unibyte string when inserting it into a multibyte
buffer; to accomplish this, first convert the string to unibyte with
`string-as-unibyte' and then insert it."
  (dolist (arg args)
    (let* ((start (point))
           (inside (elisprs--inside-interval-p start))
           (pleft (and (> start (point-min)) (text-properties-at (1- start))))
           (pright (and (< start (point-max)) (text-properties-at start))))
      (insert arg)
      ;; `merge_properties (source, target)' in the primitive: the inherited
      ;; value wins for a property the inserted text also carried.
      (elisprs--inherit-props
       start (point)
       (if inside (or pleft pright) (merge-properties-sticky pleft pright))
       pleft pright inside)))
  nil)
