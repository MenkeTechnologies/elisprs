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


;;;; subr-x.el

(defmacro with-buffer-unmodified-if-unchanged (&rest body)
  "Like `progn', but change buffer-modified status only if buffer text changes.
If the buffer was unmodified before execution of BODY, and
buffer text after execution of BODY is identical to what it was
before, ensure that buffer is still marked unmodified afterwards.
For example, the following won't change the buffer's modification
status:

  (with-buffer-unmodified-if-unchanged
    (insert \"a\")
    (delete-char -1))

Note that only changes in the raw byte sequence of the buffer text,
as stored in the internal representation, are monitored for the
purpose of detecting the lack of changes in buffer text.  Any other
changes that are normally perceived as \"buffer modifications\", such
as changes in text properties, `buffer-file-coding-system', buffer
multibyteness, etc. -- will not be noticed, and the buffer will still
be marked unmodified, effectively ignoring those changes."
  (declare (debug t) (indent 0))
  (cl-with-gensyms (hash buffer)
    `(let ((,hash (and (not (buffer-modified-p))
                       (buffer-hash)))
           (,buffer (current-buffer)))
       (prog1
           (progn
             ,@body)
         ;; If we didn't change anything in the buffer (and the buffer
         ;; was previously unmodified), then flip the modification status
         ;; back to "unchanged".
         (when (and ,hash (buffer-live-p ,buffer))
           (with-current-buffer ,buffer
             (when (and (buffer-modified-p)
                        (equal ,hash (buffer-hash)))
               (restore-buffer-modified-p nil))))))))

;;;; subr.el — change-hook combining.  Undo entries are not recorded, so the
;;;; `buffer-undo-list' rewrite in `combine-change-calls-1' never triggers.

(defvar undo--combining-change-calls nil
  "Non-nil when `combine-change-calls-1' is running.")

(defun combine-change-calls-1 (beg end body)
  "Evaluate BODY, running the change hooks just once, for region \(BEG END).

Firstly, `before-change-functions' is invoked for the region
\(BEG END), then BODY (a function) is evaluated with
`before-change-functions' and `after-change-functions' bound to
nil, then finally `after-change-functions' is invoked on the
updated region (BEG NEW-END) with a calculated OLD-LEN argument.
If `inhibit-modification-hooks' is initially non-nil, the change
hooks are not run.

The result of `combine-change-calls-1' is the value returned by
BODY.  BODY must not make a different buffer current, except
temporarily.  It must not make any changes to the buffer outside
the specified region.  It must not change
`before-change-functions' or `after-change-functions'.

Additionally, the buffer modifications of BODY are recorded on
the buffer's undo list as a single (apply ...) entry containing
the function `undo--wrap-and-run-primitive-undo'."
  (if (markerp beg) (setq beg (marker-position beg)))
  (if (markerp end) (setq end (marker-position end)))
  (let ((old-bul buffer-undo-list)
	(end-marker (copy-marker end t))
	result)
    (if undo--combining-change-calls
	(setq result (funcall body))
      (let ((undo--combining-change-calls t))
	(if (not inhibit-modification-hooks)
	    (run-hook-with-args 'before-change-functions beg end))
	(let ((bcf before-change-functions)
	      (acf after-change-functions)
	      (local-bcf (local-variable-p 'before-change-functions))
	      (local-acf (local-variable-p 'after-change-functions)))
	  (unwind-protect
              ;; FIXME: WIBNI we could just use `inhibit-modification-hooks'?
              (progn
                ;; Ugly Hack: if the body uses syntax-ppss/syntax-propertize
                ;; (e.g. via a regexp-search or sexp-movement triggering
                ;; on-the-fly syntax-propertize), make sure that this gets
                ;; properly refreshed after subsequent changes.
	        (setq-local before-change-functions
                            (if (memq #'syntax-ppss-flush-cache bcf)
                                '(syntax-ppss-flush-cache)))
                (setq-local after-change-functions nil)
	        (setq result (funcall body)))
	    (if local-bcf (setq before-change-functions bcf)
	      (kill-local-variable 'before-change-functions))
	    (if local-acf (setq after-change-functions acf)
	      (kill-local-variable 'after-change-functions))))
	;; If buffer-undo-list is neither t (in which case undo
	;; information is not recorded) nor equal to buffer-undo-list
	;; before body was funcalled (in which case (funcall body) did
	;; not add items to buffer-undo-list) ...
	(unless (or (eq buffer-undo-list t)
		    (eq buffer-undo-list old-bul))
	  (let ((ptr buffer-undo-list) body-undo-list)
	    ;; ... then loop over buffer-undo-list, until the head of
	    ;; buffer-undo-list before body was funcalled is found, or
	    ;; ptr is nil (which may happen if garbage-collect has
	    ;; been called after (funcall body) and has removed
	    ;; entries of buffer-undo-list that were added by (funcall
	    ;; body)), and add these entries to body-undo-list.
	    (while (and ptr (not (eq ptr old-bul)))
	      (push (car ptr) body-undo-list)
	      (setq ptr (cdr ptr)))
	    (setq body-undo-list (nreverse body-undo-list))
	    ;; Warn if garbage-collect has truncated buffer-undo-list
	    ;; behind our back.
	    (when (and old-bul (not ptr))
	      (message
               "combine-change-calls: buffer-undo-list has been truncated"))
	    ;; Add an (apply ...) entry to buffer-undo-list, using
	    ;; body-undo-list ...
	    (push (list 'apply
			(- end end-marker)
			beg
			(marker-position end-marker)
			#'undo--wrap-and-run-primitive-undo
			beg (marker-position end-marker)
			body-undo-list)
		  buffer-undo-list)
	    ;; ... and set the cdr of buffer-undo-list to
	    ;; buffer-undo-list before body was funcalled.
	    (setcdr buffer-undo-list old-bul)))
	(if (not inhibit-modification-hooks)
	    (run-hook-with-args 'after-change-functions
				beg (marker-position end-marker)
				(- end beg)))))
    (set-marker end-marker nil)
    result))

(defmacro combine-change-calls (beg end &rest body)
  "Evaluate BODY, running the change hooks just once.

BODY is a sequence of Lisp forms to evaluate.  BEG and END bound
the region the change hooks will be run for.

Firstly, `before-change-functions' is invoked for the region
\(BEG END), then the BODY forms are evaluated with
`before-change-functions' and `after-change-functions' bound to
nil, and finally `after-change-functions' is invoked on the
updated region.  The change hooks are not run if
`inhibit-modification-hooks' is initially non-nil.

The result of `combine-change-calls' is the value returned by the
last of the BODY forms to be evaluated.  BODY may not make a
different buffer current, except temporarily.  BODY may not
change the buffer outside the specified region.  It must not
change `before-change-functions' or `after-change-functions'.

Additionally, the buffer modifications of BODY are recorded on
the buffer's undo list as a single \(apply ...) entry containing
the function `undo--wrap-and-run-primitive-undo'."
  (declare (debug (form form def-body)) (indent 2))
  `(combine-change-calls-1 ,beg ,end (lambda () ,@body)))


;; buffer.c's DEFVARs that Fkill_buffer runs.
(defvar kill-buffer-query-functions nil)
(defvar kill-buffer-hook nil)

(defun process-kill-buffer-query-function ()
  "Ask before killing a buffer that has a running process."
  (let ((process (get-buffer-process (current-buffer))))
    (or (not process)
        (not (memq (process-status process) '(run stop open listen)))
        (not (process-query-on-exit-flag process))
        (yes-or-no-p
	 (format "Buffer %S has a running process; kill it? "
		 (buffer-name (current-buffer)))))))

(add-hook 'kill-buffer-query-functions #'process-kill-buffer-query-function)

;;;; subr.el — progress reporters.

(defvar progress-reporter-update-functions (list #'progress-reporter-echo-area)
  "Special hook run on progress-reporter updates.
Each function is called with three arguments:
REPORTER is the result of a call to `make-progress-reporter'.
STATE can be one of:
- A float representing the percentage complete in the range 0.0-1.0
for a numeric reporter.
- A monotonically increasing integer for a pulsing reporter.
- The symbol `done' to indicate that the progress reporter is complete.
UPDATE-TEXT is a string that a progress-reporter back-end might display
as a result of this update.  A typical use is as the \"step\" of the
progress reporting process.")

(defsubst progress-reporter-update (reporter &optional value update-text)
  "Report progress of an operation, by default, in the echo area.
REPORTER should be the result of a call to `make-progress-reporter'.

If REPORTER is a numerical progress reporter---i.e. if it was
made using non-nil MIN-VALUE and MAX-VALUE arguments to
`make-progress-reporter'---then VALUE should be a number between
MIN-VALUE and MAX-VALUE.

Optional argument UPDATE-TEXT is a string that a progress-reporter
back-end might display as a result of this update.  A typical use is as
the \"step\" of the progress reporting process.  If REPORTER is a
non-numerical reporter, then VALUE should be nil, or a string to use
instead of UPDATE-TEXT.

See `progress-reporter-update-functions' for the list of functions
called on each update.

This function is relatively inexpensive.  If the change since
last update is too small or insufficient time has passed, it does
nothing."
  (when (or (not (numberp value))      ; For pulsing reporter
	    (>= value (car reporter))) ; For numerical reporter
    (progress-reporter-do-update reporter value update-text)))

(defun make-progress-reporter (message &optional min-value max-value
				       current-value min-change min-time
                                       context)
  "Return progress reporter object for use with `progress-reporter-update'.

MESSAGE is shown in the echo area, with a status indicator
appended to the end.  When you call `progress-reporter-done', the
word \"done\" is printed after the MESSAGE.  You can change the
MESSAGE of an existing progress reporter by calling
`progress-reporter-force-update'.

MIN-VALUE and MAX-VALUE, if non-nil, are starting (0% complete)
and final (100% complete) states of operation; the latter should
be larger.  In this case, the status message shows the percentage
progress.

If MIN-VALUE and/or MAX-VALUE is omitted or nil, the status
message shows a \"spinning\", non-numeric indicator.

Optional CURRENT-VALUE is the initial progress; the default is
MIN-VALUE.
Optional MIN-CHANGE is the minimal change in percents to report;
the default is 1%.
CURRENT-VALUE and MIN-CHANGE do not have any effect if MIN-VALUE
and/or MAX-VALUE are nil.

Optional MIN-TIME specifies the minimum interval time between
echo area updates (default is 0.2 seconds.)  If the OS is not
capable of measuring fractions of seconds, this parameter is
effectively rounded up.

Optional CONTEXT can be nil or `async'.  It is consulted by back ends before
showing progress updates.  For example, when CONTEXT is `async',
the echo area progress reports may be muted if the echo area is busy."
  (when (string-match "[[:alnum:]]\\'" message)
    (setq message (concat message "...")))
  (unless min-time
    (setq min-time 0.2))
  (let ((reporter
	 (cons (or min-value 0)
	       ;; FIXME: Use defstruct.
	       (vector (if (>= min-time 0.02)
			   (float-time) nil)
		       min-value
		       max-value
		       message
		       (if min-change (max (min min-change 50) 1) 1)
                       min-time
                       ;; Unused (formerly SUFFIX).
                       nil
                       ;;
                       context))))
    ;; Force a call to `message' now.
    (progress-reporter-update reporter (or current-value min-value))
    reporter))

(defalias 'progress-reporter-make #'make-progress-reporter)

(defun progress-reporter-text (reporter)
  "Return REPORTER's text."
  (aref (cdr reporter) 3))

(defun progress-reporter-context (reporter)
  "Return REPORTER's context."
  (aref (cdr reporter) 7))

(defun progress-reporter-force-update (reporter &optional
                                                value new-message update-text)
  "Report progress of an operation in the echo area unconditionally.

REPORTER, VALUE, and UPDATE-TEXT are the same as in
`progress-reporter-update'.
NEW-MESSAGE, if non-nil, sets a new message for the reporter."
  (let ((parameters (cdr reporter)))
    (when new-message
      (aset parameters 3 new-message))
    (when (aref parameters 0)
      (aset parameters 0 (float-time)))
    (progress-reporter-do-update reporter value update-text)))

(defvar progress-reporter--pulse-characters ["-" "\\" "|" "/"]
  "Characters to use for pulsing progress reporters.")

(defun progress-reporter-echo-area (reporter state update-text)
  "Progress reporter echo area update function.
REPORTER, STATE, and UPDATE-TEXT are the same as in
`progress-reporter-update-functions'.

Do not emit a message if the reporter context is `async' and the echo
area is busy with something else."
  (let ((text (progress-reporter-text reporter)))
    (unless (and (eq (progress-reporter-context reporter) 'async)
                 (current-message)
                 (not (string-prefix-p text (current-message))))
      (setq update-text (if update-text (format " %s" update-text) ""))
      (pcase state
        ((pred floatp)
         (if (plusp state)
             (message "%s%d%%%s" text (* state 100.0) update-text)
           (message "%s%s" text update-text)))
        ((pred integerp)
         (let ((message-log-max nil)
               (pulse-char
                (aref progress-reporter--pulse-characters
                      (mod state (length progress-reporter--pulse-characters)))))
           (message "%s %s%s" text pulse-char update-text)))
        ('done
         (message "%sdone" text))))))

(defun progress-reporter-do-update (reporter value &optional update-text)
  (let* ((parameters      (cdr reporter))
	 (update-time     (aref parameters 0))
	 (min-value       (aref parameters 1))
	 (max-value       (aref parameters 2))
	 (enough-time-passed
	  ;; See if enough time has passed since the last update.
	  (or (not update-time)
	      (when (time-less-p update-time nil)
		;; Calculate time for the next update
		(aset parameters 0 (+ update-time (aref parameters 5)))))))
    (cond ((and min-value max-value)
	   ;; Numerical indicator
	   (let* ((one-percent (/ (- max-value min-value) 100.0))
		  (percentage  (if (= max-value min-value)
				   0
				 (truncate (/ (- value min-value)
					      one-percent)))))
	     ;; Calculate NEXT-UPDATE-VALUE.  If we are not printing
	     ;; message because not enough time has passed, use 1
	     ;; instead of MIN-CHANGE.  This makes delays between echo
	     ;; area updates closer to MIN-TIME.
	     (setcar reporter
		     (min (+ min-value (* (+ percentage
					     (if enough-time-passed
						 ;; MIN-CHANGE
						 (aref parameters 4)
					       1))
					  one-percent))
			  max-value))
	     (when (integerp value)
	       (setcar reporter (ceiling (car reporter))))
	     ;; Print message only if enough time has passed
	     (when enough-time-passed
               (run-hook-with-args 'progress-reporter-update-functions
                                   reporter
                                   (/ percentage 100.0)
                                   update-text))))
	  ;; Pulsing indicator
	  (enough-time-passed
           (let ((index (1+ (car reporter))))
	     (setcar reporter index)
             (run-hook-with-args 'progress-reporter-update-functions
                                 reporter
                                 index
                                 (or update-text value)))))))

(defun progress-reporter-done (reporter)
  "Print reporter's message followed by word \"done\" in echo area.
Call the functions on `progress-reporter-update-functions`."
  (run-hook-with-args 'progress-reporter-update-functions
                      reporter
                      'done
                      nil))

(defmacro dotimes-with-progress-reporter (spec reporter-or-message &rest body)
  "Loop a certain number of times and report progress in the echo area.
Evaluate BODY with VAR bound to successive integers running from
0, inclusive, to COUNT, exclusive.  Then evaluate RESULT to get
the return value (nil if RESULT is omitted).

REPORTER-OR-MESSAGE is a progress reporter object or a string.  In the latter
case, use this string to create a progress reporter.

At each iteration, print the reporter message followed by progress
percentage in the echo area.  After the loop is finished,
print the reporter message followed by the word \"done\".

This macro is a convenience wrapper around `make-progress-reporter' and friends.

\(fn (VAR COUNT [RESULT]) REPORTER-OR-MESSAGE BODY...)"
  (declare (indent 2) (debug ((symbolp form &optional form) form body)))
  (let ((prep (make-symbol "--dotimes-prep--"))
        (end (make-symbol "--dotimes-end--")))
    `(let ((,prep ,reporter-or-message)
           (,end ,(cadr spec)))
       (when (stringp ,prep)
         (setq ,prep (make-progress-reporter ,prep 0 ,end)))
       (dotimes (,(car spec) ,end)
         ,@body
         (progress-reporter-update ,prep (1+ ,(car spec))))
       (progress-reporter-done ,prep)
       (or ,@(cdr (cdr spec)) nil))))

(defmacro dolist-with-progress-reporter (spec reporter-or-message &rest body)
  "Loop over a list and report progress in the echo area.
Evaluate BODY with VAR bound to each car from LIST, in turn.
Then evaluate RESULT to get return value, default nil.

REPORTER-OR-MESSAGE is a progress reporter object or a string.  In the latter
case, use this string to create a progress reporter.

At each iteration, print the reporter message followed by progress
percentage in the echo area.  After the loop is finished,
print the reporter message followed by the word \"done\".

\(fn (VAR LIST [RESULT]) REPORTER-OR-MESSAGE BODY...)"
  (declare (indent 2) (debug ((symbolp form &optional form) form body)))
  (let ((prep (make-symbol "--dolist-progress-reporter--"))
        (count (make-symbol "--dolist-count--"))
        (list (make-symbol "--dolist-list--")))
    `(let ((,prep ,reporter-or-message)
           (,count 0)
           (,list ,(cadr spec)))
       (when (stringp ,prep)
         (setq ,prep (make-progress-reporter ,prep 0 (length ,list))))
       (dolist (,(car spec) ,list)
         ,@body
         (progress-reporter-update ,prep (setq ,count (1+ ,count))))
       (progress-reporter-done ,prep)
       (or ,@(cdr (cdr spec)) nil))))

;;;; subr.el — alternate names for functions.

(defalias 'rplaca #'setcar)
(defalias 'rplacd #'setcdr)
(defalias 'indent-to-column #'indent-to)
(defalias 'backward-delete-char #'delete-backward-char)
(defalias 'store-match-data #'set-match-data)
(defalias 'chmod #'set-file-modes)
(defalias 'mkdir #'make-directory)
(define-obsolete-function-alias 'user-original-login-name #'user-login-name "28.1")
