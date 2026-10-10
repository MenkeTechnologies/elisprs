;;; keymap.c / keyboard.c ports: map-keymap, copy-keymap, keymap-prompt and
;;; the event-symbol helpers.

;; get_keymap (OBJECT, ERROR_IF_NOT_KEYMAP, AUTOLOAD)
(defun keymap--get-keymap (object error-if-not-keymap autoload)
  (let ((result
         (cond
          ((null object) nil)
          ((and (consp object) (eq (car object) 'keymap)) object)
          (t
           (let ((tem (indirect-function object)))
             (cond
              ((and (consp tem) (eq (car tem) 'keymap)) tem)
              ((and (consp tem) (eq (car tem) 'autoload) (symbolp object)
                    (or autoload (not error-if-not-keymap))
                    (eq (nth 4 tem) 'keymap))
               (if autoload
                   (progn
                     (autoload-do-load tem object)
                     (let ((new (indirect-function object)))
                       (and (consp new) (eq (car new) 'keymap) new)))
                 object))
              (t nil)))))))
    (if (and (null result) error-if-not-keymap)
        (signal 'wrong-type-argument (list 'keymapp object))
      result)))

(defun keymap--keymapp (object)
  (and (keymap--get-keymap object nil nil) t))

(defun keymap--item (function key val)
  (funcall function key (if (eq val t) nil val)))

;; map_keymap_internal: call FUNCTION on the bindings of MAP's own part and
;; return the tail that begins its parent (or an embedded keymap).
(defun keymap--map-internal (function map)
  (let ((tail (if (and (consp map) (eq (car map) 'keymap)) (cdr map) map))
        (done nil))
    (while (and (not done) (consp tail) (not (eq (car tail) 'keymap)))
      (let ((binding (car tail)))
        (cond
         ((keymap--keymapp binding) (setq done t))
         ((consp binding)
          (keymap--item function (car binding) (cdr binding)))
         ((vectorp binding)
          (let ((c 0) (n (length binding)))
            (while (< c n)
              (keymap--item function c (aref binding c))
              (setq c (1+ c)))))))
      (unless done (setq tail (cdr tail))))
    tail))

(defun keymap--map (function map autoload)
  (setq map (keymap--get-keymap map t autoload))
  (while (consp map)
    (if (keymap--keymapp (car map))
        (progn (keymap--map function (car map) autoload)
               (setq map (cdr map)))
      (setq map (keymap--map-internal function map)))
    (unless (consp map)
      (setq map (keymap--get-keymap map nil autoload)))))

(defun map-keymap (function keymap &optional sort-first)
  "Call FUNCTION once for each event binding in KEYMAP.
FUNCTION is called with two arguments: the event that is bound, and
the definition it is bound to.  The event may be a character range.

If KEYMAP has a parent, the parent's bindings are included as well.
This works recursively: if the parent has itself a parent, then the
grandparent's bindings are also included and so on.

For more information, see Info node `(elisp) Keymaps'.

If SORT-FIRST is non-nil, FUNCTION is called in the order of
`map-keymap-sorted'."
  (if sort-first
      (map-keymap-sorted function keymap)
    (keymap--map function keymap t)
    nil))

(defun map-keymap-internal (function keymap)
  "Call FUNCTION once for each event binding in KEYMAP.
FUNCTION is called with two arguments: the event that is bound, and
the definition it is bound to.  The event may be a character range.
If KEYMAP has a parent, return it without processing it."
  (keymap--map-internal function (keymap--get-keymap keymap t t)))

(defun keymap-prompt (map)
  "Return the prompt-string of a keymap MAP.
If non-nil, the prompt is shown in the echo-area
when reading a key-sequence to be looked-up in this keymap."
  (setq map (keymap--get-keymap map nil nil))
  (let ((res nil))
    (while (and (consp map) (not res))
      (let ((tem (car map)))
        (cond ((stringp tem) (setq res tem))
              ((keymap--keymapp tem)
               (setq res (keymap-prompt tem)))))
      (unless res (setq map (cdr map))))
    res))

(defun keymap--copy-item (elt)
  (if (not (consp elt))
      elt
    (let ((res elt) (tem elt))
      (if (eq (car tem) 'menu-item)
          (progn
            (setq res (cons (car tem) (cdr tem)) elt res tem (cdr elt))
            (when (consp tem)
              (setcdr elt (cons (car tem) (cdr tem)))
              (setq elt (cdr elt) tem (cdr elt)))
            (when (consp tem)
              (setcdr elt (cons (car tem) (cdr tem)))
              (setq elt (cdr elt) tem (car elt))
              (when (and (consp tem) (eq (car tem) 'keymap))
                (setcar elt (copy-keymap tem)))
              (setq tem (cdr elt))))
        (cond
         ((stringp (car tem))
          (setq res (cons (car tem) (cdr tem)) elt res tem (cdr elt))
          (when (and (consp tem) (stringp (car tem)))
            (setcdr elt (cons (car tem) (cdr tem)))
            (setq elt (cdr elt) tem (cdr elt)))
          (when (and (consp tem) (eq (car tem) 'keymap))
            (setcdr elt (copy-keymap tem))))
         ((eq (car tem) 'keymap)
          (setq res (copy-keymap elt)))))
      res)))

(defun copy-keymap (keymap)
  "Return a copy of the keymap KEYMAP.

Note that this is almost never the right way to copy a keymap.  For
instance, it does not handle parent keymaps; it just copies the
keymap's own bindings.  If you want to modify a keymap, you will
generally be better off using `define-keymap' instead."
  (setq keymap (keymap--get-keymap keymap t nil))
  (let* ((copy (list 'keymap)) (tail copy))
    (setq keymap (cdr keymap))
    (while (and (consp keymap) (not (eq (car keymap) 'keymap)))
      (let ((elt (car keymap)))
        (cond
         ((char-table-p elt) (setq elt (copy-sequence elt)))
         ((vectorp elt)
          (setq elt (copy-sequence elt))
          (let ((i 0))
            (while (< i (length elt))
              (aset elt i (keymap--copy-item (aref elt i)))
              (setq i (1+ i)))))
         ((consp elt)
          (setq elt (cons (car elt) (keymap--copy-item (cdr elt))))))
        (setcdr tail (list elt))
        (setq tail (cdr tail)
              keymap (cdr keymap))))
    (setcdr tail keymap)
    copy))

;;;; Event symbols (keyboard.c).

(defconst keymap--modifier-bits
  '((up . 1) (down . 2) (drag . 4) (click . 8) (double . 16) (triple . 32)
    (alt . #x400000) (super . #x800000) (hyper . #x1000000)
    (shift . #x2000000) (control . #x4000000) (meta . #x8000000)))

(defun keymap--symbol-modifiers (name)
  "parse_modifiers_uncached: return (MODIFIER-MASK . END-INDEX) for NAME."
  (let ((i 0) (mods 0) (len (length name)) (go t))
    (while (and go (< i (1- len)))
      (let ((this-mod 0) (this-end 0) (c (aref name i)))
        (cond
         ((eq c ?A) (setq this-end (1+ i) this-mod #x400000))
         ((eq c ?C) (setq this-end (1+ i) this-mod #x4000000))
         ((eq c ?H) (setq this-end (1+ i) this-mod #x1000000))
         ((eq c ?M) (setq this-end (1+ i) this-mod #x8000000))
         ((eq c ?S) (setq this-end (1+ i) this-mod #x2000000))
         ((eq c ?s) (setq this-end (1+ i) this-mod #x800000))
         ((eq c ?d)
          (cond ((and (<= (+ i 5) len) (string= (substring name i (+ i 4)) "drag"))
                 (setq this-end (+ i 4) this-mod 4))
                ((and (<= (+ i 5) len) (string= (substring name i (+ i 4)) "down"))
                 (setq this-end (+ i 4) this-mod 2))
                ((and (<= (+ i 7) len) (string= (substring name i (+ i 6)) "double"))
                 (setq this-end (+ i 6) this-mod 16))))
         ((eq c ?t)
          (when (and (<= (+ i 7) len) (string= (substring name i (+ i 6)) "triple"))
            (setq this-end (+ i 6) this-mod 32)))
         ((eq c ?u)
          (when (and (<= (+ i 3) len) (string= (substring name i (+ i 2)) "up"))
            (setq this-end (+ i 2) this-mod 1))))
        (if (or (= this-end 0) (>= this-end len) (/= (aref name this-end) ?-))
            (setq go nil)
          (setq mods (logior mods this-mod)
                i (1+ this-end)))))
    (when (and (zerop (logand mods (logior 2 4 16 32)))
               (= (+ i 7) len)
               (string= (substring name i (+ i 6)) "mouse-")
               (<= ?0 (aref name (+ i 6)) ?9))
      (setq mods (logior mods 8)))
    (when (and (zerop (logand mods (logior 16 32)))
               (< (+ i 6) len)
               (string= (substring name i (+ i 6)) "wheel-"))
      (setq mods (logior mods 8)))
    (cons mods i)))

(defun keymap--modifier-list (mods)
  (let ((res nil))
    (dolist (m keymap--modifier-bits)
      (unless (zerop (logand mods (cdr m)))
        (push (car m) res)))
    res))

(defun internal-event-symbol-parse-modifiers (symbol)
  "Parse the event symbol.  For internal use."
  (unless (symbolp symbol)
    (signal 'wrong-type-argument (list 'symbolp symbol)))
  (let ((cached (get symbol 'event-symbol-elements)))
    (or cached
        (let* ((name (symbol-name symbol))
               (parsed (keymap--symbol-modifiers name))
               (unmodified (intern (substring name (cdr parsed)))))
          (put symbol 'event-symbol-element-mask (list unmodified (car parsed)))
          (put symbol 'event-symbol-elements
               (cons unmodified (keymap--modifier-list (car parsed))))))))

(defun keymap--make-ctrl-char (c)
  (let ((upper (logand c (lognot #o177))))
    (if (>= c 128)
        (logior c #x4000000)
      (setq c (logand c #o177))
      (cond
       ((and (>= c #o100) (< c #o140))
        (let ((oc c))
          (setq c (logand c (lognot #o140)))
          (when (and (>= oc ?A) (<= oc ?Z))
            (setq c (logior c #x2000000)))))
       ((and (>= c ?a) (<= c ?z))
        (setq c (logand c (lognot #o140))))
       ((>= c ?\s)
        (setq c (logior c #x4000000))))
      (logior c (logand upper (lognot #x4000000))))))

(defun keymap--solitary-modifier (sym)
  (let ((name (symbol-name sym)))
    (cond
     ((member name '("A" "alt")) #x400000)
     ((member name '("C" "ctrl" "control")) #x4000000)
     ((member name '("H" "hyper")) #x1000000)
     ((member name '("M" "meta")) #x8000000)
     ((member name '("S" "shift")) #x2000000)
     ((member name '("s" "super")) #x800000)
     ((string= name "down") 2)
     ((string= name "drag") 4)
     ((string= name "double") 16)
     ((string= name "triple") 32)
     ((string= name "up") 1)
     ((string= name "click") 8)
     (t 0))))

(defun event-convert-list (event-desc)
  "Convert the event description list EVENT-DESC to an event type.
EVENT-DESC should contain one base event type (a character or symbol)
and zero or more modifier names (control, meta, hyper, super, shift, alt,
drag, down, double or triple).  The base must be last.
The return value is an event type (a character or symbol) which
has essentially the same meaning as the event description list.
See Info node `(elisp)Classifying Events' for more information."
  (let ((base nil) (modifiers 0))
    (while (consp event-desc)
      (let* ((elt (car event-desc))
             (this (if (and (symbolp elt) (consp (cdr event-desc)))
                       (keymap--solitary-modifier elt)
                     0)))
        (cond ((/= this 0) (setq modifiers (logior modifiers this)))
              (base (error "Two bases given in one event"))
              (t (setq base elt))))
      (setq event-desc (cdr event-desc)))
    (when (and (symbolp base) (= (length (symbol-name base)) 1))
      (setq base (aref (symbol-name base) 0)))
    (cond
     ((integerp base)
      (when (and (/= 0 (logand modifiers #x2000000)) (<= ?a base ?z))
        (setq base (- base (- ?a ?A))
              modifiers (logand modifiers (lognot #x2000000))))
      (if (/= 0 (logand modifiers #x4000000))
          (logior (logand modifiers (lognot #x4000000))
                  (keymap--make-ctrl-char base))
        (logior modifiers base)))
     ((symbolp base)
      (let ((name (symbol-name base)) (prefix ""))
        (dolist (m '((#x400000 . "A-") (#x4000000 . "C-") (#x1000000 . "H-")
                     (#x8000000 . "M-") (#x2000000 . "S-") (#x800000 . "s-")
                     (16 . "double-") (32 . "triple-") (1 . "up-")
                     (2 . "down-") (4 . "drag-")))
          (unless (zerop (logand modifiers (car m)))
            (setq prefix (concat prefix (cdr m)))))
        (intern (concat prefix name))))
     (t (error "Invalid base event")))))

;;;; Key descriptions (keymap.c).

(defun keymap--lucid-event-type-list-p (object)
  (and (consp object)
       (not (memq (car object)
                  '(help-echo vertical-line mode-line tab-line header-line)))
       (let ((tail object) (ok t))
         (while (and ok (consp tail))
           (let ((elt (car tail)))
             (unless (or (integerp elt) (symbolp elt)) (setq ok nil)))
           (setq tail (cdr tail)))
         (and ok (null tail)))))

(defun keymap--push-key-description (ch)
  "push_key_description: the text of the character CH with modifier bits."
  (let* ((c (logand ch (logior #x8000000 (lognot (- #x8000000)))))
         (c2 (logand c (lognot (logior #x400000 #x4000000 #x1000000
                                       #x8000000 #x2000000 #x800000))))
         (out ""))
    (if (not (characterp c2))
        (format "[%d]" c)
      (let ((tab-as-ci (and (= c2 ?\t) (/= 0 (logand c #x8000000)))))
        (when (/= 0 (logand c #x400000))
          (setq out (concat out "A-") c (- c #x400000)))
        (when (or (/= 0 (logand c #x4000000))
                  (and (< c2 ?\s) (/= c2 27) (/= c2 ?\t) (/= c2 13))
                  tab-as-ci)
          (setq out (concat out "C-") c (logand c (lognot #x4000000))))
        (when (/= 0 (logand c #x1000000))
          (setq out (concat out "H-") c (- c #x1000000)))
        (when (/= 0 (logand c #x8000000))
          (setq out (concat out "M-") c (- c #x8000000)))
        (when (/= 0 (logand c #x2000000))
          (setq out (concat out "S-") c (- c #x2000000)))
        (when (/= 0 (logand c #x800000))
          (setq out (concat out "s-") c (- c #x800000)))
        (cond
         ((< c ?\s)
          (setq out (concat out
                            (cond ((= c 27) "ESC")
                                  (tab-as-ci "i")
                                  ((= c ?\t) "TAB")
                                  ((= c 13) "RET")
                                  ((and (> c 0) (<= c 26)) (string (+ c 96)))
                                  (t (string (+ c 64)))))))
         ((= c 127) (setq out (concat out "DEL")))
         ((= c ?\s) (setq out (concat out "SPC")))
         (t (setq out (concat out (string c)))))
        out))))

(defun single-key-description (key &optional no-angles)
  "Return a pretty description of a character event KEY.
Control characters turn into C-whatever, etc.
Optional argument NO-ANGLES non-nil means don't put angle brackets
around function keys and event symbols."
  (when (keymap--lucid-event-type-list-p key)
    (setq key (event-convert-list key)))
  (if (and (consp key) (integerp (car key)) (integerp (cdr key)))
      (concat (single-key-description (car key) no-angles) ".."
              (single-key-description (cdr key) no-angles))
    (when (consp key) (setq key (car key)))
    (cond
     ((integerp key) (keymap--push-key-description key))
     ((symbolp key)
      (if no-angles
          (symbol-name key)
        (let* ((name (symbol-name key)) (len (length name)) (i 0))
          (while (and (< i (- len 3)) (eq (aref name (1+ i)) ?-)
                      (memq (aref name i) '(?C ?M ?S ?s ?H ?A)))
            (setq i (+ i 2)))
          (concat (substring name 0 i) "<" (substring name i) ">"))))
     ((stringp key) (copy-sequence key))
     (t (error "KEY must be an integer, cons, symbol, or string")))))

(defun key-description (keys &optional prefix)
  "Return a pretty description of a character sequence KEYS.
Optional arg PREFIX is the sequence of keys leading up to KEYS.
For example, [?\\C-x ?l] is converted into the string \"C-x l\"."
  (length keys)
  (length prefix)
  (let ((parts nil) (add-meta nil))
    (dolist (seq (list prefix keys))
      (let ((items
             (cond ((null seq) nil)
                   ((stringp seq)
                    (mapcar (lambda (c)
                              (if (and (< c 256) (/= 0 (logand c #o200)))
                                  (logxor c (logior #o200 #x8000000))
                                c))
                            (append seq nil)))
                   ((vectorp seq) (append seq nil))
                   ((consp seq) seq)
                   (t (signal 'wrong-type-argument (list 'arrayp seq))))))
        (while items
          (let ((key (car items)) (skip nil))
            (cond
             (add-meta
              (if (or (not (integerp key)) (eq key 27)
                      (/= 0 (logand key #x8000000)))
                  (progn
                    (push (single-key-description 27) parts)
                    (when (eq key 27) (setq skip t)))
                (setq key (logior key #x8000000)))
              (unless skip (setq add-meta nil)))
             ((eq key 27) (setq add-meta t skip t)))
            (unless skip (push (single-key-description key) parts)))
          (setq items (cdr items)))))
    (when add-meta (push (single-key-description 27) parts))
    (mapconcat #'identity (nreverse parts) " ")))

(defun command-remapping (command &optional _position keymaps)
  "Return the remapping for command COMMAND.
Return nil if COMMAND is not remapped (or not a symbol).

If the optional argument POSITION is non-nil, it specifies a mouse
position as returned by `event-start' and `event-end', and the
remapping occurs in the keymaps associated with it.  It can also be a
number or marker, in which case the keymap properties at the specified
buffer position instead of point are used.  The KEYMAPS argument is
ignored if POSITION is non-nil.

If the optional argument KEYMAPS is non-nil, it should be a keymap or
list of keymaps to search for command remapping.  Otherwise, search for the
remapping in all currently active keymaps."
  (and (symbolp command)
       keymaps
       (let ((found nil)
             (maps (if (keymapp keymaps) (list keymaps) keymaps)))
         (while (and maps (not found))
           (let ((r (lookup-key (car maps) (vector 'remap command))))
             (when (and r (not (integerp r))) (setq found r)))
           (setq maps (cdr maps)))
         found)))

;; subr.el's `event-basic-type' reads the `event-symbol-elements' cache, which a
;; symbol has only after something parsed it; Emacs parses every function key
;; the keyboard layer or a keymap has met by the time a script runs, so the
;; common call -- `(event-basic-type 'f1)' -- answers `f1' there.  This parses
;; on demand, which gives that answer without a startup table.
(defun event-basic-type (event)
  "Return the basic type of the given event (all modifiers removed).
The value is a printing character (not upper case) or a symbol.
EVENT may be an event or an event type."
  (unless (stringp event)
    (if (consp event)
        (setq event (car event)))
    (if (symbolp event)
        (car (internal-event-symbol-parse-modifiers event))
      (let* ((base (logand event (1- ?\A-\0)))
             (uncontrolled (if (< base 32) (logior base 64) base)))
        (condition-case ()
            (downcase uncontrolled)
          (error uncontrolled))))))

;;;; Char-table keymaps (keymap.c) and `map-char-table' (chartab.c).

(defun map-char-table (function char-table)
  "Call FUNCTION for each character in CHAR-TABLE that has non-nil value.
FUNCTION is called with two arguments, KEY and VALUE.
KEY is a character code or a cons of character codes specifying a
range of characters that have the same value.
VALUE is what (aref CHAR-TABLE CHAR) returns."
  (let ((range (cons 0 #x3FFFFF)) (val nil) (first t))
    (dolist (run (elisprs--char-table-runs char-table))
      (let ((c (car run)))
        (unless first
          (when val
            (setcdr range (1- c))
            (funcall function (if (eq (car range) (cdr range)) (car range) range)
                     val))
          (setcar range c)
          (setcdr range #x3FFFFF))
        (setq val (cdr run) first nil)))
    (when val
      (funcall function (if (eq (car range) (cdr range)) (car range) range) val))
    nil))

(defun make-keymap (&optional string)
  "Construct and return a new keymap, of the form (keymap CHARTABLE . ALIST).
CHARTABLE is a char-table that holds the bindings for all characters
without modifiers.  All entries in it are initially nil, meaning
\"command undefined\".  ALIST is an assoc-list which holds bindings for
function keys, mouse events, and any other things that appear in the
input stream.  Initially, ALIST is nil.

The optional arg STRING supplies a menu name for the keymap
in case you use it as a menu with `x-popup-menu'."
  (cons 'keymap
        (cons (make-char-table 'keymap)
              (and string (list string)))))

(defun keymap--char-event-p (event)
  (and (natnump event) (< event #x400000)))

;; A char-table or vector among KM's own bindings that holds EVENT.
(defun keymap--slot-holder (km event)
  (let ((tail (cdr km)) (res nil))
    (while (and (consp tail) (not res))
      (let ((el (car tail)))
        (cond ((eq el 'keymap) (setq tail nil))
              ((and (char-table-p el) (keymap--char-event-p event))
               (setq res el))
              ((and (vectorp el) (natnump event) (< event (length el)))
               (setq res el))
              (t (setq tail (cdr tail))))))
    res))

(defun keymap--set-binding (km event def)
  (let ((slot (keymap--slot-holder km event)))
    (if slot
        (aset slot event (if (and (char-table-p slot) (null def)) t def))
      (let ((cell (keymap--own-binding km event)))
        (if cell
            (setcdr cell def)
          (setcdr km (cons (cons event def) (cdr km))))))))

(defun keymap--remove-binding (km event)
  (let ((slot (keymap--slot-holder km event)))
    (if slot
        (aset slot event nil)
      (let ((cell (keymap--own-binding km event)))
        (when cell (setcdr km (delq cell (cdr km))))))))

(defun keymap--own-binding (km event)
  (let ((tail (cdr km)) (res nil))
    (while (and (consp tail) (not res))
      (let ((el (car tail)))
        (cond ((eq el 'keymap) (setq tail nil))
              ((and (char-table-p el) (keymap--char-event-p event)
                    (aref el event))
               (setq res (cons event (let ((v (aref el event)))
                                       (if (eq v t) nil v)))))
              ((and (vectorp el) (natnump event) (< event (length el))
                    (aref el event))
               (setq res (cons event (aref el event))))
              ((and (consp el) (equal (car el) event)) (setq res el))
              (t (setq tail (cdr tail))))))
    res))

(defun lookup-key--event (km event accept-default)
  (let ((tail (cdr km)) (res nil) (deflt nil) (done nil))
    (while (and (consp tail) (not done))
      (let ((el (car tail)))
        (cond
         ((eq el 'keymap)
          (setq res (lookup-key--event tail event accept-default) done t))
         ((and (consp el) (eq (car el) 'keymap))
          (let ((r (lookup-key--event el event accept-default)))
            (when r (setq res r done t))))
         ((and (char-table-p el) (keymap--char-event-p event))
          (let ((v (aref el event)))
            (when v
              (setq res (if (eq v t) nil v) done t))))
         ((and (vectorp el) (natnump event) (< event (length el)))
          (setq res (aref el event) done t))
         ((and (consp el) (equal (car el) event))
          (setq res (keymap--get-keyelt (cdr el)) done t))
         ((and (consp el) (eq (car el) t))
          (setq deflt (keymap--get-keyelt (cdr el))))))
      (unless done (setq tail (cdr tail))))
    (or res (and accept-default deflt))))

;; map_keymap_internal also walks a char-table element.
(defun keymap--map-internal (function map)
  (let ((tail (if (and (consp map) (eq (car map) 'keymap)) (cdr map) map))
        (done nil))
    (while (and (not done) (consp tail) (not (eq (car tail) 'keymap)))
      (let ((binding (car tail)))
        (cond
         ((keymap--keymapp binding) (setq done t))
         ((consp binding)
          (keymap--item function (car binding) (cdr binding)))
         ((vectorp binding)
          (let ((c 0) (n (length binding)))
            (while (< c n)
              (keymap--item function c (aref binding c))
              (setq c (1+ c)))))
         ((char-table-p binding)
          (map-char-table
           (lambda (key val)
             (when val
               (keymap--item function
                             (if (consp key) (cons (car key) (cdr key)) key)
                             val)))
           binding))))
      (unless done (setq tail (cdr tail))))
    tail))

;; store_in_keymap: a binding that fits no existing slot goes right after the
;; last char-table or vector among the keymap's own elements.
(defun keymap--set-binding (km event def)
  (let ((slot (keymap--slot-holder km event)))
    (if slot
        (aset slot event (if (and (char-table-p slot) (null def)) t def))
      (let ((cell (keymap--own-binding km event)))
        (if cell
            (setcdr cell def)
          (let ((insertion km) (tail (cdr km)))
            (while (and (consp tail) (not (eq (car tail) 'keymap)))
              (when (or (char-table-p (car tail)) (vectorp (car tail)))
                (setq insertion tail))
              (setq tail (cdr tail)))
            (setcdr insertion (cons (cons event def) (cdr insertion)))))))))

;;;; accessible-keymaps, where-is-internal (keymap.c).

(defun keymap--get-keyelt (object)
  "get_keyelt: the binding itself of a possible menu item."
  (let ((done nil))
    (while (not done)
      (cond
       ((not (consp object)) (setq done t))
       ((eq (car object) (quote menu-item))
        (if (consp (cdr object))
            (progn (setq object (cdr (cdr object)))
                   (when (consp object) (setq object (car object))))
          (setq done t)))
       ((stringp (car object)) (setq object (cdr object)))
       (t (setq done t))))
    object))

(defun accessible-keymaps (keymap &optional prefix)
  "Find all keymaps accessible via prefix characters from KEYMAP.
Returns a list of elements of the form (KEYS . MAP), where the sequence
KEYS starting from KEYMAP gets you to MAP.  These elements are ordered
so that the KEYS increase in length.  The first element is ([] . KEYMAP).
An optional argument PREFIX, if non-nil, should be a key sequence;
then the value includes only maps for prefixes that start with PREFIX."
  (let* ((prefixlen (if prefix (length prefix) 0))
         (maps nil))
    (if (and prefix (> prefixlen 0))
        (let ((tem (lookup-key keymap prefix t)))
          (unless (keymap--keymapp tem)
            (setq tem nil))
          (setq maps (and tem (list (cons (if (stringp prefix)
                                              (vconcat prefix)
                                            prefix)
                                          (keymap--get-keymap tem t nil))))))
      (setq maps (list (cons [] (keymap--get-keymap keymap t nil)))))
    (let ((tail maps))
      (while (consp tail)
        (let* ((thisseq (car (car tail)))
               (thismap (cdr (car tail)))
               (last (1- (length thisseq)))
               (is-metized (and (>= last 0) (>= last prefixlen)
                                (eq (aref thisseq last) 27)))
               (this-tail tail))
          (when (consp thismap)
            (keymap--map
             (lambda (key cmd)
               (setq cmd (keymap--get-keymap (keymap--get-keyelt cmd) nil nil))
               (when cmd
                 (let ((search maps) (cycle nil) found)
                   (while (and (not cycle)
                               (setq found (rassq cmd search)))
                     (let* ((prefix* (car found)) (lim (length prefix*)))
                       (when (<= lim (length thisseq))
                         (let ((i 0))
                           (while (and (< i lim)
                                       (eq (aref prefix* i) (aref thisseq i)))
                             (setq i (1+ i)))
                           (when (>= i lim) (setq cycle t))))
                       (setq search (cdr (memq found search)))))
                   (unless cycle
                     (if (and is-metized (integerp key))
                         (let ((tem (copy-sequence thisseq)))
                           (aset tem last (logior key #x8000000))
                           (setcdr this-tail
                                   (cons (cons tem cmd) (cdr this-tail))))
                       (nconc this-tail
                              (list (cons (vconcat thisseq (list key))
                                          cmd))))))))
             thismap nil)))
        (setq tail (cdr tail))))
    maps))

(defun keymap--preferred-sequence-p (seq)
  "preferred_sequence_p: 2 for an all-character sequence with a plain key,
1 for one with only modified characters, 0 when an event is not a character."
  (let ((result 1) (len (length seq)) (i 0) (bad nil))
    (while (and (not bad) (< i len))
      (let ((elt (aref seq i)))
        (if (not (integerp elt))
            (setq bad t)
          (let ((modifiers (logand elt (logand (lognot #x8000000) #xFC00000))))
            (cond ((= modifiers 0) (setq result 2))
                  (t (setq bad t))))))
      (setq i (1+ i)))
    (if bad 0 result)))

(defun keymap--shadow-lookup (keymaps key remap)
  (let ((res nil) (done nil))
    (while (and keymaps (not done))
      (let ((value (lookup-key (car keymaps) key)))
        (cond
         ((and (integerp value) (natnump value))
          (when (lookup-key (car keymaps) (substring key 0 value))
            (setq res nil done t)))
         (value
          (setq res (or (and remap (symbolp value)
                             (command-remapping value nil keymaps))
                        value)
                done t))))
      (setq keymaps (cdr keymaps)))
    res))

(defun keymap--where-is-sequences (definition keymaps noindirect)
  "where_is_internal: every key sequence in KEYMAPS bound to DEFINITION."
  (let ((sequences nil))
    (dolist (map keymaps)
      (dolist (entry (accessible-keymaps (keymap--get-keymap map t nil)))
        (let ((this (car entry)) (submap (cdr entry)))
          (keymap--map
           (lambda (key binding)
             (setq binding (if noindirect binding (keymap--get-keyelt binding)))
             (when (or (eq binding definition)
                       (and (consp definition) (equal binding definition)))
               (let ((seq (if (and (> (length this) 0)
                                   (eq (aref this (1- (length this))) 27)
                                   (integerp key)
                                   (< key 128))
                              (let ((tem (copy-sequence this)))
                                (aset tem (1- (length tem)) (logior key #x8000000))
                                tem)
                            (vconcat this (list key)))))
                 (unless (member seq sequences)
                   (push seq sequences)))))
           submap nil))))
    (nreverse sequences)))

(defun where-is-internal (definition &optional keymap firstonly noindirect no-remap)
  "Return list of keys that invoke DEFINITION.
If KEYMAP is a keymap, search only KEYMAP and the global keymap.
If KEYMAP is nil, search all the currently active keymaps, except
 for `overriding-local-map' (which is ignored).
If KEYMAP is a list of keymaps, search only those keymaps.

If optional 3rd arg FIRSTONLY is non-nil, return the first key sequence found,
rather than a list of all possible key sequences.
If FIRSTONLY is the symbol `non-ascii', return the first binding found,
no matter what it is.
If FIRSTONLY has another non-nil value, prefer bindings
that use the modifier key specified in `where-is-preferred-modifier'
\(or their meta variants) and entirely reject menu bindings.

If optional 4th arg NOINDIRECT is non-nil, don't extract the commands inside
menu-items.  This makes it possible to search for a menu-item itself.

The optional 5th arg NO-REMAP alters how command remapping is handled:

- If another command OTHER-COMMAND is remapped to DEFINITION, normally
  search for the bindings of OTHER-COMMAND and include them in the
  returned list.  But if NO-REMAP is non-nil, include the vector
  [remap OTHER-COMMAND] in the returned list instead, without
  searching for those other bindings.

- If DEFINITION is remapped to OTHER-COMMAND, normally return the
  bindings for OTHER-COMMAND.  But if NO-REMAP is non-nil, return the
  bindings for DEFINITION instead, ignoring its remapping."
  (let* ((keymaps (cond ((and (consp keymap) (keymap--keymapp (car keymap)))
                         keymap)
                        (keymap (list keymap))
                        (t nil)))
         (sequences (keymap--where-is-sequences definition keymaps noindirect))
         (remapped-sequences nil) (remapped nil)
         (found nil) (result 'unset))
    (while (and (eq result 'unset)
                (or sequences
                    (and (not remapped)
                         (progn (setq sequences remapped-sequences
                                      remapped t)
                                sequences))))
      (let ((sequence (car sequences)) (skip nil))
        (setq sequences (cdr sequences))
        (unless (equal (keymap--shadow-lookup keymaps sequence remapped)
                       definition)
          (setq skip t))
        (when (and (not skip) (not no-remap) (not remapped)
                   (vectorp sequence) (= (length sequence) 2)
                   (eq (aref sequence 0) 'remap) (symbolp (aref sequence 1)))
          (let ((seqs (keymap--where-is-sequences (aref sequence 1) keymaps
                                                  noindirect)))
            (setq remapped-sequences
                  (nconc (reverse seqs) remapped-sequences)))
          (setq skip t))
        (unless skip
          (when (and (> (length sequence) 0)
                     (stringp (aref sequence (1- (length sequence)))))
            (aset sequence (1- (length sequence)) "(any string)"))
          (unless (member sequence found) (push sequence found))
          (cond ((eq firstonly 'non-ascii) (setq result sequence))
                ((and firstonly (= 2 (keymap--preferred-sequence-p sequence)))
                 (setq result sequence))))))
    (cond ((not (eq result 'unset)) result)
          (t (setq found (nreverse found))
             (if firstonly (car found) found)))))
