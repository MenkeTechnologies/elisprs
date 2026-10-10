(defun error--p (object)
  "Return non-nil if OBJECT looks like a valid error descriptor."
  (let ((type (car-safe object)))
    (and type (symbolp type) (listp (cdr object))
         (error-type-p type))))


;;; Verbatim definitions from Emacs 31.1 lisp/subr.el, lisp/simple.el,
;;; lisp/subr-x.el, lisp/indent.el and lisp/font-lock.el that the prelude lacked. -*- lexical-binding: t -*-

(defun take-while (pred list)
  "Return the longest prefix of LIST whose elements satisfy PRED."
  (declare (compiler-macro
            (lambda (form)
              (let* ((tail (make-symbol "tail"))
                     (r (make-symbol "r")))
                (if (not (internal--effect-free-fun-arg-p pred))
                    ;; Don't inline since it would just duplicate the code
                    ;; without allowing any more optimizations.
                    form
                  `(let ((,r nil)
                         (,tail ,list))
                     (while (and ,tail (funcall ,pred (car ,tail)))
                       (push (car ,tail) ,r)
                       (setq ,tail (cdr ,tail)))
                     (nreverse ,r)))))))
  (let ((r nil))
    (while (and list (funcall pred (car list)))
      (push (car list) r)
      (setq list (cdr list)))
    (nreverse r)))


(defun drop-while (pred list)
  "Skip initial elements of LIST satisfying PRED and return the rest."
  (declare (compiler-macro
            (lambda (form)
              (let* ((tail (make-symbol "tail")))
                (if (not (internal--effect-free-fun-arg-p pred))
                    ;; Don't inline since it would just duplicate the code
                    ;; without allowing any more optimizations.
                    form
                  `(let ((,tail ,list))
                     (while (and ,tail (funcall ,pred (car ,tail)))
                       (setq ,tail (cdr ,tail)))
                     ,tail))))))
  (while (and list (funcall pred (car list)))
    (setq list (cdr list)))
  list)


(defun member-if (pred list)
  "Non-nil if PRED is true for at least one element in LIST.
Returns the suffix of LIST starting with the first element that
satisfies PRED, or nil if none do.

Compatibility note: this function replaces `cl-member-if' but does not
support the latter's `:key KEY-FN' argument.  It is better to compose
any KEY-FN into PRED.  For example, you can replace

    (cl-member-if #\\='foo items :key #\\='bar)

with

    (member-if (lambda (x) (foo (bar x))) items)"
  (declare (compiler-macro
            (lambda (form)
              (if (not (internal--effect-free-fun-arg-p pred))
                  ;; Don't inline since it would just duplicate the code
                  ;; without allowing any more optimizations.
                  form
                (let* ((x (make-symbol "x")))
                  `(drop-while (lambda (,x)
                                 (not (funcall ,pred ,x)))
                               ,list))))))
  (drop-while (lambda (x) (not (funcall pred x))) list))


;; This is good to have for improved readability in certain uses, but
;; use the traditional Lisp name for the underlying function.  --spwhitton
(defalias 'any #'member-if)


(defun all (pred list)
  "Non-nil if PRED is true for all elements in LIST."
  (declare (compiler-macro
            (lambda (form)
              (if (not (internal--effect-free-fun-arg-p pred))
                  ;; Don't inline since it would just duplicate the code
                  ;; without allowing any more optimizations.
                  form
                `(not (drop-while ,pred ,list))))))
  (not (drop-while pred list)))


;; See https://lists.gnu.org/r/emacs-devel/2013-05/msg00204.html
(defun delete-consecutive-dups (list &optional circular)
  "Destructively remove `equal' consecutive duplicates from LIST.
First and last elements are considered consecutive if CIRCULAR is
non-nil.
Of several consecutive `equal' occurrences, the one earliest in
the list is kept."
  (let ((tail list) last)
    (while (cdr tail)
      (if (equal (car tail) (cadr tail))
	  (setcdr tail (cddr tail))
	(setq last tail
	      tail (cdr tail))))
    (if (and circular
	     last
	     (equal (car tail) (car list)))
	(setcdr last nil)))
  list)


(defalias 'drop #'nthcdr)

;; Note: `static-if' can be copied into a package to enable it to be
;; used in Emacsen older than Emacs 30.1.  If the package is used in
;; very old Emacsen or XEmacs (in which `eval' takes exactly one
;; argument) the copy will need amending.
(defmacro static-if (condition then-form &rest else-forms)
  "A conditional compilation macro.
Evaluate CONDITION at macro-expansion time.  If it is non-nil,
expand the macro to THEN-FORM.  Otherwise expand it to ELSE-FORMS
enclosed in a `progn' form.  ELSE-FORMS may be empty."
  (declare (indent 2)
           (debug (sexp sexp &rest sexp)))
  (if (eval condition lexical-binding)
      then-form
    (cons 'progn else-forms)))


(defmacro static-when (condition &rest body)
  "A conditional compilation macro.
Evaluate CONDITION at macro-expansion time.  If it is non-nil,
expand the macro to evaluate all BODY forms sequentially and return
the value of the last one, or nil if there are none."
  (declare (indent 1) (debug t))
  (if body
      (if (eval condition lexical-binding)
          (cons 'progn body)
        nil)
    (macroexp-warn-and-return (format-message "`static-when' with empty body")
                              nil '(empty-body static-when) t
                              condition)))


(defmacro static-unless (condition &rest body)
  "A conditional compilation macro.
Evaluate CONDITION at macro-expansion time.  If it is nil,
expand the macro to evaluate all BODY forms sequentially and return
the value of the last one, or nil if there are none."
  (declare (indent 1) (debug t))
  (if body
      (if (eval condition lexical-binding)
          nil
        (cons 'progn body))
    (macroexp-warn-and-return (format-message "`static-unless' with empty body")
                              (list 'progn nil nil) '(empty-body static-unless) t)))


(defun error-type-p (symbol)
  "Return non-nil if SYMBOL is a condition type."
  (get symbol 'error-conditions))


(defun error-has-type-p (error condition)
  "Return non-nil if ERROR is of type CONDITION (or a subtype of it)."
  (unless (error--p error)
    (signal 'wrong-type-argument (list #'error--p error)))
  (or (eq condition t)
      (memq condition (get (car error) 'error-conditions))))


(defalias 'error-slot-value #'elt
  "Access the SLOT of object ERROR.
Slots are specified by position, and slot 0 is the error symbol.
\n(fn ERROR SLOT)")


(defalias 'error-type #'car
 "Return the symbol which represents the type of ERROR.
\n(fn ERROR)")


(defun ensure-proper-list (object)
  "Return OBJECT as a list.
If OBJECT is already a proper list, return OBJECT itself.  If it's not a
proper list, return a one-element list containing OBJECT.

`ensure-list' is usually preferable because that function runs in
constant time, but this one has to traverse the whole of OBJECT."
  (declare (side-effect-free error-free))
  (if (proper-list-p object)
      object
    (list object)))


(defun readablep (object)
  "Say whether OBJECT has a readable syntax.
This means that OBJECT can be printed out and then read back
again by the Lisp reader.  This function returns nil if OBJECT is
unreadable, and the printed representation (from `prin1') of
OBJECT if it is readable."
  (declare (side-effect-free error-free))
  (catch 'unreadable
    (let ((print-unreadable-function
           (lambda (_object _escape)
             (throw 'unreadable nil))))
      (prin1-to-string object))))


(defun list-of-strings-p (object)
  "Return t if OBJECT is nil or a list of strings."
  (declare (pure t) (side-effect-free error-free))
  (while (and (consp object) (stringp (car object)))
    (setq object (cdr object)))
  (null object))


(defun integer-or-null-p (object)
  "Return non-nil if OBJECT is either an integer or nil.
Otherwise, return nil."
  (declare (pure t) (side-effect-free error-free))
  (or (integerp object) (null object)))


(defalias 'int-to-string #'number-to-string)

(defun log10 (x)
  "Return (log X 10), the log base 10 of X."
  (declare (ftype (function (number) float))
           (side-effect-free t) (obsolete log "24.4"))
  (log x 10))


(defun delete-line ()
  "Delete the current line."
  (delete-region (pos-bol) (pos-bol 2)))


(defun buffer-local-boundp (symbol buffer)
  "Return non-nil if SYMBOL is bound in BUFFER.
Also see `local-variable-p'."
  (declare (side-effect-free t))
  (condition-case nil
      (buffer-local-value symbol buffer)
    (:success t)
    (void-variable nil)))


(defun match-substitute-replacement (replacement
				     &optional fixedcase literal string subexp)
  "Return REPLACEMENT as it will be inserted by `replace-match'.
In other words, all back-references in the form `\\&' and `\\N'
are substituted with actual strings matched by the last search.
Optional FIXEDCASE, LITERAL, STRING and SUBEXP have the same
meaning as for `replace-match'."
  (declare (side-effect-free t))
  (let ((match (match-string 0 string)))
    (save-match-data
      (match-data--translate (- (match-beginning 0)))
      (replace-match replacement fixedcase literal match subexp))))



(defun subregexp-context-p (regexp pos &optional start)
  "Return non-nil if POS is in a normal subregexp context in REGEXP.
A subregexp context is one where a sub-regexp can appear.
A non-subregexp context is for example within brackets, or within a
repetition bounds operator `\\=\\{...\\}', or right after a `\\'.
If START is non-nil, it should be a position in REGEXP, smaller
than POS, and known to be in a subregexp context."
  (declare (important-return-value t))
  ;; Here's one possible implementation, with the great benefit that it
  ;; reuses the regexp-matcher's own parser, so it understands all the
  ;; details of the syntax.  A disadvantage is that it needs to match the
  ;; error string.
  (condition-case err
      (progn
        (string-match (substring regexp (or start 0) pos) "")
        t)
    (invalid-regexp
     (not (member (cadr err) '("Unmatched [ or [^"
                               "Unmatched \\{"
                               "Trailing backslash")))))
  ;; An alternative implementation:
  ;; (defconst re-context-re
  ;;   (let* ((harmless-ch "[^\\[]")
  ;;          (harmless-esc "\\\\[^{]")
  ;;          (class-harmless-ch "[^][]")
  ;;          (class-lb-harmless "[^]:]")
  ;;          (class-lb-colon-maybe-charclass ":\\([a-z]+:]\\)?")
  ;;          (class-lb (concat "\\[\\(" class-lb-harmless
  ;;                            "\\|" class-lb-colon-maybe-charclass "\\)"))
  ;;          (class
  ;;           (concat "\\[^?]?"
  ;;                   "\\(" class-harmless-ch
  ;;                   "\\|" class-lb "\\)*"
  ;;                   "\\[?]"))     ; special handling for bare [ at end of re
  ;;          (braces "\\\\{[0-9,]+\\\\}"))
  ;;     (concat "\\`\\(" harmless-ch "\\|" harmless-esc
  ;;             "\\|" class "\\|" braces "\\)*\\'"))
  ;;   "Matches any prefix that corresponds to a normal subregexp context.")
  ;; (string-match re-context-re (substring regexp (or start 0) pos))
  )


(defun set-local (variable value)
  "Make VARIABLE buffer local and set it to VALUE."
  (set (make-local-variable variable) value))


(defun map-keymap-sorted (function keymap)
  "Implement `map-keymap' with sorting.
Don't call this function; it is for internal use only."
  (let (list)
    (map-keymap (lambda (a b) (push (cons a b) list))
                keymap)
    (setq list (sort list
                     (lambda (a b)
                       (setq a (car a) b (car b))
                       (if (integerp a)
                           (if (integerp b) (< a b)
                             t)
                         (if (integerp b) t
                           ;; string< also accepts symbols.
                           (string< a b))))))
    (dolist (p list)
      (funcall function (car p) (cdr p)))))


(defun keymap--menu-item-binding (val)
  "Return the binding part of a menu-item."
  (cond
   ((not (consp val)) val)              ;Not a menu-item.
   ((eq 'menu-item (car val))
    (let* ((binding (nth 2 val))
           (plist (nthcdr 3 val))
           (filter (plist-get plist :filter)))
      (if filter (funcall filter binding)
        binding)))
   ((and (consp (cdr val)) (stringp (cadr val)))
    (cddr val))
   ((stringp (car val))
    (cdr val))
   (t val)))                            ;Not a menu-item either.


(defun keymap--menu-item-with-binding (item binding)
  "Build a menu-item like ITEM but with its binding changed to BINDING."
  (cond
   ((not (consp item)) binding)		;Not a menu-item.
   ((eq 'menu-item (car item))
    (setq item (copy-sequence item))
    (let ((tail (nthcdr 2 item)))
      (setcar tail binding)
      ;; Remove any potential filter.
      (if (plist-get (cdr tail) :filter)
          (setcdr tail (plist-put (cdr tail) :filter nil))))
    item)
   ((and (consp (cdr item)) (stringp (cadr item)))
    (cons (car item) (cons (cadr item) binding)))
   (t (cons (car item) binding))))


(defun keymap--merge-bindings (val1 val2)
  "Merge bindings VAL1 and VAL2."
  (let ((map1 (keymap--menu-item-binding val1))
        (map2 (keymap--menu-item-binding val2)))
    (if (not (and (keymapp map1) (keymapp map2)))
        ;; There's nothing to merge: val1 takes precedence.
        val1
      (let ((map (list 'keymap map1 map2))
            (item (if (keymapp val1) (if (keymapp val2) nil val2) val1)))
        (keymap--menu-item-with-binding item map)))))


(defun keymap-canonicalize (map)
  "Return a simpler equivalent keymap.
This resolves inheritance and redefinitions.  The returned keymap
should behave identically to a copy of KEYMAP w.r.t `lookup-key'
and use in active keymaps and menus.
Subkeymaps may be modified but are not canonicalized."
  (declare (important-return-value t))
  ;; FIXME: Problem with the difference between a nil binding
  ;; that hides a binding in an inherited map and a nil binding that's ignored
  ;; to let some further binding visible.  Currently a nil binding hides all.
  ;; FIXME: we may want to carefully (re)order elements in case they're
  ;; menu-entries.
  (let ((bindings ())
        (ranges ())
	(prompt (keymap-prompt map)))
    (while (keymapp map)
      (setq map (map-keymap ;; -internal
                 (lambda (key item)
                   (if (consp key)
                       (if (= (car key) (1- (cdr key)))
                           ;; If we have a two-character range, then
                           ;; treat it as two separate characters
                           ;; (because this makes `describe-bindings'
                           ;; look better and shouldn't affect
                           ;; anything else).
                           (progn
                             (push (cons (car key) item) bindings)
                             (push (cons (cdr key) item) bindings))
                         ;; Treat char-ranges specially.
                         (push (cons key item) ranges))
                     (push (cons key item) bindings)))
                 map)))
    ;; Create the new map.
    (setq map (funcall (if ranges #'make-keymap #'make-sparse-keymap) prompt))
    (dolist (binding ranges)
      ;; Treat char-ranges specially.  FIXME: need to merge as well.
      (define-key map (vector (car binding)) (cdr binding)))
    ;; Process the bindings starting from the end.
    (dolist (binding (prog1 bindings (setq bindings ())))
      (let* ((key (car binding))
             (oldbind (assq key bindings)))
        (push (if (not oldbind)
                  ;; The normal case: no duplicate bindings.
                  binding
                ;; This is the second binding for this key.
                (setq bindings (delq oldbind bindings))
                (cons key (keymap--merge-bindings (cdr binding)
                                                  (cdr oldbind))))
              bindings)))
    (nconc map bindings)))


(defconst listify-key-sequence-1 (logior 128 ?\M-\C-@))


(defun listify-key-sequence (key)
  "Convert a key sequence to a list of events."
  (declare (side-effect-free t))
  (if (or (vectorp key) (multibyte-string-p key))
      (append key nil)
    (mapcar (lambda (c)
              (if (> c 127)
                  (logxor c listify-key-sequence-1)
                c))
	    key)))


(defun eventp (object)
  "Return non-nil if OBJECT is an input event or event object."
  (declare (ftype (function (t) boolean))
           (pure t) (side-effect-free error-free))
  (or (integerp object)
      (and (if (consp object)
               (setq object (car object))
             object)
           (symbolp object)
           (not (keywordp object)))))


(defun event-modifiers (event)
  "Return a list of symbols representing the modifier keys in event EVENT.
The elements of the list may include `meta', `control',
`shift', `hyper', `super', `alt', `click', `double', `triple', `drag',
and `down'.
EVENT may be an event or an event type.  If EVENT is a symbol
that has never been used in an event that has been read as input
in the current Emacs session, then this function may fail to include
the `click' modifier."
  (declare (side-effect-free t))
  (unless (stringp event)
    (let ((type event))
      (if (listp type)
	  (setq type (car type)))
      (if (symbolp type)
          ;; Don't read event-symbol-elements directly since we're not
          ;; sure the symbol has already been parsed.
	  (cdr (internal-event-symbol-parse-modifiers type))
        (let ((list nil)
	      (char (logand type (lognot (logior ?\M-\0 ?\C-\0 ?\S-\0
					         ?\H-\0 ?\s-\0 ?\A-\0)))))
	  (if (not (zerop (logand type ?\M-\0)))
	      (push 'meta list))
	  (if (or (not (zerop (logand type ?\C-\0)))
		  (< char 32))
	      (push 'control list))
	  (if (or (not (zerop (logand type ?\S-\0)))
		  (/= char (downcase char)))
	      (push 'shift list))
	  (or (zerop (logand type ?\H-\0))
	      (push 'hyper list))
	  (or (zerop (logand type ?\s-\0))
	      (push 'super list))
	  (or (zerop (logand type ?\A-\0))
	      (push 'alt list))
	  list)))))


(defsubst mouse-movement-p (object)
  "Return non-nil if OBJECT is a mouse movement event."
  (declare (ftype (function (t) boolean))
           (side-effect-free error-free))
  (eq (car-safe object) 'mouse-movement))


(defun mouse-event-p (object)
  "Return non-nil if OBJECT is a mouse click event."
  (declare (side-effect-free t))
  ;; is this really correct? maybe remove mouse-movement?
  (memq (event-basic-type object) '(mouse-1 mouse-2 mouse-3 mouse-movement)))


(defun event-apply-modifier (event symbol lshiftby prefix)
  "Apply a modifier flag to event EVENT.
SYMBOL is the name of this modifier, as a symbol.
LSHIFTBY is the numeric value of this modifier, in keyboard events.
PREFIX is the string that represents this modifier in an event type symbol."
  (if (numberp event)
      ;; Use the base event to determine how the control and shift
      ;; modifiers should be applied.
      (let* ((base-event (event-basic-type event)))
        (cond ((eq symbol 'control)
	       (if (<= 64 (upcase base-event) 95)
                   ;; Apply the control modifier...
		   (logior (- (upcase base-event) 64)
                           ;; ... and any additional modifiers
                           ;; specified in the original event...
                           (logand event (logior ?\M-\0 ?\C-\0 ?\S-\0
					         ?\H-\0 ?\s-\0 ?\A-\0))
                           ;; ... including any shift modifier that
                           ;; `event-basic-type' may have removed.
                           (if (<= ?A event ?Z) ?\S-\0 0))
	         (logior (ash 1 lshiftby) event)))
	      ((eq symbol 'shift)
               ;; FIXME: Should we also apply this "upcase" behavior of shift
               ;; to non-ascii letters?
	       (if (<= ?a base-event ?z)
                   ;; Apply the Shift modifier.
		   (logior (upcase base-event)
                           ;; ... and any additional modifiers
                           ;; specified in the original event.
                           (logand event (logior ?\M-\0 ?\C-\0 ?\S-\0
					         ?\H-\0 ?\s-\0 ?\A-\0)))
	         (logior (ash 1 lshiftby) event)))
	      (t
	       (logior (ash 1 lshiftby) event))))
    (if (memq symbol (event-modifiers event))
	event
      (let ((event-type (if (symbolp event) event (car event))))
	(setq event-type (intern (concat prefix (symbol-name event-type))))
	(if (symbolp event)
	    event-type
	  (cons event-type (cdr event)))))))



(defun add-remove--display-text-property (start end spec value
                                                &optional object remove)
  (let ((sub-start start)
        (sub-end 0)
        (limit (if (stringp object)
                   (min (length object) end)
                 (min end (point-max))))
        disp)
    (while (< sub-end end)
      (setq sub-end (next-single-property-change sub-start 'display object
                                                 limit))
      (if (not (setq disp (get-text-property sub-start 'display object)))
          ;; No old properties in this range.
          (unless remove
            (put-text-property sub-start sub-end 'display (list spec value)
                               object))
        ;; We have old properties.
        (let ((changed nil)
              type)
          ;; Make disp into a list.
          (setq disp
                (cond
                 ((vectorp disp)
                  (setq type 'vector)
                  (seq-into disp 'list))
                 ((or (not (consp (car-safe disp)))
                      ;; If disp looks like ((margin ...) ...), that's
                      ;; still a single display specification.
                      (eq (caar disp) 'margin))
                  (setq type 'scalar)
                  (list disp))
                 (t
                  (setq type 'list)
                  disp)))
          ;; Remove any old instances.
          (when-let* ((old (assoc spec disp)))
            ;; If the property value was a list, don't modify the
            ;; original value in place; it could be used by other
            ;; regions of text.
            (setq disp (if (eq type 'list)
                           (remove old disp)
                         (delete old disp))
                  changed t))
          (unless remove
            (setq disp (cons (list spec value) disp)
                  changed t))
          (when changed
            (if (not disp)
                (remove-text-properties sub-start sub-end '(display nil) object)
              (when (eq type 'vector)
                (setq disp (seq-into disp 'vector)))
              ;; Finally update the range.
              (put-text-property sub-start sub-end 'display disp object)))))
      (setq sub-start sub-end))))


;;;###autoload
(defun add-display-text-property (start end spec value &optional object)
  "Add the display specification (SPEC VALUE) to the text from START to END.
If any text in the region has a non-nil `display' property, the existing
display specifications are retained.

OBJECT is either a string or a buffer to add the specification to.
If omitted, OBJECT defaults to the current buffer."
  (add-remove--display-text-property start end spec value object))


;;;###autoload
(defun remove-display-text-property (start end spec &optional object)
  "Remove the display specification SPEC from the text from START to END.
SPEC is the car of the display specification to remove, e.g. `height'.
If any text in the region has other display specifications, those specs
are retained.

OBJECT is either a string or a buffer to remove the specification from.
If omitted, OBJECT defaults to the current buffer."
  (add-remove--display-text-property start end spec nil object 'remove))



(defun alter-text-property (from to prop func &optional object)
  "Programmatically change value of a text-property.
For each region between FROM and TO that has a single value for PROPERTY,
apply FUNCTION to that value and sets the property to the function's result.
Optional fifth argument OBJECT specifies the string or buffer to operate on."
  (let ((begin from)
	end val)
    (while (setq val (get-text-property begin prop object)
		 end (text-property-not-all begin to prop val object))
      (put-text-property begin end prop (funcall func val) object)
      (setq begin end))
    (if (< begin to)
	(put-text-property begin to prop (funcall func val) object))))


(defun font-lock--add-text-property (start end prop value object append)
  "Add an element to a property of the text from START to END.
Arguments PROP and VALUE specify the property and value to add to
the value already in place.  The resulting property values are
always lists.  Argument OBJECT is the string or buffer containing
the text.  If argument APPEND is non-nil, VALUE will be appended,
otherwise it will be prepended."
  (let ((val (if (and (listp value) (not (keywordp (car value))))
                 ;; Already a list of faces.
                 value
               ;; A single face (e.g. a plist of face properties).
               (list value)))
        next prev)
    (while (/= start end)
      (setq next (next-single-property-change start prop object end)
	    prev (get-text-property start prop object))
      ;; Canonicalize old forms of face property.
      (and (memq prop '(face font-lock-face))
	   (listp prev)
	   (or (keywordp (car prev))
	       (memq (car prev) '(foreground-color background-color)))
	   (setq prev (list prev)))
      (let* ((list-prev (if (listp prev) prev (list prev)))
             (new-value (if append
                           (append list-prev val)
                         (append val list-prev))))
        (put-text-property start next prop new-value object))
      (setq start next))))


(defun font-lock-prepend-text-property (start end prop value &optional object)
  "Prepend to one property of the text from START to END.
Arguments PROP and VALUE specify the property and value to prepend to the value
already in place.  The resulting property values are always lists.
Optional argument OBJECT is the string or buffer containing the text."
  (font-lock--add-text-property start end prop value object nil))


(defun font-lock-append-text-property (start end prop value &optional object)
  "Append to one property of the text from START to END.
Arguments PROP and VALUE specify the property and value to append to the value
already in place.  The resulting property values are always lists.
Optional argument OBJECT is the string or buffer containing the text."
  (font-lock--add-text-property start end prop value object t))


(defun font-lock-fillin-text-property (start end prop value &optional object)
  "Fill in one property of the text from START to END.
Arguments PROP and VALUE specify the property and value to put where none are
already in place.  Therefore existing property values are not overwritten.
Optional argument OBJECT is the string or buffer containing the text."
  (let ((start (text-property-any start end prop nil object)) next)
    (while start
      (setq next (next-single-property-change start prop object end))
      (put-text-property start next prop value object)
      (setq start (text-property-any next end prop nil object)))))



(defvar truncate-string-ellipsis nil
  "String to use to indicate truncation.
Serves as default value of ELLIPSIS argument to `truncate-string-to-width'
returned by the function `truncate-string-ellipsis'.")


;;;###autoload
(defun truncate-string-to-width (str end-column
				     &optional start-column padding ellipsis
                                     ellipsis-text-property)
  "Truncate string STR to end at column END-COLUMN.
The optional 3rd arg START-COLUMN, if non-nil, specifies the starting
column (default: zero); that means to return the characters occupying
columns START-COLUMN ... END-COLUMN of STR.  Both END-COLUMN and
START-COLUMN are specified in terms of character display width in the
current buffer; see `char-width'.

Since character composition on display can produce glyphs whose
width is smaller than the sum of `char-width' values of the
composed characters, this function can produce inaccurate results
when used in such cases.

The optional 4th arg PADDING, if non-nil, specifies a padding
character (which should have a display width of 1) to add at the end
of the result if STR doesn't reach column END-COLUMN, or if END-COLUMN
comes in the middle of a character in STR.  PADDING is also added at
the beginning of the result if column START-COLUMN appears in the
middle of a character in STR.

If PADDING is nil, no padding is added in these cases, so
the resulting string may be narrower than END-COLUMN.

If ELLIPSIS is non-nil, it should be a string which will replace the
end of STR (including any padding) if it extends beyond END-COLUMN,
unless the display width of STR is equal to or less than the display
width of ELLIPSIS.  If it is non-nil and not a string, then ELLIPSIS
defaults to `truncate-string-ellipsis', or to three dots when it's nil.

If ELLIPSIS-TEXT-PROPERTY is non-nil, a too-long string will not
be truncated, but instead the elided parts will be covered by a
`display' text property showing the ellipsis."
  (or start-column
      (setq start-column 0))
  (when (and ellipsis (not (stringp ellipsis)))
    (setq ellipsis (truncate-string-ellipsis)))
  (let ((str-len (length str))
	(str-width (string-width str))
	(ellipsis-width (if ellipsis (string-width ellipsis) 0))
	(idx 0)
	(column 0)
	(head-padding "") (tail-padding "")
	ch last-column last-idx from-idx)
    (condition-case nil
	(while (< column start-column)
	  (setq ch (aref str idx)
		column (+ column (char-width ch))
		idx (1+ idx)))
      (args-out-of-range (setq idx str-len)))
    (if (< column start-column)
	(if padding (make-string end-column padding) "")
      (when (and padding (> column start-column))
	(setq head-padding (make-string (- column start-column) padding)))
      (setq from-idx idx)
      (when (>= end-column column)
	(if (and (< end-column str-width)
		 (> str-width ellipsis-width))
	    (setq end-column (- end-column ellipsis-width))
	  (setq ellipsis ""))
	(condition-case nil
	    (while (< column end-column)
	      (setq last-column column
		    last-idx idx
		    ch (aref str idx)
		    column (+ column (char-width ch))
		    idx (1+ idx)))
	  (args-out-of-range (setq idx str-len)))
	(when (> column end-column)
	  (setq column last-column
		idx last-idx))
	(when (and padding (< column end-column))
	  (setq tail-padding (make-string (- end-column column) padding))))
      (if (and ellipsis-text-property
               (not (equal ellipsis ""))
               idx)
          ;; Use text properties for the ellipsis.
          (concat head-padding
                  (substring str from-idx idx)
	          (propertize (substring str idx) 'display (or ellipsis "")))
        ;; (Possibly) chop off bits of the string.
        (concat head-padding (substring str from-idx idx)
	        tail-padding ellipsis)))))



(defun truncate-string-ellipsis ()
  "Return the string used to indicate truncation.
Use the value of the variable `truncate-string-ellipsis' when it's non-nil.
Otherwise, return the Unicode character U+2026 \"HORIZONTAL ELLIPSIS\"
when it's displayable on the selected frame, or `...'.  This function
needs to be called on every use of `truncate-string-to-width' to
decide whether the selected frame can display that Unicode character."
  (cond
   (truncate-string-ellipsis)
   ((char-displayable-p ?…) "…")
   ("...")))

