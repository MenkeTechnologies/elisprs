;;; replace-31.el --- isearch.el's search core and replace.el's replacement commands  -*- lexical-binding: t -*-

;; Verbatim definitions from GNU Emacs 31.1 lisp/isearch.el, then
;; lisp/replace.el, each in file order: the search function, faces and
;; lazy-highlight state `perform-replace' uses, and `perform-replace' with the
;; commands over it (`replace-string', `replace-regexp', `query-replace',
;; `how-many', `keep-lines', `flush-lines', ...).  The isearch command loop,
;; occur and the minibuffer readers are not here.

;;;; isearch.el

(defcustom search-upper-case 'not-yanks
  "If non-nil, upper case chars disable case fold searching.
That is, upper and lower case chars must match exactly.
This applies no matter where the chars come from, but does not
apply to chars in regexps that are prefixed with `\\'.
If this value is `not-yanks', text yanked into the search string
in Isearch mode is always downcased."
  :type '(choice (const :tag "off" nil)
		 (const not-yanks)
		 (other :tag "on" t)))

(defcustom search-whitespace-regexp "[ \t]+"
  "If non-nil, regular expression to match a sequence of whitespace chars.
When you enter a space or spaces in the incremental search, it
will match any sequence matched by this regexp.  As an exception,
spaces are treated normally in regexp incremental search if they
occur in a regexp construct like [...] or *, + or ?.

If the value is a string, it applies to both ordinary and
regexp incremental search.  If the value is nil, or
`isearch-lax-whitespace' is nil for ordinary incremental search, or
`isearch-regexp-lax-whitespace' is nil for regexp incremental search,
then each space you type matches literally, against one space.

You might want to use something like \"[ \\t\\r\\n]+\" instead.
In the Customization buffer, that is `[' followed by a space, a
tab, a carriage return (control-M), a newline, and `]+'.  Don't
add any capturing groups into this value; that can change the
numbering of existing capture groups in unexpected ways."
  :type '(choice (const :tag "Match Spaces Literally" nil)
                 (const :tag "Tabs and spaces" "[ \t]+")
                 (const :tag "Tabs, spaces and line breaks" "[ \t\n]+")
		 regexp)
  :version "28.1")

(defcustom search-invisible 'open
  "If t incremental search/query-replace can match hidden text.
A nil value means don't match invisible text.
When the value is `open', if the text matched is made invisible by
an overlay having a non-nil `invisible' property, and that overlay
has a non-nil property `isearch-open-invisible', then incremental
search will show the hidden text.  (This applies when using `outline.el'
and `hideshow.el'.)

To temporarily change the value for an active incremental search,
use \\<isearch-mode-map>\\[isearch-toggle-invisible].

See also the related option `isearch-hide-immediately'.

See also `reveal-mode' if you want overlays to automatically be opened
whenever point is in one of them."
  :type '(choice (const :tag "Match hidden text" t)
		 (const :tag "Open overlays" open)
		 (const :tag "Don't match hidden text" nil)))

(defcustom isearch-hide-immediately t
  "If non-nil, re-hide an invisible match right away.
This variable makes a difference when `search-invisible' is set to `open'.
If non-nil, invisible matches are re-hidden as soon as the match moves
off the invisible text surrounding the match.
If nil then do not re-hide opened invisible text when the match moves.
Whatever the value, all opened invisible text is hidden again after exiting
the search, with the exception of the last successful match, if any."
  :type 'boolean)

(defvar isearch-filter-predicate #'isearch-filter-visible
  "Predicate to filter hits of Isearch and replace commands.
Isearch hits that don't satisfy the predicate will be skipped.
The value should be a function of two arguments; it will be
called with the positions of the start and the end of the text
matched by Isearch and replace commands.  If this function
returns nil, Isearch and replace commands will continue searching
without stopping at resp. replacing this match.
This function is expected to be careful not to clobber the match data.

If you use `add-function' to modify this variable, you can use the
`isearch-message-prefix' advice property to specify the prefix string
displayed in the search message.")

(defface isearch
  '((((class color) (min-colors 88) (background light))
     ;; The background must not be too dark, for that means
     ;; the character is hard to see when the cursor is there.
     (:background "magenta3" :foreground "lightskyblue1"))
    (((class color) (min-colors 88) (background dark))
     (:background "palevioletred2" :foreground "brown4"))
    (((class color) (min-colors 16))
     (:background "magenta4" :foreground "cyan1"))
    (((class color) (min-colors 8))
     (:background "magenta4" :foreground "cyan1"))
    (t (:inverse-video t)))
  "Face for highlighting Isearch matches."
  :group 'isearch
  :group 'basic-faces)

(defface isearch-fail
  '((((class color) (min-colors 88) (background light))
     (:background "RosyBrown1"))
    (((class color) (min-colors 88) (background dark))
     (:background "red4"))
    (((class color) (min-colors 16))
     (:background "red"))
    (((class color) (min-colors 8))
     (:background "red"))
    (((class color grayscale))
     :foreground "grey")
    (t (:inverse-video t)))
  "Face for highlighting failed part in Isearch echo-area message."
  :version "23.1")

(defcustom isearch-lazy-count nil
  "Show match numbers in the search prompt.
When both this option and `isearch-lazy-highlight' are non-nil,
show the current match number and the total number of matches
in the buffer (or its restriction), including all hidden matches."
  :type 'boolean
  :group 'lazy-count
  :group 'isearch
  :version "27.1")

(defcustom lazy-highlight-cleanup t
  "Controls whether to remove extra highlighting after a search.
If this is nil, extra highlighting can be \"manually\" removed with
\\[lazy-highlight-cleanup]."
  :type 'boolean
  :group 'lazy-highlight)

(defcustom lazy-highlight-initial-delay 0.25
  "Seconds to wait before beginning to lazily highlight all matches.
This setting only has effect when the search string is shorter than
`lazy-highlight-no-delay-length' characters."
  :type 'number
  :group 'lazy-highlight)

(defcustom lazy-highlight-no-delay-length 3
  "For search strings at least this long, lazy highlight starts immediately.
For shorter search strings, `lazy-highlight-initial-delay'
applies."
  :type 'integer
  :group 'lazy-highlight
  :version "28.1")

(defcustom lazy-highlight-interval 0 ; 0.0625
  "Seconds between lazily highlighting successive matches."
  :type 'number
  :group 'lazy-highlight)

(defcustom lazy-highlight-buffer nil
  "Controls the lazy-highlighting of the full buffer.
When non-nil, all text in the buffer matching the current search
string is highlighted lazily (see `lazy-highlight-initial-delay',
`lazy-highlight-interval' and `lazy-highlight-buffer-max-at-a-time').
This is useful when `lazy-highlight-cleanup' is customized to nil
and doesn't remove full-buffer highlighting after a search.
If this is nil (the default), only the text currently visible in
the window is highlighted, subject to `isearch-lazy-highlight'."
  :type 'boolean
  :group 'lazy-highlight
  :version "27.1")

(defface lazy-highlight
  '((((class color) (min-colors 88) (background light))
     (:background "paleturquoise" :distant-foreground "black"))
    (((class color) (min-colors 88) (background dark))
     (:background "paleturquoise4" :distant-foreground "white"))
    (((class color) (min-colors 16))
     (:background "turquoise3" :distant-foreground "white"))
    (((class color) (min-colors 8))
     (:background "turquoise3" :distant-foreground "white"))
    (t (:underline t)))
  "Face for lazy highlighting of matches other than the current one.
Used in Isearch when `isearch-lazy-highlight' is non-nil,
and in `query-replace' when `query-replace-lazy-highlight' is non-nil."
  :group 'lazy-highlight
  :group 'basic-faces)

(defvar isearch-forward nil)

(defvar isearch-regexp nil)

(defvar isearch-regexp-function nil
  "Regexp-based search mode for words/symbols.
If non-nil, a function to convert a search string to a regexp
used by regexp search functions.

The function should accept 1 or 2 arguments: the original string
to convert, and a flag, whose non-nil value means the match
doesn't have to start or end on a word boundary.  The function
should return the corresponding regexp, a string.

The symbol property `isearch-message-prefix' put on this function
specifies the prefix string displayed in the search message.

Existing functions you could use as values are `word-search-regexp',
`isearch-symbol-regexp', and `char-fold-to-regexp'.

This variable is set and changed during isearch.  To change the
default behavior used for searches, see `search-default-mode'
instead.")

(defcustom isearch-lax-whitespace t
  "If non-nil, a space will match a sequence of whitespace chars.
When you enter a space or spaces in ordinary incremental search, it
will match any sequence matched by the regexp defined by the variable
`search-whitespace-regexp'.  If the value is nil, each space you type
matches literally, against one space.  You can toggle the value of this
variable by the command `isearch-toggle-lax-whitespace', usually bound to
`M-s SPC' during isearch."
  :type 'boolean
  :group 'isearch
  :version "25.1")

(defvar isearch-regexp-lax-whitespace nil
  "If non-nil, a space will match a sequence of whitespace chars.
When you enter a space or spaces in regexp incremental search, it
will match any sequence matched by the regexp defined by the variable
`search-whitespace-regexp'.  If the value is nil, each space you type
matches literally, against one space.  You can toggle the value of this
variable by the command `isearch-toggle-lax-whitespace', usually bound to
`M-s SPC' during isearch.")

(defvar isearch-string "")

(defvar isearch-error nil)

(defvar isearch-other-end nil)

(defvar isearch-case-fold-search nil)

(defvar isearch-invisible search-invisible)

(defvar isearch-adjusted nil)

(defvar isearch-yank-flag nil)

(defvar isearch-nonincremental nil)

(defvar isearch-opened-overlays nil)

(defun isearch-forward (&optional regexp-p no-recursive-edit)
  "Do incremental search forward.
With a prefix argument, do an incremental regular expression search instead.
\\<isearch-mode-map>
As you type characters, they add to the search string and are found.
Current match for the search string is highlighted using the `isearch' face,
and if `isearch-lazy-highlight' is non-nil, the other matches are
highlighted using the `lazy-highlight' face.
The following non-printing keys are bound in `isearch-mode-map'.

Type \\[isearch-delete-char] to cancel last input item from end of search string.
Type \\[isearch-exit] to exit, leaving point at location found.
Type LFD (\\`C-j') to match end of line.
Type \\[isearch-repeat-forward] to search again forward,\
 \\[isearch-repeat-backward] to search again backward.
Type \\[isearch-beginning-of-buffer] to go to the first match,\
 \\[isearch-end-of-buffer] to go to the last match.
Type \\[isearch-yank-word-or-char] to yank next word or character in buffer
  onto the end of the search string, and search for it.
Type \\[isearch-del-char] to delete character from end of search string.
Type \\[isearch-yank-char] to yank char from buffer onto end of search\
 string and search for it.
Type \\[isearch-yank-until-char] to yank from point until the next instance of a
 specified character onto end of search string and search for it.
Type \\[isearch-yank-line] to yank rest of line onto end of search string\
 and search for it.
Type \\[isearch-yank-kill] to yank the last string of killed text.
Type \\[isearch-yank-pop-only] to replace string just yanked into search prompt
 with string killed before it.
Type \\[isearch-quote-char] to quote control character to search for it.
Type \\[isearch-char-by-name] to add a character to search by Unicode name,\
 with completion.
\\[isearch-abort] while searching or when search has failed cancels input\
 back to what has
 been found successfully.
\\[isearch-abort] when search is successful aborts and moves point to\
 starting point.

If you try to exit with the search string still empty, it invokes
 nonincremental search.

Type \\[isearch-toggle-case-fold] to toggle search case-sensitivity.
Type \\[isearch-toggle-invisible] to toggle search in invisible text.
Type \\[isearch-toggle-regexp] to toggle regular-expression mode.
Type \\[isearch-toggle-word] to toggle word mode.
Type \\[isearch-toggle-symbol] to toggle symbol mode.
Type \\[isearch-toggle-char-fold] to toggle character folding.

Type \\[isearch-toggle-lax-whitespace] to toggle whitespace matching.
In incremental searches, a space or spaces normally matches any whitespace
defined by the variable `search-whitespace-regexp'; see also the variables
`isearch-lax-whitespace' and `isearch-regexp-lax-whitespace'.

Type \\[isearch-edit-string] to edit the search string in the minibuffer.

Also supported is a search ring of the previous 16 search strings.
Type \\[isearch-ring-advance] to search for the next item in the search ring.
Type \\[isearch-ring-retreat] to search for the previous item in the search\
 ring.
Type \\[isearch-complete] to complete the search string using the search ring.

Type \\[isearch-query-replace] to run `query-replace' with string to\
 replace from last search string.
Type \\[isearch-query-replace-regexp] to run `query-replace-regexp'\
 with the last search string.
Type \\[isearch-occur] to run `occur' that shows\
 the last search string.
Type \\[isearch-highlight-regexp] to run `highlight-regexp'\
 that highlights the last search string.
Type \\[isearch-highlight-lines-matching-regexp] to run\
 `highlight-lines-matching-regexp' that highlights lines
 matching the last search string.

Type \\[isearch-describe-bindings] to display all Isearch key bindings.
Type \\[isearch-describe-key] to display documentation of Isearch key.
Type \\[isearch-describe-mode] to display documentation of Isearch mode.

If an input method is turned on in the current buffer, that input
method is also active while you are typing characters to search.
To toggle the input method, type \\[isearch-toggle-input-method].  \
It also toggles the input
method in the current buffer.

To use a different input method for searching, type \
\\[isearch-toggle-specified-input-method],
and specify an input method you want to use.

To activate a transient input method, type \\[isearch-transient-input-method].

The above keys, bound in `isearch-mode-map', are often controlled by
 options; do \\[apropos] on search-.* to find them.
Other control and meta characters terminate the search
 and are then executed normally (depending on `search-exit-option').
Likewise for function keys and mouse button events.

If this function is called non-interactively with a nil NO-RECURSIVE-EDIT,
it does not return to the calling function until the search is done.
See the function `isearch-mode' for more information."

  (interactive "P\np")
  (isearch-mode t (not (null regexp-p)) nil (not no-recursive-edit)))

(defun word-search-regexp (string &optional lax)
  "Return a regexp which matches words, ignoring punctuation.
Given STRING, a string of words separated by word delimiters,
compute a regexp that matches those exact words separated by
arbitrary punctuation.  If the string begins or ends in whitespace,
the beginning or the end of the string matches arbitrary whitespace.
Otherwise if LAX is non-nil, the beginning or the end of the string
need not match a word boundary.

Used in `word-search-forward', `word-search-backward',
`word-search-forward-lax', `word-search-backward-lax'."
  (cond
   ((equal string "") "")
   ((string-match-p "\\`\\W+\\'" string) "\\W+")
   (t (concat
       (if (string-match-p "\\`\\W" string) "\\W+"
	 "\\<")
       (mapconcat 'regexp-quote (split-string string "\\W+" t) "\\W+")
       (if (string-match-p "\\W\\'" string) "\\W+"
	 (unless lax "\\>"))))))

(defvar isearch-search-fun-function 'isearch-search-fun-default
  "Non-default value overrides the behavior of `isearch-search-fun-default'.
This variable's value should be a function, which will be called
with no arguments, and should return a function that takes three
arguments: STRING, BOUND, and NOERROR.  STRING is the string to
be searched for.  See `re-search-forward' for the meaning of
BOUND and NOERROR arguments.

This returned function will be used by `isearch-search-string' to
search for the first occurrence of STRING.")

(defun isearch-search-fun ()
  "Return the function to use for the search.
Can be changed via `isearch-search-fun-function' for special needs."
  (funcall (or isearch-search-fun-function 'isearch-search-fun-default)))

(defun isearch--lax-regexp-function-p ()
  "Non-nil if next regexp-function call should be lax."
  (or (memq this-command '(isearch-printing-char isearch-del-char))
      isearch-yank-flag))

(defun isearch-search-fun-default ()
  "Return default functions to use for the search."
  (lambda (string &optional bound noerror count)
    (let (;; Evaluate this before binding `search-spaces-regexp' which
          ;; can break all sorts of regexp searches.  In particular,
          ;; calling `isearch-regexp-function' can trigger autoloading
          ;; (Bug#35802).
          (regexp
           (cond (isearch-regexp-function
                  (let ((lax (and (not bound) ; not lazy-highlight
                                  (isearch--lax-regexp-function-p))))
                    (when lax
                      (setq isearch-adjusted 'lax))
                    (if (functionp isearch-regexp-function)
                        (funcall isearch-regexp-function string lax)
                      (word-search-regexp string lax))))
                 (isearch-regexp string)
                 (t (regexp-quote string))))
          ;; Use lax versions to not fail at the end of the word while
          ;; the user adds and removes characters in the search string
          ;; (or when using nonincremental word isearch)
          (search-spaces-regexp (when (if isearch-regexp
                                          isearch-regexp-lax-whitespace
                                        isearch-lax-whitespace)
                                  search-whitespace-regexp)))
      (funcall
       (if isearch-forward #'re-search-forward #'re-search-backward)
       regexp bound noerror count))))

(defun isearch-clean-overlays ()
  (when isearch-opened-overlays
    (mapc 'isearch-open-necessary-overlays isearch-opened-overlays)
    (setq isearch-opened-overlays nil)))

(defun isearch-close-unnecessary-overlays (beg end)
  (let ((overlays isearch-opened-overlays))
    (setq isearch-opened-overlays nil)
    (dolist (ov overlays)
      (when (overlay-buffer ov)
        (if (isearch-intersects-p beg end (overlay-start ov) (overlay-end ov))
            (push ov isearch-opened-overlays)
          (let ((fct-temp (overlay-get ov 'isearch-open-invisible-temporary)))
            (if fct-temp
                ;; If this exists it means that the overlay was opened
                ;; using this function, not by us tweaking the overlay
                ;; properties.
                (funcall fct-temp ov t)
              (overlay-put ov 'invisible (overlay-get ov 'isearch-invisible))
              (overlay-put ov 'isearch-invisible nil))))))))

(defun isearch-range-invisible (beg end)
  "Return t if all the text from BEG to END is invisible."
  (when (/= beg end)
    ;; Check that invisibility runs up to END.
    (save-excursion
      (goto-char beg)
      (let (;; can-be-opened keeps track if we can open some overlays.
	    (can-be-opened (memq search-invisible '(open can-be-opened)))
	    ;; the list of overlays that could be opened
	    (crt-overlays nil))
	(when (and can-be-opened isearch-hide-immediately
		   (not (eq search-invisible 'can-be-opened)))
	  (isearch-close-unnecessary-overlays beg end))
	;; If the following character is currently invisible,
	;; skip all characters with that same `invisible' property value.
	;; Do that over and over.
	(while (and (< (point) end) (invisible-p (point)))
	  (if (invisible-p (get-text-property (point) 'invisible))
	      (progn
		(goto-char (next-single-property-change (point) 'invisible
							nil end))
		;; if text is hidden by an `invisible' text property
		;; we cannot open it at all.
		(setq can-be-opened nil))
	    (when can-be-opened
	      (let ((overlays (overlays-at (point)))
		    ov-list
		    o
		    invis-prop)
		(while overlays
		  (setq o (car overlays)
			invis-prop (overlay-get o 'invisible))
		  (if (invisible-p invis-prop)
		      (if (overlay-get o 'isearch-open-invisible)
			  (setq ov-list (cons o ov-list))
			;; We found one overlay that cannot be
			;; opened, that means the whole chunk
			;; cannot be opened.
			(setq can-be-opened nil)))
		  (setq overlays (cdr overlays)))
		(if can-be-opened
		    ;; It makes sense to append to the open
		    ;; overlays list only if we know that this is
		    ;; t.
		    (setq crt-overlays (append ov-list crt-overlays)))))
	    (goto-char (next-overlay-change (point)))))
	;; See if invisibility reaches up thru END.
	(if (>= (point) end)
	    (if (and can-be-opened (consp crt-overlays))
		(progn
		  (unless (eq search-invisible 'can-be-opened)
		    (setq isearch-opened-overlays
			  (append isearch-opened-overlays crt-overlays))
		    (mapc 'isearch-open-overlay-temporary crt-overlays))
		  nil)
	      (setq isearch-hidden t)))))))

(defun isearch-filter-visible (beg end)
  "Return non-nil if text between BEG and END is deemed visible by Isearch.
This function is intended to be used as `isearch-filter-predicate'.
It returns non-nil if the text between BEG and END is visible to
Isearch, at least partially, as determined by `isearch-range-invisible'.
If `search-invisible' is t, which allows Isearch matches inside
invisible text, this function will always return non-nil, regardless
of what `isearch-range-invisible' says."
  (and (not (text-property-not-all beg end 'inhibit-isearch nil))
       (or (eq search-invisible t)
           (not (isearch-range-invisible beg end)))))

(defun isearch-no-upper-case-p (string regexp-flag)
  "Return t if there are no upper case chars in STRING.
If REGEXP-FLAG is non-nil, disregard letters preceded by `\\' (but not `\\\\')
since they have special meaning in a regexp."
  (let (quote-flag (i 0) (len (length string)) found)
    (while (and (not found) (< i len))
      (let ((char (aref string i)))
	(if (and regexp-flag (eq char ?\\))
	    (setq quote-flag (not quote-flag))
	  (if (and (not quote-flag) (not (eq char (downcase char))))
	      (setq found t))
	  (setq quote-flag nil)))
      (setq i (1+ i)))
    (not (or found
             ;; Even if there's no uppercase char, we want to detect the use
             ;; of [:upper:] or [:lower:] char-class, which indicates
             ;; clearly that the user cares about case distinction.
             (and regexp-flag (string-match "\\[:\\(upp\\|low\\)er:]" string)
                  (condition-case err
                      (progn
                        (string-match (substring string 0 (match-beginning 0))
                                      "")
                        nil)
                    (invalid-regexp
                     (equal "Unmatched [ or [^" (cadr err)))))))))

(defface isearch-group-1
  '((((class color) (min-colors 88) (background light))
     (:background "#f000f0" :foreground "lightskyblue1"))
    (((class color) (min-colors 88) (background dark))
     (:background "palevioletred1" :foreground "brown4"))
    (t (:inherit isearch)))
  "Face for highlighting Isearch the odd group matches."
  :group 'isearch
  :version "28.1")

(defface isearch-group-2
  '((((class color) (min-colors 88) (background light))
     (:background "#a000a0" :foreground "lightskyblue1"))
    (((class color) (min-colors 88) (background dark))
     (:background "palevioletred3" :foreground "brown4"))
    (t (:inherit isearch)))
  "Face for highlighting Isearch the even group matches."
  :group 'isearch
  :version "28.1")

(defvar isearch-lazy-highlight-overlays nil)

(defvar isearch-lazy-highlight-timer nil)

(defvar isearch-lazy-highlight-last-string nil)

(defun lazy-highlight-cleanup (&optional force procrastinate)
  "Stop lazy highlighting and remove extra highlighting from current buffer.
FORCE non-nil means do it whether or not `lazy-highlight-cleanup' is nil.
PROCRASTINATE non-nil means postpone cleanup to a later time.
This function is called when exiting an incremental search if
`lazy-highlight-cleanup' is non-nil."
  (interactive '(t))
  (when (and (or force lazy-highlight-cleanup) (not procrastinate))
    (while isearch-lazy-highlight-overlays
      (delete-overlay (car isearch-lazy-highlight-overlays))
      (setq isearch-lazy-highlight-overlays
            (cdr isearch-lazy-highlight-overlays))))
  (when isearch-lazy-highlight-timer
    (cancel-timer isearch-lazy-highlight-timer)
    (setq isearch-lazy-highlight-timer nil)))

(defun isearch-lazy-highlight-new-loop (&optional beg end)
  "Cleanup any previous `lazy-highlight' loop and begin a new one.
BEG and END specify the bounds within which highlighting should occur.
This is called when `isearch-update' is invoked (which can cause the
search string to change or the window to scroll).  It is also used
by other Emacs features."
  (when (and (null executing-kbd-macro)
             ;; This used to read `(sit-for 0)', but that has proved
             ;; unreliable when called from within
             ;; after-change-functions bound to certain special events.
             (redisplay)         ;make sure (window-start) is credible
             (or (not (equal isearch-string
                             isearch-lazy-highlight-last-string))
                 (not (memq (selected-window)
                            isearch-lazy-highlight-window-group))
		 (not (eq isearch-lazy-highlight-case-fold-search
			  isearch-case-fold-search))
		 (not (eq isearch-lazy-highlight-invisible
		          isearch-invisible))
		 (not (eq isearch-lazy-highlight-regexp
			  isearch-regexp))
		 (not (eq isearch-lazy-highlight-regexp-function
			  isearch-regexp-function))
		 (not (eq isearch-lazy-highlight-lax-whitespace
			  isearch-lax-whitespace))
		 (not (eq isearch-lazy-highlight-regexp-lax-whitespace
			  isearch-regexp-lax-whitespace))
		 (not (eq isearch-forward
			  isearch-lazy-highlight-forward))
		 ;; In case we are recovering from an error.
		 (not (equal isearch-error
			     isearch-lazy-highlight-error))
		 (if lazy-highlight-buffer
		     (not (= (point-min)
			     isearch-lazy-highlight-point-min))
		   (setq isearch-lazy-highlight-window-start-changed
			 (not (= (window-group-start)
			         isearch-lazy-highlight-window-start))))
		 (if lazy-highlight-buffer
		     (not (= (point-max)
			     isearch-lazy-highlight-point-max))
		   (setq isearch-lazy-highlight-window-end-changed
			 (not (= (window-group-end) ; Window may have been split/joined.
			         isearch-lazy-highlight-window-end))))))
    ;; something important did indeed change
    (lazy-highlight-cleanup t (not (equal isearch-string ""))) ;stop old timer
    (when isearch-lazy-count
      (when (or (equal isearch-string "")
                ;; Check if this place was reached by a condition above
                ;; other than changed window boundaries (that shouldn't
                ;; reset the counter)
                (and (not isearch-lazy-highlight-window-start-changed)
                     (not isearch-lazy-highlight-window-end-changed))
                ;; Also check for changes in buffer boundaries in
                ;; a possibly narrowed buffer in case lazy-highlight-buffer
                ;; is nil, thus the same check was not performed above
                (not (= (point-min)
                        isearch-lazy-highlight-point-min))
                (not (= (point-max)
                        isearch-lazy-highlight-point-max)))
        ;; Reset old counter before going to count new numbers
        (clrhash isearch-lazy-count-hash)
        (setq isearch-lazy-count-current nil
              isearch-lazy-count-total nil
              isearch-lazy-count-invisible nil)
        ;; Delay updating the message if possible, to avoid flicker
        (when (string-equal isearch-string "")
          (when (and isearch-mode (null isearch-message-function))
            (isearch-message))
          (run-hooks 'lazy-count-update-hook))))
    (setq isearch-lazy-highlight-window-start-changed nil)
    (setq isearch-lazy-highlight-window-end-changed nil)
    (setq isearch-lazy-highlight-error isearch-error)
    ;; It used to check for `(not isearch-error)' here, but actually
    ;; lazy-highlighting might find matches to highlight even when
    ;; `isearch-error' is non-nil.  (Bug#9918)
    (setq isearch-lazy-highlight-start-limit beg
	  isearch-lazy-highlight-end-limit end)
    (setq isearch-lazy-highlight-window       (selected-window)
          isearch-lazy-highlight-window-group (selected-window-group)
	  isearch-lazy-highlight-window-start (window-group-start)
	  isearch-lazy-highlight-window-end   (window-group-end)
	  isearch-lazy-highlight-point-min    (point-min)
	  isearch-lazy-highlight-point-max    (point-max)
	  isearch-lazy-highlight-buffer       lazy-highlight-buffer
	  ;; Start lazy-highlighting at the beginning of the found
	  ;; match (`isearch-other-end').  If no match, use point.
	  ;; One of the next two variables (depending on search direction)
	  ;; is used to define the starting position of lazy-highlighting
	  ;; and also to remember the current position of point between
	  ;; calls of `isearch-lazy-highlight-update', and another variable
	  ;; is used to define where the wrapped search must stop.
	  isearch-lazy-highlight-start        (or isearch-other-end (point))
	  isearch-lazy-highlight-end          (or isearch-other-end (point))
	  isearch-lazy-highlight-wrapped      nil
	  isearch-lazy-highlight-last-string  isearch-string
	  isearch-lazy-highlight-case-fold-search isearch-case-fold-search
	  isearch-lazy-highlight-invisible isearch-invisible
	  isearch-lazy-highlight-regexp       isearch-regexp
	  isearch-lazy-highlight-lax-whitespace   isearch-lax-whitespace
	  isearch-lazy-highlight-regexp-lax-whitespace isearch-regexp-lax-whitespace
	  isearch-lazy-highlight-regexp-function  isearch-regexp-function
	  isearch-lazy-highlight-forward      isearch-forward)
    ;; Extend start/end to match whole string at point (bug#19353)
    (if isearch-lazy-highlight-forward
        (setq isearch-lazy-highlight-start
	      (min (+ isearch-lazy-highlight-start
		      (1- (length isearch-lazy-highlight-last-string)))
		   (point-max)))
      (setq isearch-lazy-highlight-end
	    (max (- isearch-lazy-highlight-end
		    (1- (length isearch-lazy-highlight-last-string)))
		 (point-min))))
    (unless (equal isearch-string "")
      (setq isearch-lazy-highlight-timer
            (run-with-idle-timer (if (>= (length isearch-string)
                                         lazy-highlight-no-delay-length)
                                     0
                                   lazy-highlight-initial-delay)
                                 nil
                                 'isearch-lazy-highlight-start))))
  ;; Update the current match number only in isearch-mode and
  ;; unless isearch-mode is used specially with isearch-message-function
  (when isearch-lazy-count
    ;; Update isearch-lazy-count-current only when it was already set
    ;; at the end of isearch-lazy-highlight-buffer-update
    (when isearch-lazy-count-current
      (setq isearch-lazy-count-current
            (gethash (point) isearch-lazy-count-hash 0))
      (when (and isearch-mode (null isearch-message-function))
        (isearch-message))
      (run-hooks 'lazy-count-update-hook))))

;;;; replace.el

(defcustom case-replace t
  "Non-nil means `query-replace' should preserve case in replacements."
  :type 'boolean
  :group 'matching)

(defcustom replace-char-fold nil
  "Non-nil means replacement commands should do character folding in matches.
This means, for instance, that \\=' will match a large variety of
Unicode quotes.
This variable affects `query-replace' and `replace-string', but not
`replace-regexp'."
  :type 'boolean
  :group 'matching
  :version "25.1")

(defcustom replace-lax-whitespace nil
  "Non-nil means `query-replace' matches a sequence of whitespace chars.
When you enter a space or spaces in the strings to be replaced,
it will match any sequence matched by the regexp `search-whitespace-regexp'."
  :type 'boolean
  :group 'matching
  :version "24.3")

(defcustom replace-regexp-lax-whitespace nil
  "Non-nil means `query-replace-regexp' matches a sequence of whitespace chars.
When you enter a space or spaces in the regexps to be replaced,
it will match any sequence matched by the regexp `search-whitespace-regexp'."
  :type 'boolean
  :group 'matching
  :version "24.3")

(defvar query-replace-history nil
  "Default history list for `query-replace' commands.
See `query-replace-from-history-variable' and
`query-replace-to-history-variable'.")

(defvar query-replace-defaults nil
  "Default values of FROM-STRING and TO-STRING for `query-replace'.
This is a list of cons cells (FROM-STRING . TO-STRING), or nil
if there are no default values.")

(defcustom query-replace-from-to-separator " → "
  "String that separates FROM and TO in the history of replacement pairs.
When nil, the pair will not be added to the history (same behavior
as in Emacs 24.5)."
  :group 'matching
  :type '(choice
          (const :tag "Disabled" nil)
          string)
  :version "25.1")

(defcustom query-replace-from-history-variable 'query-replace-history
  "History list to use for the FROM argument of `query-replace' commands.
The value of this variable should be a symbol; that symbol
is used as a variable to hold a history list for the strings
or patterns to be replaced."
  :group 'matching
  :type 'symbol
  :version "20.3")

(defcustom query-replace-to-history-variable 'query-replace-history
  "History list to use for the TO argument of `query-replace' commands.
The value of this variable should be a symbol; that symbol
is used as a variable to hold a history list for replacement
strings or patterns."
  :group 'matching
  :type 'symbol
  :version "20.3")

(defcustom query-replace-skip-read-only nil
  "Non-nil means `query-replace' and friends ignore read-only matches."
  :type 'boolean
  :group 'matching
  :version "22.1")

(defcustom query-replace-show-replacement t
  "Non-nil means show substituted replacement text in the minibuffer.
This variable affects only `query-replace-regexp'."
  :type 'boolean
  :group 'matching
  :version "23.1")

(defcustom query-replace-highlight t
  "Non-nil means to highlight matches during query replacement."
  :type 'boolean
  :group 'matching)

(defcustom query-replace-highlight-submatches t
  "Whether to highlight regexp subexpressions during query replacement.
The faces used to do the highlights are named `isearch-group-1',
`isearch-group-2', etc.  (By default, only these 2 are defined.)
When there are more matches than faces, then faces are reused from the
beginning, in a cyclical manner, so the `isearch-group-1' face is
isreused for the third match.  If you want to use more distinctive colors,
you can define more of these faces using the same numbering scheme."
  :type 'boolean
  :group 'matching
  :version "28.1")

(defcustom query-replace-lazy-highlight t
  "Controls the lazy-highlighting during query replacements.
When non-nil, all text matching the current match that is
currently visible in the window is highlighted lazily using
isearch lazy highlighting (see `lazy-highlight-initial-delay'
and `lazy-highlight-interval').  Uses the `lazy-highlight' face
to highlight matching text."
  :type 'boolean
  :group 'lazy-highlight
  :group 'matching
  :version "22.1")

(defface query-replace
  '((t (:inherit isearch)))
  "Face for highlighting query replacement matches.
Used in `query-replace' and `query-replace-regexp'
when `query-replace-highlight' is non-nil"
  :group 'matching
  :version "22.1")

(defvar replace-count 0
  "Number of replacements done so far.
See `replace-regexp'.")

(defun query-replace-descr (string)
  (setq string (copy-sequence string))
  (dotimes (i (length string))
    (let ((c (aref string i)))
      (cond
       ((< c ?\s) (add-text-properties
                   i (1+ i)
                   `(display ,(propertize (format "^%c" (+ c 64)) 'face 'escape-glyph))
                   string))
       ((= c ?\^?) (add-text-properties
	            i (1+ i)
                    `(display ,(propertize "^?" 'face 'escape-glyph))
                    string)))))
  string)

(defun query-replace-compile-replacement (to regexp-flag)
  "Maybe convert a regexp replacement TO to Lisp.
REGEXP-FLAG non-nil means TO is a regexp.
Returns a list suitable for `perform-replace' if necessary,
the original string if not."
  (if (and regexp-flag
	   (string-match "\\(\\`\\|[^\\]\\)\\(\\\\\\\\\\)*\\\\[,#]" to))
      (let (pos list char)
	(while
	    (progn
	      (setq pos (match-end 0))
	      (push (substring to 0 (- pos 2)) list)
	      (setq char (aref to (1- pos))
		    to (substring to pos))
	      (cond ((eq char ?\#)
		     (push '(number-to-string replace-count) list))
		    ((eq char ?\,)
		     (setq pos (read-from-string to))
		     (push `(replace-quote ,(car pos)) list)
		     (let ((end
			    ;; Swallow a space after a symbol
			    ;; if there is a space.
			    (if (and (or (symbolp (car pos))
					 ;; Swallow a space after 'foo
					 ;; but not after (quote foo).
					 (and (eq (car-safe (car pos)) 'quote)
					      (not (= ?\( (aref to 0)))))
				     (eq (string-search " " to (cdr pos))
					 (cdr pos)))
				(1+ (cdr pos))
			      (cdr pos))))
		       (setq to (substring to end)))))
	      (string-match "\\(\\`\\|[^\\]\\)\\(\\\\\\\\\\)*\\\\[,#]" to)))
	(setq to (nreverse (delete "" (cons to list))))
	(replace-match-string-symbols to)
	(cons #'replace-eval-replacement
	      (if (cdr to)
		  (cons 'concat to)
		(car to))))
    to))

(defun query-replace (from-string to-string &optional delimited start end backward region-noncontiguous-p)
  "Replace some occurrences of FROM-STRING with TO-STRING.
As each match is found, the user must type a character saying
what to do with it.  Type SPC or `y' to replace the match,
DEL or `n' to skip and go to the next match.  For more directions,
type \\[help-command] at that time.

In Transient Mark mode, if the mark is active, operate on the contents
of the region.  Otherwise, operate from point to the end of the buffer's
accessible portion.

The current match of FROM-STRING is highlighted using
the `query-replace' face.  Other matches of FROM-STRING are highlighted
using the `lazy-highlight' face if `query-replace-lazy-highlight' is
non-nil.

In interactive use, the prefix arg (non-nil DELIMITED in
non-interactive use), means replace only matches surrounded by
word boundaries.  A negative prefix arg means replace backward.

Use \\<minibuffer-local-map>\\[next-history-element] \
to pull the last incremental search string to the minibuffer
that reads FROM-STRING, or invoke replacements from
incremental search with a key sequence like \\`C-s C-s M-%'
to use its current search string as the string to replace.

Matching is independent of case if both `case-fold-search'
and `search-upper-case' are non-nil and FROM-STRING has no
uppercase letters; if `search-upper-case' is nil, then
whether matching ignores case depends on `case-fold-search'
regardless of whether there are uppercase letters in FROM-STRING.
Replacement transfers the case pattern of the old text to the
new text, if both `case-fold-search' and `case-replace' are
non-nil and FROM-STRING has no uppercase letters.
\(Transferring the case pattern means that if the old text
matched is all caps, or all of its words are capitalized, then its
replacement is respectively upcased or capitalized.  For more
details about this, see `replace-match'.)

Ignore read-only matches if `query-replace-skip-read-only' is non-nil,
ignore hidden matches if `search-invisible' is nil, and ignore more
matches using `isearch-filter-predicate'.

If `replace-lax-whitespace' is non-nil, a space or spaces in the string
to be replaced will match a sequence of whitespace chars defined by the
regexp in `search-whitespace-regexp'.

If `replace-char-fold' is non-nil, matching uses character folding,
i.e. it ignores diacritics and other differences between equivalent
character strings.

Fourth and fifth arg START and END specify the region to operate on.

Arguments FROM-STRING, TO-STRING, DELIMITED, START, END, BACKWARD, and
REGION-NONCONTIGUOUS-P are passed to `perform-replace' (which see).
\(TO-STRING is passed to `perform-replace' as REPLACEMENTS and
DELIMITED is passed as DELIMITED-FLAG.)

To customize possible responses, change the bindings in `query-replace-map'."
  (declare (interactive-args
	    (start (use-region-beginning))
	    (end (use-region-end))
	    (region-noncontiguous-p (use-region-noncontiguous-p))))
  (interactive
   (let ((common
	  (query-replace-read-args
	   (concat "Query replace"
		   (if current-prefix-arg
		       (if (eq current-prefix-arg '-) " backward" " word")
		     "")
		   (if (use-region-p) " in region" ""))
	   nil)))
     (list (nth 0 common) (nth 1 common) (nth 2 common)
	   ;; These are done separately here
	   ;; so that command-history will record these expressions
	   ;; rather than the values they had this time.
	   (use-region-beginning) (use-region-end)
	   (nth 3 common)
	   (use-region-noncontiguous-p))))
  (perform-replace from-string to-string t nil delimited nil nil start end backward region-noncontiguous-p))

(defun query-replace-regexp (regexp to-string &optional delimited start end backward region-noncontiguous-p)
  "Replace some things after point matching REGEXP with TO-STRING.
As each match is found, the user must type a character saying
what to do with it.  Type \\`SPC' or \\`y' to replace the match,
\\`DEL' or \\`n' to skip and go to the next match.  For more directions,
type \\[help-command] at that time.

In Transient Mark mode, if the mark is active, operate on the contents
of the region.  Otherwise, operate from point to the end of the buffer's
accessible portion.

When invoked interactively, matching a newline with `\\n' will not work;
use \\`C-q C-j' instead.  To match a tab character (`\\t'), just press \\`TAB'.

The current match of REGEXP is highlighted using
the `query-replace' face.  Other matches of REGEXP are highlighted
using the `lazy-highlight' face if `query-replace-lazy-highlight' is
non-nil.

Use \\<minibuffer-local-map>\\[next-history-element] \
to pull the last incremental search regexp to the minibuffer
that reads REGEXP, or invoke replacements from
incremental search with a key sequence like \\`C-M-s C-M-s C-M-%'
to use its current search regexp as the regexp to replace.

Matching is independent of case if both `case-fold-search'
and `search-upper-case' are non-nil and REGEXP has no uppercase
letters; if `search-upper-case' is nil, then whether matching
ignores case depends on `case-fold-search' regardless of whether
there are uppercase letters in REGEXP.
Replacement transfers the case pattern of the old text to the new
text, if both `case-fold-search' and `case-replace' are non-nil
and REGEXP has no uppercase letters.  (Transferring the case pattern
means that if the old text matched is all caps, or all of its words
are capitalized, then its replacement is respectively upcased or
capitalized.  For more details about this, see `replace-match'.)

Ignore read-only matches if `query-replace-skip-read-only' is non-nil,
ignore hidden matches if `search-invisible' is nil, and ignore more
matches using `isearch-filter-predicate'.

If `replace-regexp-lax-whitespace' is non-nil, a space or spaces in the regexp
to be replaced will match a sequence of whitespace chars defined by the
regexp in `search-whitespace-regexp'.

This function is not affected by `replace-char-fold'.

Third arg DELIMITED (prefix arg if interactive), if non-nil, means replace
only matches surrounded by word boundaries.  A negative prefix arg means
replace backward.

Fourth and fifth arg START and END specify the region to operate on.

In TO-STRING, `\\&' stands for whatever matched the whole of REGEXP,
and `\\=\\N' (where N is a digit) stands for whatever matched
the Nth `\\(...\\)' (1-based) in REGEXP.  The `\\(...\\)' groups are
counted from 1.
`\\?' lets you edit the replacement text in the minibuffer
at the given position for each replacement.

In interactive calls, the replacement text can contain `\\,'
followed by a Lisp expression.  Each
replacement evaluates that expression to compute the replacement
string.  Inside of that expression, `\\&' is a string denoting the
whole match as a string, `\\N' for a partial match, `\\#&' and `\\#N'
for the whole or a partial match converted to a number with
`string-to-number', and `\\#' itself for the number of replacements
done so far (starting with zero).

If the replacement expression is a symbol, write a space after it
to terminate it.  One space there, if any, will be discarded.

When using those Lisp features interactively in the replacement
text, TO-STRING is actually made a list instead of a string.
Use \\[repeat-complex-command] after this command for details.

Arguments REGEXP, TO-STRING, DELIMITED, START, END, BACKWARD, and
REGION-NONCONTIGUOUS-P are passed to `perform-replace' (which see).
\(REGEXP is passed to `perform-replace' as FROM-STRING,
TO-STRING is passed as REPLACEMENTS, and DELIMITED is passed
as DELIMITED-FLAG.)"
  (declare (interactive-args
	    (start (use-region-beginning))
	    (end (use-region-end))
	    (region-noncontiguous-p (use-region-noncontiguous-p))))
  (interactive
   (let ((common
	  (query-replace-read-args
	   (concat "Query replace"
		   (if current-prefix-arg
		       (if (eq current-prefix-arg '-) " backward" " word")
		     "")
		   " regexp"
		   (if (use-region-p) " in region" ""))
	   t)))
     (list (nth 0 common) (nth 1 common) (nth 2 common)
	   ;; These are done separately here
	   ;; so that command-history will record these expressions
	   ;; rather than the values they had this time.
	   (use-region-beginning) (use-region-end)
	   (nth 3 common)
	   (use-region-noncontiguous-p))))
  (perform-replace regexp to-string t t delimited nil nil start end backward region-noncontiguous-p))

(defun map-query-replace-regexp (regexp to-strings &optional n start end region-noncontiguous-p)
  "Replace some matches for REGEXP with various strings, in rotation.
The second argument TO-STRINGS contains the replacement strings, separated
by spaces.  This command works like `query-replace-regexp' except that
each successive replacement uses the next successive replacement string,
wrapping around from the last such string to the first.

In Transient Mark mode, if the mark is active, operate on the contents
of the region.  Otherwise, operate from point to the end of the buffer's
accessible portion.

Non-interactively, TO-STRINGS may be a list of replacement strings.

Interactively, reads the regexp using `read-regexp'.
Use \\<minibuffer-local-map>\\[next-history-element] \
to pull the last incremental search regexp to the minibuffer
that reads REGEXP.

As each match is found, the user must type a character saying
what to do with it.  Type SPC or `y' to replace the match,
DEL or `n' to skip and go to the next match.  For more directions,
type \\[help-command] at that time.

A prefix argument N says to use each replacement string N times
before rotating to the next.
Fourth and fifth arg START and END specify the region to operate on.

Arguments REGEXP, START, END, and REGION-NONCONTIGUOUS-P are passed to
`perform-replace' (which see)."
  (declare (interactive-args
	    (start (use-region-beginning))
	    (end (use-region-end))
	    (region-noncontiguous-p (use-region-noncontiguous-p))))
  (interactive
   (let* ((from (read-regexp "Map query replace (regexp): " nil
			     query-replace-from-history-variable))
	  (to (read-from-minibuffer
	       (format "Query replace %s with (space-separated strings): "
		       (query-replace-descr from))
	       nil nil nil
	       query-replace-to-history-variable from t)))
     (list from to
	   (and current-prefix-arg
		(prefix-numeric-value current-prefix-arg))
	   (use-region-beginning) (use-region-end)
	   (use-region-noncontiguous-p))))
  (let (replacements)
    (if (listp to-strings)
	(setq replacements to-strings)
      (while (/= (length to-strings) 0)
	(if (string-search " " to-strings)
	    (setq replacements
		  (append replacements
			  (list (substring to-strings 0
					   (string-search " " to-strings))))
		  to-strings (substring to-strings
				       (1+ (string-search " " to-strings))))
	  (setq replacements (append replacements (list to-strings))
		to-strings ""))))
    (perform-replace regexp replacements t t nil n nil start end nil region-noncontiguous-p)))

(defun replace-string (from-string to-string &optional delimited start end backward region-noncontiguous-p)
  "Replace occurrences of FROM-STRING with TO-STRING.
Preserve case in each match if `case-replace' and `case-fold-search'
are non-nil and FROM-STRING has no uppercase letters.
\(Preserving case means that if the string matched is all caps, or capitalized,
then its replacement is upcased or capitalized.)

Ignore read-only matches if `query-replace-skip-read-only' is non-nil,
ignore hidden matches if `search-invisible' is nil, and ignore more
matches using `isearch-filter-predicate'.

If `replace-lax-whitespace' is non-nil, a space or spaces in the string
to be replaced will match a sequence of whitespace chars defined by the
regexp in `search-whitespace-regexp'.

If `replace-char-fold' is non-nil, matching uses character folding,
i.e. it ignores diacritics and other differences between equivalent
character strings.

Third arg DELIMITED (prefix arg if interactive), if non-nil, means replace
only matches surrounded by word boundaries.  A negative prefix arg means
replace backward.

Operates on the region between START and END (if both are nil, from point
to the end of the buffer).  Interactively, if Transient Mark mode is
enabled and the mark is active, operates on the contents of the region;
otherwise from point to the end of the buffer's accessible portion.

Arguments BACKWARD and REGION-NONCONTIGUOUS-P are passed
to `perform-replace' (which see).

Use \\<minibuffer-local-map>\\[next-history-element] \
to pull the last incremental search string to the minibuffer
that reads FROM-STRING.

This function is usually the wrong thing to use in a Lisp program.
What you probably want is a loop like this:
  (while (search-forward FROM-STRING nil t)
    (replace-match TO-STRING nil t))
which will run faster and will not set the mark or print anything.
\(You may need a more complex loop if FROM-STRING can match the null string
and TO-STRING is also null.)"
  (declare (interactive-only
	    "use `search-forward' and `replace-match' instead.")
	   (interactive-args
	    (start (use-region-beginning))
	    (end (use-region-end))
	    (region-noncontiguous-p (use-region-noncontiguous-p))))
  (interactive
   (let ((common
	  (query-replace-read-args
	   (concat "Replace"
		   (if current-prefix-arg
		       (if (eq current-prefix-arg '-) " backward" " word")
		     "")
		   " string"
		   (if (use-region-p) " in region" ""))
	   nil)))
     (list (nth 0 common) (nth 1 common) (nth 2 common)
	   (use-region-beginning) (use-region-end)
	   (nth 3 common)
	   (use-region-noncontiguous-p))))
  (perform-replace from-string to-string nil nil delimited nil nil start end backward region-noncontiguous-p))

(defun replace-regexp (regexp to-string &optional delimited start end backward region-noncontiguous-p)
  "Replace things after point matching REGEXP with TO-STRING.
Preserve case in each match if `case-replace' and `case-fold-search'
are non-nil and REGEXP has no uppercase letters.

Ignore read-only matches if `query-replace-skip-read-only' is non-nil,
ignore hidden matches if `search-invisible' is nil, and ignore more
matches using `isearch-filter-predicate'.

If `replace-regexp-lax-whitespace' is non-nil, a space or spaces in the regexp
to be replaced will match a sequence of whitespace chars defined by the
regexp in `search-whitespace-regexp'.

This function is not affected by `replace-char-fold'

In Transient Mark mode, if the mark is active, operate on the contents
of the region.  Otherwise, operate from point to the end of the buffer's
accessible portion.

Third arg DELIMITED (prefix arg if interactive), if non-nil, means replace
only matches surrounded by word boundaries.  A negative prefix arg means
replace backward.

Fourth and fifth arg START and END specify the region to operate on.

Arguments BACKWARD and REGION-NONCONTIGUOUS-P are passed
to `perform-replace' (which see).

In TO-STRING, `\\&' stands for whatever matched the whole of REGEXP,
and `\\=\\N' (where N is a digit) stands for whatever matched
the Nth `\\(...\\)' (1-based) in REGEXP.
`\\?' lets you edit the replacement text in the minibuffer
at the given position for each replacement.

In interactive calls, the replacement text may contain `\\,'
followed by a Lisp expression used as part of the replacement
text.  Inside of that expression, `\\&' is a string denoting the
whole match, `\\N' a partial match, `\\#&' and `\\#N' the respective
numeric values from `string-to-number', and `\\#' itself for
`replace-count', the number of replacements occurred so far, starting
from zero.

If your Lisp expression is an identifier and the next letter in
the replacement string would be interpreted as part of it, you
can wrap it with an expression like `\\,(or \\#)'.  Incidentally,
for this particular case you may also enter `\\#' in the
replacement text directly.

When using those Lisp features interactively in the replacement
text, TO-STRING is actually made a list instead of a string.
Use \\[repeat-complex-command] after this command for details.

Use \\<minibuffer-local-map>\\[next-history-element] \
to pull the last incremental search regexp to the minibuffer
that reads REGEXP.

This function is usually the wrong thing to use in a Lisp program.
What you probably want is a loop like this:
  (while (re-search-forward REGEXP nil t)
    (replace-match TO-STRING nil nil))
which will run faster and will not set the mark or print anything."
  (declare (interactive-only
	    "use `re-search-forward' and `replace-match' instead.")
	   (interactive-args
	    (start (use-region-beginning))
	    (end (use-region-end))
	    (region-noncontiguous-p (use-region-noncontiguous-p))))
  (interactive
   (let ((common
	  (query-replace-read-args
	   (concat "Replace"
		   (if current-prefix-arg
		       (if (eq current-prefix-arg '-) " backward" " word")
		     "")
		   " regexp"
		   (if (use-region-p) " in region" ""))
	   t)))
     (list (nth 0 common) (nth 1 common) (nth 2 common)
	   (use-region-beginning) (use-region-end)
	   (nth 3 common)
	   (use-region-noncontiguous-p))))
  (perform-replace regexp to-string nil t delimited nil nil start end backward region-noncontiguous-p))

(defvar regexp-history nil
  "History list for some commands that read regular expressions.

Maximum length of the history list is determined by the value
of `history-length', which see.")

(defalias 'delete-non-matching-lines 'keep-lines)

(defalias 'delete-matching-lines 'flush-lines)

(defalias 'count-matches 'how-many)

(defun keep-lines (regexp &optional rstart rend interactive)
  "Delete all lines except those containing matches for REGEXP.
A match split across lines preserves all the lines it lies in.
When called from Lisp (and usually interactively as well, see below)
applies to all lines starting after point.

If REGEXP contains upper case characters (excluding those preceded by `\\')
and `search-upper-case' is non-nil, the matching is case-sensitive.

Second and third arg RSTART and REND specify the region to operate on.
This command operates on (the accessible part of) all lines whose
accessible part is entirely contained in the region determined by RSTART
and REND.  (A newline ending a line counts as part of that line.)  If RSTART
is non-nil, REND also has to be given.

Interactively, in Transient Mark mode when the mark is active, operate
on all lines whose accessible part is entirely contained in the region.
Otherwise, the command applies to all lines starting after point.
When calling this function from Lisp, you can pretend that it was
called interactively by passing a non-nil INTERACTIVE argument.

This function starts looking for the next match from the end of
the previous match.  Hence, it ignores matches that overlap
a previously found match."
  (interactive
   (progn
     (barf-if-buffer-read-only)
     (keep-lines-read-args "Keep lines containing match for regexp")))
  (if rstart
      (progn
	(goto-char (min rstart rend))
	(setq rend
	      (progn
		(save-excursion
		  (goto-char (max rstart rend))
		  (unless (or (bolp) (eobp))
		    (forward-line 0))
		  (point-marker)))))
    (if (and interactive (use-region-p))
	(setq rstart (region-beginning)
	      rend (progn
		     (goto-char (region-end))
		     (unless (or (bolp) (eobp))
		       (forward-line 0))
		     (point-marker)))
      (setq rstart (point)
	    rend (point-max-marker)))
    (goto-char rstart))
  (save-excursion
    (or (bolp) (forward-line 1))
    (let ((start (point))
	  (case-fold-search
	   (if (and case-fold-search search-upper-case)
	       (isearch-no-upper-case-p regexp t)
	     case-fold-search)))
      (while (< (point) rend)
	;; Start is first char not preserved by previous match.
	(if (not (re-search-forward regexp rend 'move))
	    (delete-region start rend)
	  (let ((end (save-excursion (goto-char (match-beginning 0))
				     (forward-line 0)
				     (point))))
	    ;; Now end is first char preserved by the new match.
	    (if (< start end)
		(delete-region start end))))

	(setq start (save-excursion (forward-line 1) (point)))
	;; If the match was empty, avoid matching again at same place.
	(and (< (point) rend)
	     (= (match-beginning 0) (match-end 0))
	     (forward-char 1)))))
  (set-marker rend nil)
  nil)

(defun flush-lines (regexp &optional rstart rend interactive)
  "Delete lines containing matches for REGEXP.
When called from Lisp (and usually when called interactively as
well, see below), applies to the part of the buffer after point.
The line point is in is deleted if and only if it contains a
match for regexp starting after point.

If REGEXP contains upper case characters (excluding those preceded by `\\')
and `search-upper-case' is non-nil, the matching is case-sensitive.

Second and third arg RSTART and REND specify the region to operate on.
Lines partially contained in this region are deleted if and only if
they contain a match entirely contained in it.

Interactively, in Transient Mark mode when the mark is active, operate
on the contents of the region.  Otherwise, operate from point to the
end of (the accessible portion of) the buffer.  When calling this function
from Lisp, you can pretend that it was called interactively by passing
a non-nil INTERACTIVE argument.

If a match is split across lines, all the lines it lies in are deleted.
They are deleted _before_ looking for the next match.  Hence, a match
starting on the same line at which another match ended is ignored.

Return the number of deleted matching lines.  When called interactively,
also print the number.

If you want to not just delete the lines, but also add them to
the kill ring, use the \\[kill-matching-lines] command instead."
  (interactive
   (progn
     (barf-if-buffer-read-only)
     (keep-lines-read-args "Flush lines containing match for regexp")))
  (if rstart
      (progn
	(goto-char (min rstart rend))
	(setq rend (copy-marker (max rstart rend))))
    (if (and interactive (use-region-p))
	(setq rstart (region-beginning)
	      rend (copy-marker (region-end)))
      (setq rstart (point)
	    rend (point-max-marker)))
    (goto-char rstart))
  (let ((count 0)
        (case-fold-search
	 (if (and case-fold-search search-upper-case)
	     (isearch-no-upper-case-p regexp t)
	   case-fold-search)))
    (save-excursion
      (while (and (< (point) rend)
		  (re-search-forward regexp rend t))
	(delete-region (save-excursion (goto-char (match-beginning 0))
				       (forward-line 0)
				       (point))
		       (progn (forward-line 1) (point)))
        (setq count (1+ count))))
    (set-marker rend nil)
    (when interactive (message (ngettext "Deleted %d matching line"
					 "Deleted %d matching lines"
					 count)
			       count))
    count))

(defun kill-matching-lines (regexp &optional rstart rend interactive)
  "Kill lines containing matches for REGEXP.

When called from Lisp (and usually when called interactively as
well, see below), applies to the part of the buffer after point.
The line point is in is killed if and only if it contains a match
for REGEXP starting after point.

If REGEXP contains upper case characters (excluding those
preceded by `\\') and `search-upper-case' is non-nil, the
matching is case-sensitive.

Second and third args RSTART and REND specify the region to
operate on.  Lines partially contained in this region are killed
if and only if they contain a match entirely contained in the
region.

Interactively, in Transient Mark mode when the mark is active,
operate on the contents of the region.  Otherwise, operate from
point to the end of (the accessible portion of) the buffer.

If a match is split across lines, all the lines it lies in are
killed.  They are killed _before_ looking for the next match.
Hence, a match starting on the same line at which another match
ended is ignored.

Return the number of killed matching lines.  When called
interactively, also print the number.

If you merely want to delete the lines, without adding them to
the kill ring, the \\[delete-matching-lines] command is faster."
  (interactive
   (progn
     (barf-if-buffer-read-only)
     (keep-lines-read-args "Kill lines containing match for regexp")))
  (if rstart
      (progn
	(goto-char (min rstart rend))
	(setq rend (copy-marker (max rstart rend))))
    (if (and interactive (use-region-p))
	(setq rstart (region-beginning)
	      rend (copy-marker (region-end)))
      (setq rstart (point)
	    rend (point-max-marker)))
    (goto-char rstart))
  (let ((count 0)
        (case-fold-search
	 (if (and case-fold-search search-upper-case)
	     (isearch-no-upper-case-p regexp t)
	   case-fold-search)))
    (save-excursion
      (while (and (< (point) rend)
		  (re-search-forward regexp rend t))
        (unless (zerop count)
          (setq last-command 'kill-region))
	(kill-region (save-excursion (goto-char (match-beginning 0))
                                     (forward-line 0)
                                     (point))
                     (progn (forward-line 1) (point)))
        (setq count (1+ count))))
    (set-marker rend nil)
    (when interactive (message (ngettext "Killed %d matching line"
					 "Killed %d matching lines"
					 count)
			       count))
    count))

(defun copy-matching-lines (regexp &optional rstart rend interactive)
  "Copy lines containing matches for REGEXP to the kill ring.

When called from Lisp (and usually when called interactively as
well, see below), applies to the part of the buffer after point.
The line point is in is copied if and only if it contains a match
for REGEXP starting after point.

If REGEXP contains upper case characters (excluding those
preceded by `\\') and `search-upper-case' is non-nil, the
matching is case-sensitive.

Second and third args RSTART and REND specify the region to
operate on.  Lines partially contained in this region are copied
if and only if they contain a match entirely contained in the
region.

Interactively, in Transient Mark mode when the mark is active,
operate on the contents of the region.  Otherwise, operate from
point to the end of (the accessible portion of) the buffer.

If a match is split across lines, all the lines it lies in are
copied.

Return the number of copied matching lines.  When called
interactively, also print the number."
  (interactive
   (keep-lines-read-args "Copy lines containing match for regexp"))
  (if rstart
      (progn
	(goto-char (min rstart rend))
	(setq rend (copy-marker (max rstart rend))))
    (if (and interactive (use-region-p))
	(setq rstart (region-beginning)
	      rend (copy-marker (region-end)))
      (setq rstart (point)
	    rend (point-max-marker)))
    (goto-char rstart))
  (let ((count 0)
        (case-fold-search
	 (if (and case-fold-search search-upper-case)
	     (isearch-no-upper-case-p regexp t)
	   case-fold-search)))
    (save-excursion
      (while (and (< (point) rend)
		  (re-search-forward regexp rend t))
	(unless (zerop count)
          (setq last-command 'kill-region))
	(copy-region-as-kill (save-excursion (goto-char (match-beginning 0))
                                             (forward-line 0)
                                             (point))
                             (progn (forward-line 1) (point)))
        (setq count (1+ count))))
    (set-marker rend nil)
    (when interactive (message (ngettext "Copied %d matching line"
					 "Copied %d matching lines"
					 count)
			       count))
    count))

(defun how-many (regexp &optional rstart rend interactive)
  "Print and return number of matches for REGEXP following point.
When called from Lisp and INTERACTIVE is omitted or nil, just return
the number, do not print it; if INTERACTIVE is t, the function behaves
in all respects as if it had been called interactively.

If REGEXP contains upper case characters (excluding those preceded by `\\')
and `search-upper-case' is non-nil, the matching is case-sensitive.

Second and third arg RSTART and REND specify the region to operate on.

Interactively, in Transient Mark mode when the mark is active, operate
on the contents of the region.  Otherwise, operate from point to the
end of (the accessible portion of) the buffer.

This function starts looking for the next match from the end of
the previous match.  Hence, it ignores matches that overlap
a previously found match."
  (interactive
   (keep-lines-read-args "How many matches for regexp"))
  (save-excursion
    (if rstart
        (if rend
            (progn
              (goto-char (min rstart rend))
              (setq rend (max rstart rend)))
          (goto-char rstart)
          (setq rend (point-max)))
      (if (and interactive (use-region-p))
	  (setq rstart (region-beginning)
		rend (region-end))
	(setq rstart (point)
	      rend (point-max)))
      (goto-char rstart))
    (let ((count 0)
	  (case-fold-search
	   (if (and case-fold-search search-upper-case)
	       (isearch-no-upper-case-p regexp t)
	     case-fold-search)))
      (while (and (< (point) rend)
		  (re-search-forward regexp rend t))
        ;; Ensure forward progress on zero-length matches like "^$".
        (when (and (= (match-beginning 0) (match-end 0))
                   (not (eobp)))
          (forward-char 1))
	(setq count (1+ count)))
      (when interactive (message (ngettext "%d occurrence"
					   "%d occurrences"
					   count)
				 count))
      count)))

(defconst query-replace-help
  "Type \\`SPC' or \\`y' to replace one match, Delete or \\`n' to skip to next,
\\`RET' or \\`q' to exit, Period to replace one match and exit,
\\`,' to replace but not move point immediately,
\\`!' to replace all remaining matches in this buffer with no more questions,
\\`C-r' to enter recursive edit (\\[exit-recursive-edit] to get out again),
\\`C-w' to delete match and then enter recursive edit,
\\`^' to move point back to previous match,
\\`u' to undo previous replacement,
\\`U' to undo all replacements,
\\`e' to edit the replacement string.
\\`E' to edit the replacement string with exact case.
\\`d' to display the diff buffer with all replacements.
\\`C-l' to clear the screen, redisplay, and offer same replacement again,
\\`Y' to replace all remaining matches in all remaining buffers (in
multi-buffer replacements) with no more questions,
\\`N' (in multi-buffer replacements) to skip to the next buffer without
replacing remaining matches in the current buffer.
Any other character exits the interactive replacement loop, and is then
re-executed as a normal key sequence."
  "Help message while in `query-replace'.")

(defvar query-replace-map
  (let ((map (make-sparse-keymap)))
    (define-key map " " 'act)
    (define-key map "\d" 'skip)
    (define-key map [delete] 'skip)
    (define-key map [backspace] 'skip)
    (define-key map "y" 'act)
    (define-key map "n" 'skip)
    (define-key map "Y" 'act)
    (define-key map "N" 'skip)
    (define-key map "e" 'edit-replacement)
    (define-key map "E" 'edit-replacement-exact-case)
    (define-key map "," 'act-and-show)
    (define-key map "q" 'exit)
    (define-key map "\r" 'exit)
    (define-key map [return] 'exit)
    (define-key map "." 'act-and-exit)
    (define-key map "\C-r" 'edit)
    (define-key map "\C-w" 'delete-and-edit)
    (define-key map "\C-l" 'recenter)
    (define-key map "!" 'automatic)
    (define-key map "^" 'backup)
    (define-key map "u" 'undo)
    (define-key map "U" 'undo-all)
    (define-key map "d" 'diff)
    (define-key map "\C-h" 'help)
    (define-key map [f1] 'help)
    (define-key map [help] 'help)
    (define-key map "?" 'help)
    (define-key map "\C-g" 'quit)
    (define-key map "\C-]" 'quit)
    (define-key map "\C-v" 'scroll-up)
    (define-key map "\M-v" 'scroll-down)
    (define-key map [next] 'scroll-up)
    (define-key map [prior] 'scroll-down)
    (define-key map [?\C-\M-v] 'scroll-other-window)
    (define-key map [M-next] 'scroll-other-window)
    (define-key map [?\C-\M-\S-v] 'scroll-other-window-down)
    (define-key map [M-prior] 'scroll-other-window-down)
    ;; Binding ESC would prohibit the M-v binding.  Instead, callers
    ;; should check for ESC specially.
    ;; (define-key map "\e" 'exit-prefix)
    (define-key map [escape] 'exit-prefix)
    map)
  "Keymap of responses to questions posed by commands like `query-replace'.
The \"bindings\" in this map are not commands; they are answers.
The valid answers include `act', `skip', `act-and-show',
`act-and-exit', `exit', `exit-prefix', `recenter', `scroll-up',
`scroll-down', `scroll-other-window', `scroll-other-window-down',
`edit', `edit-replacement', `edit-replacement-exact-case',
`delete-and-edit', `automatic', `backup', `undo', `undo-all',
`diff', `quit', and `help'.

This keymap is used by `y-or-n-p' as well as `query-replace'.")

(defun replace-match-string-symbols (n)
  "Process a list (and any sub-lists), expanding certain symbols.
Symbol  Expands To
N     (match-string N)           (where N is a string of digits)
#N    (string-to-number (match-string N))
&     (match-string 0)
#&    (string-to-number (match-string 0))
#     replace-count

Note that these symbols must be preceded by a backslash in order to
type them using Lisp syntax."
  (while (consp n)
    (cond
     ((consp (car n))
      (replace-match-string-symbols (car n))) ;Process sub-list
     ((symbolp (car n))
      (let ((name (symbol-name (car n))))
        (cond
         ((string-match "^[0-9]+$" name)
          (setcar n (list 'match-string (string-to-number name))))
         ((string-match "^#[0-9]+$" name)
          (setcar n (list 'string-to-number
                          (list 'match-string
                                (string-to-number (substring name 1))))))
         ((string= "&" name)
          (setcar n '(match-string 0)))
         ((string= "#&" name)
          (setcar n '(string-to-number (match-string 0))))
	 ((string= "#" name)
	  (setcar n 'replace-count))))))
    (setq n (cdr n))))

(defun replace-eval-replacement (expression count)
  (let* ((replace-count count)
         (replacement
          (condition-case err
              (eval expression)
            (error
             (error "Error evaluating replacement expression: %S" err)))))
    (if (stringp replacement)
        replacement
      (prin1-to-string replacement t))))

(defun replace-quote (replacement)
  "Quote a replacement string.
This just doubles all backslashes in REPLACEMENT and
returns the resulting string.  If REPLACEMENT is not
a string, it is first passed through `prin1-to-string'
with the `noescape' argument set.

`match-data' is preserved across the call."
  (string-replace "\\" "\\\\"
		  (if (stringp replacement)
		      replacement
		    (prin1-to-string replacement t))))

(defun replace-loop-through-replacements (data count)
  ;; DATA is a vector containing the following values:
  ;;   0 next-rotate-count
  ;;   1 repeat-count
  ;;   2 next-replacement
  ;;   3 replacements
  (if (= (aref data 0) count)
      (progn
        (aset data 0 (+ count (aref data 1)))
        (let ((next (cdr (aref data 2))))
          (aset data 2 (if (consp next) next (aref data 3))))))
  (car (aref data 2)))

(defun replace-match-data (integers reuse &optional new)
  "Like `match-data', but markers in REUSE get invalidated.
If NEW is non-nil, it is set and returned instead of fresh data,
but coerced to the correct value of INTEGERS."
  (or (and new
	   (progn
	     (set-match-data new)
	     (and (eq new reuse)
		  (eq (null integers) (markerp (car reuse)))
		  new)))
      (match-data integers reuse t)))

(defun replace-match-maybe-edit (newtext fixedcase literal noedit match-data
                                 &optional backward)
  "Make a replacement with `replace-match', editing `\\?'.
FIXEDCASE, LITERAL are passed to `replace-match' (which see).
After possibly editing it (if `\\?' is present), NEWTEXT is also
passed to `replace-match'.  If NOEDIT is true, no check for `\\?'
is made (to save time).
MATCH-DATA is used for the replacement, and is a data structure
as returned from the `match-data' function.
In case editing is done, it is changed to use markers.  BACKWARD is
used to reverse the replacement direction.

The return value is non-nil if there has been no `\\?' or NOEDIT was
passed in.  If LITERAL is set, no checking is done, anyway."
  (unless (or literal noedit)
    (setq noedit t)
    (while (string-match "\\(\\`\\|[^\\]\\)\\(\\\\\\\\\\)*\\(\\\\\\?\\)"
			 newtext)
      (setq newtext
	    (read-string "Edit replacement string: "
                         (prog1
                             (cons
                              (replace-match "" t t newtext 3)
                              (1+ (match-beginning 3)))
                           (setq match-data
                                 (replace-match-data
                                  nil match-data match-data))))
	    noedit nil)))
  (set-match-data match-data)
  (replace-match newtext fixedcase literal)
  ;; `replace-match' leaves point at the end of the replacement text,
  ;; so move point to the beginning when replacing backward.
  (when backward (goto-char (nth 0 match-data)))
  noedit)

(defvar replace-update-post-hook nil
  "Function(s) to call after `query-replace' has found a match in the buffer.")

(defvar replace-search-function nil
  "Function to use when searching for strings to replace.
It is used by `query-replace' and `replace-string', and is called
with three arguments, as if it were `search-forward'.")

(defvar replace-re-search-function nil
  "Function to use when searching for regexps to replace.
It is used by `query-replace-regexp', `replace-regexp', and
`map-query-replace-regexp'.  It is called with three arguments,
as if it were `re-search-forward'.")

(defvar replace-regexp-function nil
  "Function to convert the FROM string of query-replace commands to a regexp.
This is used by `query-replace', `query-replace-regexp', etc. as
the value of `isearch-regexp-function' when they search for the
occurrences of the string/regexp to be replaced.  This is intended
to be used when the string to be replaced, as typed by the user,
is not to be interpreted literally, but instead should be converted
to a regexp that is actually used for the search.")

(defun replace-search (search-string limit regexp-flag delimited-flag
		       case-fold &optional backward)
  "Search for the next occurrence of SEARCH-STRING to replace."
  ;; Let-bind global isearch-* variables to values used
  ;; to search the next replacement.  These let-bindings
  ;; should be effective both at the time of calling
  ;; `isearch-search-fun-default' and also at the
  ;; time of funcalling `search-function'.
  ;; These isearch-* bindings can't be placed higher
  ;; outside of this function because then another I-search
  ;; used after `recursive-edit' might override them.
  (let* ((isearch-regexp regexp-flag)
	 (isearch-regexp-function (or replace-regexp-function
				      delimited-flag
				      (and replace-char-fold
					   (not regexp-flag)
					   #'char-fold-to-regexp)))
	 (isearch-lax-whitespace
	  replace-lax-whitespace)
	 (isearch-regexp-lax-whitespace
	  replace-regexp-lax-whitespace)
	 (isearch-case-fold-search case-fold)
	 (isearch-adjusted nil)
	 (isearch-nonincremental t)	; don't use lax word mode
	 (isearch-forward (not backward))
	 (search-function
	  (or (if regexp-flag
		  replace-re-search-function
		replace-search-function)
              ;; `isearch-search-fun' can't be used here because
              ;; when buffer-local `isearch-search-fun-function'
              ;; searches e.g. the minibuffer history, then
              ;; `query-replace' should not operate on the whole
              ;; history, but only on the minibuffer contents.
	      (isearch-search-fun-default))))
    (funcall search-function search-string limit t)))

(defvar replace-overlay nil)

(defvar replace-submatches-overlays nil)

(defun replace-highlight (match-beg match-end range-beg range-end
			  search-string regexp-flag delimited-flag
			  case-fold &optional backward)
  (if query-replace-highlight
      (if replace-overlay
	  (move-overlay replace-overlay match-beg match-end (current-buffer))
	(setq replace-overlay (make-overlay match-beg match-end))
	(overlay-put replace-overlay 'priority 1001) ;higher than lazy overlays
	(overlay-put replace-overlay 'face 'query-replace)))

  (when (and query-replace-highlight-submatches regexp-flag)
    (mapc 'delete-overlay replace-submatches-overlays)
    (setq replace-submatches-overlays nil)
    ;; 'cddr' removes whole expression match from match-data
    (let ((submatch-data (cddr (match-data t)))
          (group 0)
          b e ov face)
      (while submatch-data
        (setq b (pop submatch-data)
              e (pop submatch-data))
        (when (and (integer-or-marker-p b)
                   (integer-or-marker-p e))
          (setq ov (make-overlay b e)
                group (1+ group)
                face (intern-soft (format "isearch-group-%d" group)))
          ;; Recycle faces from beginning
          (unless (facep face)
            (setq group 1 face 'isearch-group-1))
          (overlay-put ov 'face face)
          (overlay-put ov 'priority 1002)
          (push ov replace-submatches-overlays)))))

  (if query-replace-lazy-highlight
      (let ((isearch-string search-string)
	    (isearch-regexp regexp-flag)
	    (isearch-regexp-function (or replace-regexp-function
					 delimited-flag
					 (and replace-char-fold
					      (not regexp-flag)
					      #'char-fold-to-regexp)))
	    (isearch-lax-whitespace
	     replace-lax-whitespace)
	    (isearch-regexp-lax-whitespace
	     replace-regexp-lax-whitespace)
	    (isearch-case-fold-search case-fold)
	    (isearch-invisible search-invisible)
	    (isearch-forward (not backward))
	    (isearch-other-end match-beg)
	    (isearch-error nil)
	    (isearch-lazy-count nil)
	    (lazy-highlight-buffer nil))
	(isearch-lazy-highlight-new-loop range-beg range-end))))

(defun replace-dehighlight ()
  (when replace-overlay
    (delete-overlay replace-overlay))
  (when query-replace-highlight-submatches
    (mapc 'delete-overlay replace-submatches-overlays)
    (setq replace-submatches-overlays nil))
  (when query-replace-lazy-highlight
    (lazy-highlight-cleanup lazy-highlight-cleanup)
    (setq isearch-lazy-highlight-last-string nil))
  ;; Close overlays opened by `isearch-range-invisible' in `perform-replace'.
  (isearch-clean-overlays))

(defmacro replace--push-stack (replaced search-str next-replace next-replacement match-again stack)
  (declare (indent 0) (debug (form form form form form gv-place)))
  `(push (list (point) ,replaced
               ;; If the replacement has already happened, all we need is the
               ;; current match start and end.  We could get this with a trivial
               ;; match like
               ;; (save-excursion (goto-char (match-beginning 0))
               ;;        	     (search-forward (match-string 0))
               ;;                 (match-data t))
               ;; if we really wanted to avoid manually constructing match data.
               ;; Adding current-buffer is necessary so that match-data calls
               ;; can return markers which are appropriate for editing.
	       (if ,replaced
		   (list
		    (match-beginning 0) (match-end 0) (current-buffer))
	         (match-data))
	       ,search-str ,next-replace ,next-replacement ,match-again)
         ,stack))

(defun replace--region-filter (bounds)
  "Return a function that decides if a region is inside BOUNDS.
BOUNDS is a list of cons cells of the form (START . END).  The
returned function takes as argument two buffer positions, START
and END."
  (let ((region-bounds
         (mapcar (lambda (position)
                   (cons (copy-marker (car position))
                         (copy-marker (cdr position))))
                 bounds)))
    (lambda (start end)
      (delq nil (mapcar
                 (lambda (bounds)
                   (and
                    (>= start (car bounds))
                    (<= start (cdr bounds))
                    (>= end   (car bounds))
                    (<= end   (cdr bounds))))
                 region-bounds)))))

(defun perform-replace (from-string replacements
		        query-flag regexp-flag delimited-flag
			&optional repeat-count map start end backward region-noncontiguous-p)
  "Subroutine of `query-replace'.  Its complexity handles interactive queries.
Don't use this in your own program unless you want to query and set the mark
just as `query-replace' does.  Instead, write a simple loop like this:

  (while (re-search-forward \"foo[ \\t]+bar\" nil t)
    (replace-match \"foobar\" nil nil))

which will run faster and probably do exactly what you want.  Please
see the documentation of `replace-match' to find out how to simulate
`case-replace'.

This function returns nil if there were no matches to make, or
the user canceled the call.

REPLACEMENTS is either a string, a list of strings, or a cons cell
containing a function and its first argument.  The function is
called to generate each replacement like this:
  (funcall (car replacements) (cdr replacements) replace-count)
It must return a string.

Non-nil REGION-NONCONTIGUOUS-P means that the region is composed of
noncontiguous pieces.  The most common example of this is a
rectangular region, where the pieces are separated by newline
characters."
  (or map (setq map query-replace-map))
  (and query-flag minibuffer-auto-raise
       (raise-frame (window-frame (minibuffer-window))))
  (let* ((case-fold-search
	  (if (and case-fold-search search-upper-case)
	      (isearch-no-upper-case-p from-string regexp-flag)
	    case-fold-search))
         (nocasify (not (and case-replace case-fold-search)))
         (literal (or (not regexp-flag) (eq regexp-flag 'literal)))
         (search-string from-string)
         (real-match-data nil)       ; The match data for the current match.
         (next-replacement nil)
         ;; This is non-nil if we know there is nothing for the user
         ;; to edit in the replacement.
         (noedit nil)
         (keep-going t)
         (stack nil)
         (search-string-replaced nil)    ; last string matching `from-string'
         (next-replacement-replaced nil) ; replacement string
                                         ; (substituted regexp)
         (last-was-undo)
         (last-was-act-and-show)
         (update-stack t)
         (replace-count 0)
         (skip-read-only-count 0)
         (skip-filtered-count 0)
         (skip-invisible-count 0)
         (nonempty-match nil)
	 (multi-buffer nil)
	 (recenter-last-op nil)	; Start cycling order with initial position.

         ;; If non-nil, it is marker saying where in the buffer to stop.
         (limit nil)
         (region-filter nil)

         ;; Disable text conversion during the replacement operation.
         (old-text-conversion-style (and (boundp 'overriding-text-conversion-style)
                                         overriding-text-conversion-style))
         overriding-text-conversion-style

         ;; Data for the next match.  If a cons, it has the same format as
         ;; (match-data); otherwise it is t if a match is possible at point.
         (match-again t)
         (message
          (if query-flag
              (apply #'propertize
                     (concat "Query replacing "
                             (if backward "backward " "")
                             (if delimited-flag
                                 (or (and (symbolp delimited-flag)
                                          (get delimited-flag
                                               'isearch-message-prefix))
                                     "word ") "")
                             (if regexp-flag "regexp " "")
                             "%s with %s: "
                             (substitute-command-keys
                              "(\\<query-replace-map>\\[help] for help) "))
                     minibuffer-prompt-properties))))

    ;; Unless a single contiguous chunk is selected, operate on multiple chunks.
    (when region-noncontiguous-p
      (setq region-filter (replace--region-filter
                           (funcall region-extract-function 'bounds)))
      (add-function :after-while isearch-filter-predicate region-filter))

    ;; If region is active, in Transient Mark mode, operate on region.
    (if backward
	(when end
	  (setq limit (copy-marker (min start end)))
	  (goto-char (max start end))
	  (deactivate-mark))
      (when start
	(setq limit (copy-marker (max start end)))
	(goto-char (min start end))
	(deactivate-mark)))

    ;; If last typed key in previous call of multi-buffer perform-replace
    ;; was `automatic-all', don't ask more questions in next files
    (when (eq (lookup-key map (vector last-input-event) t) 'automatic-all)
      (setq query-flag nil multi-buffer t))

    (cond
     ((stringp replacements)
      (setq next-replacement replacements
            replacements     nil))
     ((stringp (car replacements)) ; If it isn't a string, it must be a cons
      (or repeat-count (setq repeat-count 1))
      ;; This is a hand-made `iterator'.
      (setq replacements (cons #'replace-loop-through-replacements
                               (vector repeat-count repeat-count
                                       replacements replacements)))))

    (when query-replace-lazy-highlight
      (setq isearch-lazy-highlight-last-string nil))

    (push-mark)
    (undo-boundary)
    (when (and query-flag (fboundp 'set-text-conversion-style))
      (setq overriding-text-conversion-style nil)
      (set-text-conversion-style text-conversion-style))
    (unwind-protect
	;; Loop finding occurrences that perhaps should be replaced.
	(while (and keep-going
		    (if backward
			(not (or (bobp) (and limit (<= (point) limit))))
		      (not (or (eobp) (and limit (>= (point) limit)))))
		    ;; Use the next match if it is already known;
		    ;; otherwise, search for a match after moving forward
		    ;; one char if progress is required.
		    (setq real-match-data
			  (cond ((consp match-again)
				 (goto-char (if backward
						(nth 0 match-again)
					      (nth 1 match-again)))
				 (replace-match-data
				  nil real-match-data match-again))
				;; MATCH-AGAIN non-nil means accept an
				;; adjacent match.
				(match-again
				 (and
				  (replace-search search-string limit
						  regexp-flag delimited-flag
						  case-fold-search backward)
				  ;; For speed, use only integers and
				  ;; reuse the list used last time.
				  (replace-match-data nil real-match-data)))
				((and (if backward
					  (> (1- (point)) (point-min))
					(< (1+ (point)) (point-max)))
				      (or (null limit)
					  (if backward
					      (> (1- (point)) limit)
					    (< (1+ (point)) limit))))
				 ;; If not accepting adjacent matches,
				 ;; move one char to the right before
				 ;; searching again.  Undo the motion
				 ;; if the search fails.
				 (let ((opoint (point)))
				   (forward-char (if backward -1 1))
				   (if (replace-search search-string limit
						       regexp-flag delimited-flag
						       case-fold-search backward)
				       (replace-match-data
					nil real-match-data)
				     (goto-char opoint)
				     nil))))))

	  ;; Record whether the match is nonempty, to avoid an infinite loop
	  ;; repeatedly matching the same empty string.
	  (setq nonempty-match
		(/= (nth 0 real-match-data) (nth 1 real-match-data)))

	  ;; If the match is empty, record that the next one can't be
	  ;; adjacent.

	  ;; Otherwise, if matching a regular expression, do the next
	  ;; match now, since the replacement for this match may
	  ;; affect whether the next match is adjacent to this one.
	  ;; If that match is empty, don't use it.
	  (setq match-again
		(and nonempty-match
		     (or (not regexp-flag)
			 (and (if backward
				  (looking-back search-string nil)
				(looking-at search-string))
			      (let ((match (match-data)))
				(and (/= (nth 0 match) (nth 1 match))
				     match))))))

	  (cond
	   ;; Optionally ignore matches that have a read-only property.
	   ((not (or (not query-replace-skip-read-only)
		     (not (text-property-not-all
			   (nth 0 real-match-data) (nth 1 real-match-data)
			   'read-only nil))))
	    (setq skip-read-only-count (1+ skip-read-only-count)))
	   ;; Optionally filter out matches.
	   ((not (funcall isearch-filter-predicate
                          (nth 0 real-match-data) (nth 1 real-match-data)))
	    (setq skip-filtered-count (1+ skip-filtered-count)))
	   ;; Optionally ignore invisible matches.
	   ((not (or (eq search-invisible t)
		     ;; Don't open overlays for automatic replacements.
		     (and (not query-flag) search-invisible)
		     ;; Open hidden overlays for interactive replacements.
		     (not (isearch-range-invisible
			   (nth 0 real-match-data) (nth 1 real-match-data)))))
	    (setq skip-invisible-count (1+ skip-invisible-count)))
	   (t
	    ;; Calculate the replacement string, if necessary.
	    (when replacements
	      (set-match-data real-match-data)
	      (setq next-replacement
		    (funcall (car replacements) (cdr replacements)
			     replace-count)))
	    (if (not query-flag)
		(progn
		  (unless (or literal noedit)
		    (replace-highlight
		     (nth 0 real-match-data) (nth 1 real-match-data)
		     start end search-string
		     regexp-flag delimited-flag case-fold-search backward))
		  (setq noedit
			(replace-match-maybe-edit
			 next-replacement nocasify literal
			 noedit real-match-data backward)
			replace-count (1+ replace-count)))
	      (undo-boundary)
	      (let (done replaced key def)
		;; Loop reading commands until one of them sets done,
		;; which means it has finished handling this
		;; occurrence.  Any command that sets `done' should
		;; leave behind proper match data for the stack.
		;; Commands not setting `done' need to adjust
		;; `real-match-data'.
		(while (not done)
		  ;; This sets match-data only for the next hook and
		  ;; replace-highlight that calls `sit-for' from
		  ;; isearch-lazy-highlight-new-loop whose redisplay
		  ;; might clobber match-data. So subsequent code should
		  ;; use only real-match-data, not match-data (bug#36328).
		  (set-match-data real-match-data)
                  (run-hooks 'replace-update-post-hook) ; Before `replace-highlight'.
                  (replace-highlight
		   (match-beginning 0) (match-end 0)
		   start end search-string
		   regexp-flag delimited-flag case-fold-search backward)
                  ;; Obtain the matched groups: needed only when
                  ;; regexp-flag non nil.
                  (when (and last-was-undo regexp-flag)
                    (setq last-was-undo nil
                          real-match-data
                          (save-excursion
                            (goto-char (nth 0 real-match-data))
                            (looking-at search-string)
                            (match-data nil real-match-data))))
                  ;; Matched string and next-replacement-replaced
                  ;; stored in stack.
                  (setq search-string-replaced (buffer-substring-no-properties
                                                (nth 0 real-match-data)
                                                (nth 1 real-match-data))
                        next-replacement-replaced
                        (query-replace-descr
                         (save-match-data
                           (set-match-data real-match-data)
                           (match-substitute-replacement
                            next-replacement nocasify literal))))
		  (let* ((replacement-presentation
			  (if query-replace-show-replacement
			      (save-match-data
			        (set-match-data real-match-data)
			        (match-substitute-replacement next-replacement
							      nocasify literal))
			    next-replacement))
			 (prompt
			  (format message
                                  (query-replace-descr from-string)
                                  (query-replace-descr
                                   replacement-presentation))))
                    ;; Use `read-key' so that escape sequences on TTYs
                    ;; are properly mapped back to the intended key.
		    (setq key (read-key prompt)))
		  ;; Necessary in case something happens during
		  ;; read-event that clobbers the match data.
		  (set-match-data real-match-data)
		  (setq key (vector key))
		  (setq def (lookup-key map key t))
		  ;; Restore the match data while we process the command.
		  (cond ((eq def 'help)
			 (let ((display-buffer-overriding-action
				'(nil (inhibit-same-window . t))))
			   (with-output-to-temp-buffer "*Help*"
			     (princ
			      (concat "Query replacing "
				      (if backward "backward " "")
				      (if delimited-flag
					  (or (and (symbolp delimited-flag)
						   (get delimited-flag
							'isearch-message-prefix))
					      "word ") "")
				      (if regexp-flag "regexp " "")
				      from-string " with "
				      next-replacement ".\n\n"
				      (substitute-command-keys
				       query-replace-help)))
			     (with-current-buffer standard-output
			       (help-mode)))))
			((eq def 'exit)
			 (setq keep-going nil)
			 (setq done t))
			((eq def 'exit-current)
			 (setq multi-buffer t keep-going nil done t))
			((eq def 'backup)
			 (if stack
			     (let ((elt (pop stack)))
			       (goto-char (nth 0 elt))
			       (setq replaced (nth 1 elt)
				     real-match-data
				     (replace-match-data
				      nil real-match-data
				      (nth 2 elt))
                                     next-replacement (nth 5 elt)
                                     match-again (nth 6 elt)))
			   (message "No previous match")
			   (ding 'no-terminate)
			   (sit-for 1)))
			((or (eq def 'undo) (eq def 'undo-all))
			 (if (null stack)
                             (progn
                               (message "Nothing to undo")
                               (ding 'no-terminate)
                               (sit-for 1))
			   (let ((stack-idx         0)
                                 (stack-len         (length stack))
                                 (num-replacements  0)
                                 (nocasify t) ; Undo must preserve case (Bug#31073).
                                 search-string
                                 last-replacement)
                             (while (and (< stack-idx stack-len)
                                         stack
                                         (or (null replaced) last-was-act-and-show))
                               (let* ((elt (nth stack-idx stack)))
                                 (setq
                                  stack-idx (1+ stack-idx)
                                  replaced (nth 1 elt)
                                  ;; Bind swapped values
                                  ;; (search-string <--> replacement)
                                  search-string (nth (if replaced 4 3) elt)
                                  last-replacement (nth (if replaced 3 4) elt)
                                  search-string-replaced search-string
                                  next-replacement-replaced last-replacement
                                  last-was-act-and-show nil)

                                 (when (and (= stack-idx stack-len)
                                            (and (null replaced) (not last-was-act-and-show))
                                            (zerop num-replacements))
                                          (message "Nothing to undo")
                                          (ding 'no-terminate)
                                          (sit-for 1))

                                 (when replaced
                                   (setq stack (nthcdr stack-idx stack))
                                   (goto-char (nth 0 elt))
                                   (set-match-data (nth 2 elt))
                                   (setq real-match-data
                                         (save-excursion
                                           (goto-char (match-beginning 0))
                                           ;; We must quote the string (Bug#37073)
                                           (looking-at (regexp-quote search-string))
                                           (match-data nil (nth 2 elt)))
                                         noedit
                                         (replace-match-maybe-edit
                                          last-replacement nocasify literal
                                          noedit real-match-data backward)
                                         replace-count (1- replace-count)
                                         real-match-data
                                         (save-excursion
                                           (goto-char (match-beginning 0))
                                           (looking-at (if regexp-flag
                                                           last-replacement
                                                         (regexp-quote
                                                          last-replacement)))
                                           (match-data nil (nth 2 elt))))
                                   (when regexp-flag
                                     (setq next-replacement (nth 4 elt)))
                                   ;; Set replaced nil to keep in loop
                                   (when (eq def 'undo-all)
                                     (setq replaced nil
                                           stack-len (- stack-len stack-idx)
                                           stack-idx 0
                                           num-replacements
                                           (1+ num-replacements))))))
                             (when (and (eq def 'undo-all)
                                        (null (zerop num-replacements)))
                               (message (ngettext "Undid %d replacement"
                                                  "Undid %d replacements"
                                                  num-replacements)
                                        num-replacements)
                               (ding 'no-terminate)
                               (sit-for 1)))
			   (setq replaced nil last-was-undo t last-was-act-and-show nil)))
			((eq def 'act)
			 (or replaced
			     (setq noedit
				   (replace-match-maybe-edit
				    next-replacement nocasify literal
				    noedit real-match-data backward)
				   replace-count (1+ replace-count)))
			 (setq done t replaced t update-stack (not last-was-act-and-show)))
			((eq def 'act-and-exit)
			 (or replaced
			     (setq noedit
				   (replace-match-maybe-edit
				    next-replacement nocasify literal
				    noedit real-match-data backward)
				   replace-count (1+ replace-count)))
			 (setq keep-going nil)
			 (setq done t replaced t))
			((eq def 'act-and-show)
			 (unless replaced
			     (setq noedit
				   (replace-match-maybe-edit
				    next-replacement nocasify literal
				    noedit real-match-data backward)
				   replace-count (1+ replace-count)
				   real-match-data (replace-match-data
						    nil real-match-data)
				   replaced t last-was-act-and-show t)
                             (replace--push-stack
                              replaced
                              search-string-replaced
                              next-replacement-replaced next-replacement match-again
                              stack)))
			((or (eq def 'automatic) (eq def 'automatic-all))
			 (or replaced
			     (setq noedit
				   (replace-match-maybe-edit
				    next-replacement nocasify literal
				    noedit real-match-data backward)
				   replace-count (1+ replace-count)))
			 (setq done t query-flag nil replaced t)
			 (if (eq def 'automatic-all) (setq multi-buffer t)))
			((eq def 'skip)
			 (setq done t update-stack (not last-was-act-and-show)))
			((eq def 'recenter)
			 ;; `this-command' has the value `query-replace',
			 ;; so we need to bind it to `recenter-top-bottom'
			 ;; to allow it to detect a sequence of `C-l'.
			 (let ((this-command 'recenter-top-bottom)
			       (last-command 'recenter-top-bottom))
			   (recenter-top-bottom)))
			((eq def 'edit)
			 (let ((opos (point-marker))
			       ;; Restore original isearch filter to allow
			       ;; using isearch in a recursive edit even
			       ;; when perform-replace was started from
			       ;; `xref--query-replace-1' that let-binds
			       ;; `isearch-filter-predicate' (bug#53758).
			       (isearch-filter-predicate #'isearch-filter-visible))
			   (setq real-match-data (replace-match-data
						  nil real-match-data
						  real-match-data))
			   (goto-char (match-beginning 0))
			   (save-excursion
			     (save-window-excursion
			       (recursive-edit)))
			   (goto-char opos)
			   (set-marker opos nil))
			 ;; Before we make the replacement,
			 ;; decide whether the search string
			 ;; can match again just after this match.
			 (if (and regexp-flag nonempty-match)
			     (setq match-again (and (looking-at search-string)
						    (match-data)))))
			;; Edit replacement.
			((or (eq def 'edit-replacement)
                             (eq def 'edit-replacement-exact-case))
			 (setq real-match-data (replace-match-data
						nil real-match-data
						real-match-data)
			       next-replacement
			       (read-string
                                (format "Edit replacement string%s: "
                                        (if (eq def
                                                'edit-replacement-exact-case)
                                            " (exact case)"
                                          ""))
                                next-replacement)
			       noedit nil)
			 (if replaced
			     (set-match-data real-match-data)
			   (setq noedit
				 (replace-match-maybe-edit
				  next-replacement
                                  (if (eq def 'edit-replacement-exact-case)
                                      t
                                    nocasify)
                                  literal noedit
				  real-match-data backward)
				 replaced t)
			   (setq next-replacement-replaced next-replacement))
			 (setq done t))

			((eq def 'delete-and-edit)
			 (replace-match "" t t)
			 (setq real-match-data (replace-match-data
						nil real-match-data))
			 (replace-dehighlight)
			 (save-excursion (recursive-edit))
			 (setq replaced t))

                        ((eq def 'diff)
                         (let ((display-buffer-overriding-action
                                ;; Override only the default value.
                                (if (equal display-buffer-overriding-action '(nil))
                                    '(nil (inhibit-same-window . t))
                                  display-buffer-overriding-action)))
                           (save-selected-window
                             (multi-file-replace-as-diff
                              (list (or buffer-file-name (current-buffer)))
                              from-string (or replacements next-replacement)
                              regexp-flag delimited-flag))))

                        ((commandp def t)
                         (call-interactively def))
			;; Note: we do not need to treat `exit-prefix'
			;; specially here, since we reread
			;; any unrecognized character.
			(t
			 (setq this-command 'mode-exited)
			 (setq keep-going nil)
			 (setq unread-command-events
			       (append (listify-key-sequence key)
				       unread-command-events))
			 (setq done t)))
		  (when query-replace-lazy-highlight
		    ;; Force lazy rehighlighting only after replacements.
		    (if (not (memq def '(skip backup)))
			(setq isearch-lazy-highlight-last-string nil)))
		  (unless (eq def 'recenter)
		    ;; Reset recenter cycling order to initial position.
		    (setq recenter-last-op nil)))
		;; Record previous position for ^ when we move on.
		;; Change markers to numbers in the match data
		;; since lots of markers slow down editing.
                (when update-stack
                  (replace--push-stack
                   replaced
                   search-string-replaced
                   next-replacement-replaced next-replacement match-again
                   stack))
                (setq next-replacement-replaced nil
                      search-string-replaced    nil
                      last-was-act-and-show     nil))))))
      (replace-dehighlight)
      (when region-filter
        (remove-function isearch-filter-predicate region-filter))
      (when (and query-flag (fboundp 'set-text-conversion-style))
        ;; Resume text conversion.
        (setq overriding-text-conversion-style
              old-text-conversion-style)
        (set-text-conversion-style text-conversion-style)))
    (or unread-command-events
	(message (ngettext "Replaced %d occurrence%s"
			   "Replaced %d occurrences%s"
			   replace-count)
		 replace-count
		 (if (> (+ skip-read-only-count
			   skip-filtered-count
			   skip-invisible-count)
                        0)
		     (format " (skipped %s)"
			     (mapconcat
			      #'identity
			      (delq nil (list
					 (if (> skip-read-only-count 0)
					     (format "%s read-only"
						     skip-read-only-count))
					 (if (> skip-invisible-count 0)
					     (format "%s invisible"
						     skip-invisible-count))
					 (if (> skip-filtered-count 0)
					     (format "%s filtered out"
						     skip-filtered-count))))
			      ", "))
		   "")))
    (or (and keep-going stack) multi-buffer)))

