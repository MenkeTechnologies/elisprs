;;; faces-31.el --- the faces of batch Emacs, in face-id order  -*- lexical-binding: t -*-

;; Verbatim `defface' forms from GNU Emacs 31.1 lisp/faces.el and the other
;; preloaded libraries that define faces, ordered as `(face-list)' of
;; `emacs -Q --batch' orders them (each library's in file order, libraries
;; in loadup.el's order), so `face-list' and face IDs match.  Generated with
;; the oracle: for each face of `(reverse (face-list))', its `defface' form
;; from the source of `(symbol-file FACE 'defface)'.

;;;; faces.el

(defface default
  '((t nil)) ; If this were nil, face-defface-spec would not be set.
  "Basic default face."
  :group 'basic-faces)

(defface bold
  '((t :weight bold))
  "Basic bold face."
  :group 'basic-faces)

(defface italic
  '((((supports :slant italic))
     :slant italic)
    (((supports :underline t))
     ;; Include italic, even if it isn't supported by the default
     ;; font, because this face could be merged with another face
     ;; which uses font that does have an italic variant.
     :underline t :slant italic)
    (t
     ;; Default to italic, even if it doesn't appear to be supported,
     ;; because in some cases the display engine will do its own
     ;; workaround (to `dim' on ttys).
     :slant italic))
  "Basic italic face."
  :group 'basic-faces)

(defface bold-italic
  '((t :weight bold :slant italic))
  "Basic bold-italic face."
  :group 'basic-faces)

(defface underline
  '((((supports :underline t))
     :underline t)
    ;; Include underline, for when this face is merged with another
    ;; whose font does support underline.
    (((supports :weight bold :underline t))
     :weight bold)
    (t :underline t))
  "Basic underlined face."
  :group 'basic-faces)

(defface fixed-pitch
  '((t :family "Monospace"))
  "The basic fixed-pitch face."
  :group 'basic-faces)

(defface fixed-pitch-serif
  '((t :family "Monospace Serif"))
  "The basic fixed-pitch face with serifs."
  :group 'basic-faces)

(defface variable-pitch
  '((((type w32))
     ;; This is a workaround for an issue discussed in
     ;; https://lists.gnu.org/r/emacs-devel/2016-04/msg00746.html.
     ;; We need (a) the splash screen not to pick up bold-italics variant of
     ;; the font, and (b) still be able to request bold/italic/larger size
     ;; variants in the likes of EWW.
     :family "Arial" :foundry "outline")
  (t :family "Sans Serif"))
  "The basic variable-pitch face."
  :group 'basic-faces)

(defface variable-pitch-text
  '((t :inherit variable-pitch
       :height 1.1))
  "The proportional face used for longer texts.
This is like the `variable-pitch' face, but is slightly bigger by
default."
  :version "29.1"
  :group 'basic-faces)

(defface shadow
  '((((class color grayscale) (min-colors 88) (background light))
     :foreground "grey50")
    (((class color grayscale) (min-colors 88) (background dark))
     :foreground "grey70")
    (((class color) (min-colors 8) (background light))
     :foreground "green")
    (((class color) (min-colors 8) (background dark))
     :foreground "yellow"))
  "Basic face for shadowed text."
  :group 'basic-faces
  :version "22.1")

(defface link
  '((((class color) (min-colors 88) (background light))
     :foreground "RoyalBlue3" :underline t)
    (((class color) (background light))
     :foreground "blue" :underline t)
    (((class color) (min-colors 88) (background dark))
     :foreground "cyan1" :underline t)
    (((class color) (background dark))
     :foreground "cyan" :underline t)
    (t :inherit underline))
  "Basic face for unvisited links."
  :group 'basic-faces
  :version "22.1")

(defface link-visited
  '((default :inherit link)
    (((class color) (background light)) :foreground "magenta4")
    (((class color) (background dark)) :foreground "violet"))
  "Basic face for visited links."
  :group 'basic-faces
  :version "22.1")

(defface highlight
  '((((class color) (min-colors 88) (background light))
     :background "darkseagreen2")
    (((class color) (min-colors 88) (background dark))
     :background "darkolivegreen")
    (((class color) (min-colors 16) (background light))
     :background "darkseagreen2")
    (((class color) (min-colors 16) (background dark))
     :background "darkolivegreen")
    (((class color) (min-colors 8))
     :background "green" :foreground "black")
    (t :inverse-video t))
  "Basic face for highlighting."
  :group 'basic-faces)

(defface region
  '((((class color) (min-colors 88) (background dark))
     :background "blue3" :extend t)
    (((class color) (min-colors 88) (background light))
     :background "lightgoldenrod2" :extend t)
    (((class color) (min-colors 16) (background dark))
     :background "blue3" :extend t)
    (((class color) (min-colors 16) (background light))
     :background "lightgoldenrod2" :extend t)
    (((class color) (min-colors 8))
     :background "blue" :foreground "white" :extend t)
    (((type tty) (class mono))
     :inverse-video t)
    (t :background "gray" :extend t))
  "Basic face for highlighting the region."
  :version "21.1"
  :group 'basic-faces)

(defface secondary-selection
  '((((class color) (min-colors 88) (background light))
     :background "yellow1" :extend t)
    (((class color) (min-colors 88) (background dark))
     :background "SkyBlue4" :extend t)
    (((class color) (min-colors 16) (background light))
     :background "yellow" :extend t)
    (((class color) (min-colors 16) (background dark))
     :background "SkyBlue4" :extend t)
    (((class color) (min-colors 8))
     :background "cyan" :foreground "black" :extend t)
    (t :inverse-video t))
  "Basic face for displaying the secondary selection."
  :group 'basic-faces)

(defface trailing-whitespace
  '((((class color) (background light))
     :background "red1")
    (((class color) (background dark))
     :background "red1")
    (t :inverse-video t))
  "Basic face for highlighting trailing whitespace."
  :version "21.1"
  :group 'basic-faces)

(defface line-number
  '((t :inherit (shadow default)))
  "Face for displaying line numbers.
This face is used when `display-line-numbers' is non-nil.

If you customize the font of this face, make sure it is a
monospaced font, otherwise line numbers will not line up,
and text lines might move horizontally as you move through
the buffer."
  :version "26.1"
  :group 'basic-faces
  :group 'display-line-numbers)

(defface line-number-current-line
  '((t :inherit line-number))
  "Face for displaying the current line number.
This face is used when `display-line-numbers' is non-nil.

If you customize the font of this face, make sure it is a
monospaced font, otherwise line numbers will not line up,
and text lines might move horizontally as you move through
the buffer.  Similarly, making this face's font different
from that of the `line-number' face could produce such
unwanted effects."
  :version "26.1"
  :group 'basic-faces
  :group 'display-line-numbers)

(defface line-number-major-tick
  '((((class color grayscale) (background light))
     :background "grey85" :weight bold)
    (((class color grayscale) (background dark))
     :background "grey75" :weight bold)
    (t :inherit line-number))
  "Face for highlighting \"major ticks\" (as in a ruler).
When `display-line-numbers-major-tick' is positive, highlight
the line numbers of lines which are a multiple of its value.
This face is used when `display-line-numbers' is non-nil.

If you customize the font of this face, make sure it is a
monospaced font, otherwise line numbers will not line up,
and text lines might move horizontally as you move through
the buffer.  Similarly, making this face's font different
from that of the `line-number' face could produce such
unwanted effects."
  :version "27.1"
  :group 'basic-faces
  :group 'display-line-numbers)

(defface line-number-minor-tick
  '((((class color grayscale) (background light))
     :background "grey95" :weight bold)
    (((class color grayscale) (background dark))
     :background "grey55" :weight bold)
    (t :inherit line-number))
  "Face for highlighting \"minor ticks\" (as in a ruler).
When `display-line-numbers-minor-tick' is positive, highlight
the line numbers of lines which are a multiple of its value.
This face is used when `display-line-numbers' is non-nil.

If you customize the font of this face, make sure it is a
monospaced font, otherwise line numbers will not line up,
and text lines might move horizontally as you move through
the buffer.  Similarly, making this face's font different
from that of the `line-number' face could produce such
unwanted effects."
  :version "27.1"
  :group 'basic-faces
  :group 'display-line-numbers)

(defface fill-column-indicator
  '((t :inherit shadow :weight normal :slant normal
       :underline nil :overline nil :strike-through nil
       :box nil :inverse-video nil :stipple nil))
  "Face for displaying fill column indicator.
This face is used when `display-fill-column-indicator-mode' is
non-nil."
  :version "27.1"
  :group 'basic-faces
  :group 'display-fill-column-indicator)

(defface escape-glyph
  '((((background dark)) :foreground "cyan")
    ;; See the comment in minibuffer-prompt for
    ;; the reason not to use blue on MS-DOS.
    (((type pc)) :foreground "magenta")
    ;; red4 is too dark, but some say blue is too loud.
    ;; brown seems to work ok. -- rms.
    (t :foreground "brown"))
  "Face for characters displayed as sequences using `^' or `\\'."
  :group 'basic-faces
  :version "22.1")

(defface homoglyph
  '((((background dark)) :foreground "cyan")
    (((type pc)) :foreground "magenta")
    (t :foreground "brown"))
  "Face for lookalike characters."
  :group 'basic-faces
  :version "26.1")

(defface nobreak-space
  '((((class color) (min-colors 88)) :inherit escape-glyph :underline t)
    (((class color) (min-colors 8)) :background "magenta")
    (t :inverse-video t))
  "Face for displaying nobreak space."
  :group 'basic-faces
  :version "22.1")

(defface nobreak-hyphen
  '((((background dark)) :foreground "cyan")
    (((type pc)) :foreground "magenta")
    (t :foreground "brown"))
  "Face for displaying nobreak hyphens."
  :group 'basic-faces
  :version "26.1")

(defface mode-line
  '((((class color grayscale) (min-colors 88) (background light))
     :box (:line-width -1 :style released-button)
     :background "grey75" :foreground "black")
    (((class color grayscale) (min-colors 88) (background dark))
     :box (:line-width -1 :style released-button)
     :background "grey20" :foreground "white")
    (t
     :inverse-video t))
  "Face for the mode lines as well as header lines.
See `mode-line-active' and `mode-line-inactive' for the faces
used on mode lines."
  :version "31.1"
  :group 'mode-line-faces
  :group 'basic-faces)

(defface mode-line-active
  '((t :inherit mode-line))
  "Face for the selected mode line.
This inherits from the `mode-line' face."
  :version "29.1"
  :group 'mode-line-faces
  :group 'basic-faces)

(defface mode-line-inactive
  '((default
     :inherit mode-line)
    (((class color grayscale) (min-colors 88) (background light))
     :weight light
     :box (:line-width -1 :color "grey75" :style nil)
     :foreground "grey20" :background "grey90")
    (((class color grayscale) (min-colors 88) (background dark) )
     :weight light
     :box (:line-width -1 :color "grey40" :style nil)
     :foreground "grey80" :background "grey30"))
  "Basic mode line face for non-selected windows."
  :version "22.1"
  :group 'mode-line-faces
  :group 'basic-faces)

(defface mode-line-highlight
  '((((supports :box t) (class color grayscale) (min-colors 88)
      (background light))
     :box (:line-width 2 :color "grey40" :style released-button))
    (((supports :box t) (class color grayscale) (min-colors 88)
      (background dark))
     :box (:line-width 2 :color "grey80" :style released-button))
    (t
     :inherit highlight))
  "Basic mode line face for highlighting."
  :version "31.1"
  :group 'mode-line-faces
  :group 'basic-faces)

(defface mode-line-emphasis
  '((t (:weight bold)))
  "Face used to emphasize certain mode line features.
Use the face `mode-line-highlight' for features that can be selected."
  :version "23.1"
  :group 'mode-line-faces
  :group 'basic-faces)

(defface mode-line-buffer-id
  '((t (:weight bold)))
  "Face used for buffer identification parts of the mode line."
  :version "22.1"
  :group 'mode-line-faces
  :group 'basic-faces)

(defface header-line
  '((default
     :inherit mode-line)
    (((type tty))
     ;; This used to be `:inverse-video t', but that doesn't look very
     ;; good when combined with inverse-video mode-lines and multiple
     ;; windows.  Underlining looks better, and is more consistent with
     ;; the window-system face variants, which deemphasize the
     ;; header-line in relation to the mode-line face.  If a terminal
     ;; can't underline, then the header-line will end up without any
     ;; highlighting; this may be too confusing in general, although it
     ;; happens to look good with the only current use of header-lines,
     ;; the info browser. XXX
     :inverse-video nil	       ;Override the value inherited from mode-line.
     :underline t)
    (((class color grayscale) (background light))
     :background "grey90" :foreground "grey20"
     :box nil)
    (((class color grayscale) (background dark))
     :background "grey20" :foreground "grey90"
     :box nil)
    (((class mono) (background light))
     :background "white" :foreground "black"
     :inverse-video nil
     :box nil
     :underline t)
    (((class mono) (background dark))
     :background "black" :foreground "white"
     :inverse-video nil
     :box nil
     :underline t))
  "Basic header-line face."
  :version "21.1"
  :group 'basic-faces)

(defface header-line-highlight '((t :inherit mode-line-highlight))
  "Basic header line face for highlighting."
  :version "28.1"
  :group 'basic-faces)

(defface header-line-active
  '((t :inherit header-line))
  "Face for the selected header line.
This inherits from the `header-line' face."
  :version "31.1"
  :group 'mode-line-faces
  :group 'basic-faces)

(defface header-line-inactive
  '((t :inherit header-line))
  "Basic header line face for non-selected windows."
  :version "31.1"
  :group 'mode-line-faces
  :group 'basic-faces)

(defface vertical-border
  '((((type tty)) :inherit mode-line-inactive))
  "Face used for vertical window dividers on ttys."
  :version "22.1"
  :group 'basic-faces)

(defface window-divider '((t :foreground "gray60"))
  "Basic face for window dividers.
When a divider is less than 3 pixels wide, it is drawn solidly
with the foreground of this face.  For larger dividers this face
is used for the inner part while the first pixel line/column is
drawn with the `window-divider-first-pixel' face and the last
pixel line/column with the `window-divider-last-pixel' face."
  :version "24.4"
  :group 'window-divider
  :group 'basic-faces)

(defface window-divider-first-pixel
  '((t :foreground "gray80"))
  "Basic face for first pixel line/column of window dividers.
When a divider is at least 3 pixels wide, its first pixel
line/column is drawn with the foreground of this face.  If you do
not want to accentuate the first pixel line/column, set this to
the same as `window-divider' face."
  :version "24.4"
  :group 'window-divider
  :group 'basic-faces)

(defface window-divider-last-pixel
  '((t :foreground "gray40"))
  "Basic face for last pixel line/column of window dividers.
When a divider is at least 3 pixels wide, its last pixel
line/column is drawn with the foreground of this face.  If you do
not want to accentuate the last pixel line/column, set this to
the same as `window-divider' face."
  :version "24.4"
  :group 'window-divider
  :group 'basic-faces)

(defface internal-border
    '((t nil))
  "Basic face for the internal border.
For the internal border of child frames see `child-frame-border'."
  :version "26.1"
  :group 'frames
  :group 'basic-faces)

(defface child-frame-border
  '((t nil))
  "Basic face for the internal border of child frames.
For the internal border of non-child frames see `internal-border'."
  :version "28.1"
  :group 'frames
  :group 'basic-faces)

(defface minibuffer-prompt
  '((((background dark)) :foreground "cyan")
    ;; Don't use blue because many users of the MS-DOS port customize
    ;; their foreground color to be blue.
    (((type pc)) :foreground "magenta")
    (t :foreground "medium blue"))
  "Face for minibuffer prompts.
By default, Emacs automatically adds this face to the value of
`minibuffer-prompt-properties', which is a list of text properties
used to display the prompt text."
  :version "22.1"
  :group 'basic-faces)

(defface margin
  '((t :inherit default))
  "Basic face for window margins (both left and right).
This face is used to customize the appearance of the margin areas."
  :version "31.1"
  :group 'basic-faces)

(defface fringe
  '((((class color) (background light))
     :background "grey95")
    (((class color) (background dark))
     :background "grey10")
    (t
     :background "gray"))
  "Basic face for the fringes to the left and right of windows under X."
  :version "21.1"
  :group 'frames
  :group 'basic-faces)

(defface scroll-bar '((t nil))
  "Basic face for the scroll bar colors under X."
  :version "21.1"
  :group 'frames
  :group 'basic-faces)

(defface border '((t nil))
  "Basic face for the frame border under X."
  :version "21.1"
  :group 'frames
  :group 'basic-faces)

(defface cursor
  '((((background light)) :background "black")
    (((background dark))  :background "white"))
  "Basic face for the cursor color under X.
Currently, only the `:background' attribute is meaningful; all
other attributes are ignored.  The cursor foreground color is
taken from the background color of the underlying text.

Note: Other faces cannot inherit from the cursor face."
  :version "21.1"
  :group 'cursor
  :group 'basic-faces)

(defface mouse '((t nil))
  "Basic face for the mouse color under X."
  :version "21.1"
  :group 'mouse
  :group 'basic-faces)

(defface tool-bar
  '((default
     :box (:line-width 1 :style released-button)
     :foreground "black")
    (((type haiku))
     :foreground "B_MENU_ITEM_TEXT_COLOR"
     :background "B_MENU_BACKGROUND_COLOR")
    (((type x w32 ns pgtk android) (class color))
     :background "grey75")
    (((type x) (class mono))
     :background "grey"))
  "Basic tool-bar face."
  :version "21.1"
  :group 'basic-faces)

(defface tab-bar
  '((((class color) (min-colors 88) (background light))
     :inherit variable-pitch
     :background "grey85"
     :foreground "black")
    (((class color) (min-colors 88) (background dark))
     :inherit variable-pitch
     :background "grey20"
     :foreground "white")
    (((class mono))
     :background "grey")
    (t
     :inverse-video t))
  "Tab bar face."
  :version "31.1"
  :group 'basic-faces)

(defface tab-line
  '((((class color) (min-colors 88) (background light))
     :inherit variable-pitch
     :height 0.9
     :background "grey85"
     :foreground "black")
    (((class color) (min-colors 88) (background dark))
     :inherit variable-pitch
     :height 0.9
     :background "grey20"
     :foreground "white")
    (((class mono))
     :background "grey")
    (t
     :inverse-video t))
  "Basic tab line face.
See `tab-line-active' and `tab-line-inactive' for the faces
used on tab lines."
  :version "31.1"
  :group 'basic-faces)

(defface tab-line-active
  '((t :inherit tab-line))
  "Face for the selected tab line.
This inherits from the `tab-line' face."
  :version "31.1"
  :group 'basic-faces)

(defface tab-line-inactive
  '((t :inherit tab-line))
  "Basic tab line face for non-selected windows."
  :version "31.1"
  :group 'basic-faces)

(defface menu
  '((((type tty))
     :inverse-video t)
    (((type x-toolkit))
     )
    (t
     :inverse-video t))
  "Basic face for the font and colors of the menu bar and popup menus."
  :version "21.1"
  :group 'menu
  :group 'basic-faces)

(defface help-argument-name '((t :inherit italic))
  "Face to highlight argument names in *Help* buffers."
  :group 'help)

(defface help-key-binding
  '((((class color) (min-colors 88) (background light))
     :background "grey96" :foreground "DarkBlue"
     ;; We use negative thickness of the horizontal box border line to
     ;; avoid enlarging the height of the echo-area display, which
     ;; would then move the mode line a few pixels up.  We use
     ;; negative thickness for the vertical border line to avoid
     ;; making the characters wider, which then would cause unpleasant
     ;; horizontal shifts of the cursor during C-n/C-p movement
     ;; through a line with this face.
     :box (:line-width (-1 . -1) :color "grey80")
     :inherit fixed-pitch)
    (((class color) (min-colors 88) (background dark))
     :background "grey19" :foreground "LightBlue"
     :box (:line-width (-1 . -1) :color "grey35")
     :inherit fixed-pitch)
    (((class color grayscale) (background light)) :background "grey90"
     :inherit fixed-pitch)
    (((class color grayscale) (background dark)) :background "grey25"
     :inherit fixed-pitch)
    (t :background "grey90" :inherit fixed-pitch))
  "Face for keybindings in *Help* buffers.

This face is added by `substitute-command-keys', which see.

Note that this face will also be used for key bindings in
tooltips.  This means that, for example, changing the :height of
this face will increase the height of any tooltip containing key
bindings.  See also the face `tooltip'."
  :version "28.1"
  :group 'help)

(defface glyphless-char
  '((((type tty)) :inherit escape-glyph :underline t)
    (((type pc)) :inherit escape-glyph)
    (t :height 0.6))
  "Face for displaying non-graphic characters (e.g. U+202A (LRE)).
It is used for characters of no fonts too."
  :version "24.1"
  :group 'basic-faces)

(defface error
  '((default :weight bold)
    (((class color) (min-colors 88) (background light)) :foreground "Red1")
    (((class color) (min-colors 88) (background dark))  :foreground "Pink")
    (((class color) (min-colors 16) (background light)) :foreground "Red1")
    (((class color) (min-colors 16) (background dark))  :foreground "Pink")
    (((class color) (min-colors 8)) :foreground "red")
    (t :inverse-video t))
  "Basic face used to highlight errors and to denote failure."
  :version "24.1"
  :group 'basic-faces)

(defface warning
  '((default :weight bold)
    (((class color) (min-colors 16)) :foreground "DarkOrange")
    (((class color)) :foreground "yellow"))
  "Basic face used to highlight warnings."
  :version "24.1"
  :group 'basic-faces)

(defface success
  '((default :weight bold)
    (((class color) (min-colors 16) (background light)) :foreground "ForestGreen")
    (((class color) (min-colors 88) (background dark))  :foreground "Green1")
    (((class color) (min-colors 16) (background dark))  :foreground "Green")
    (((class color)) :foreground "green"))
  "Basic face used to indicate successful operation."
  :version "24.1"
  :group 'basic-faces)

(defface read-multiple-choice-face
  '((t (:inherit (help-key-binding underline)
        :weight bold)))
  "Face for the symbol name in `read-multiple-choice' output."
  :group 'basic-faces
  :version "26.1")

(defface tty-menu-enabled-face
  '((((class color))
     :foreground "yellow" :background "blue" :weight bold)
    (t :weight bold))
  "Face for displaying enabled items in TTY menus."
  :group 'basic-faces
  :version "28.1")

(defface tty-menu-disabled-face
  '((((class color) (min-colors 16))
     :foreground "lightgray" :background "blue")
    (((class color))
     :foreground "white" :background "blue")
    (t :inherit shadow))
  "Face for displaying disabled items in TTY menus."
  :group 'basic-faces
  :version "28.1")

(defface tty-menu-selected-face
  '((((class color))
      :background "red")
    (t :inverse-video t))
  "Face for displaying the currently selected item in TTY menus."
  :group 'basic-faces
  :version "28.1")

(defface show-paren-match
  '((((class color) (background light))
     :background "turquoise")		; looks OK on tty (becomes cyan)
    (((class color) (background dark))
     :background "steelblue3")		; looks OK on tty (becomes blue)
    (((background dark) (min-colors 4))
     :background "grey50")
    (((background light) (min-colors 4))
     :background "gray")
    (t
     :inherit underline))
  "Face used for a matching paren."
  :group 'paren-showing-faces)

(defface show-paren-match-expression
  '((t :inherit show-paren-match))
  "Face used for a matching paren when highlighting the whole expression.
This face is used by `show-paren-mode'."
  :group 'paren-showing-faces
  :version "26.1")

(defface show-paren-mismatch
  '((((class color)) (:foreground "white" :background "purple"))
    (t (:inverse-video t)))
  "Face used for a mismatching paren."
  :group 'paren-showing-faces)

;;;; button.el

(defface button '((t :inherit link))
  "Default face used for buttons."
  :group 'basic-faces)

;;;; abbrev.el

(defface abbrev-table-name
  '((t :inherit font-lock-function-name-face))
  "Face used for displaying the abbrev table name in `edit-abbrevs-mode'."
  :version "29.1")

;;;; help.el

(defface help-for-help-header '((t :height 1.26))
  "Face used for headers in the `help-for-help' buffer."
  :group 'help)

;;;; mule-cmds.el

(defface confusingly-reordered
  '((((supports :underline (:style wave)))
     :underline (:style wave :color "Red1"))
    (t
     :inherit warning))
  "Face for highlighting text that was bidi-reordered in confusing ways."
  :version "29.1")

;;;; simple.el

(defface next-error
  '((t (:inherit region)))
  "Face used to highlight next error locus."
  :group 'next-error
  :version "22.1")

(defface next-error-message
  '((t (:inherit highlight :extend t)))
  "Face used to highlight the current error message in the `next-error' buffer."
  :group 'next-error
  :version "28.1")

(defface separator-line
  '((((type graphic) (background dark))
     :height 0.1 :background "#505050")
    (((type graphic) (background light))
     :height 0.1 :background "#a0a0a0")
    (t
     :foreground "ForestGreen" :underline t))
  "Face for separator lines."
  :version "29.1"
  :group 'text)

(defface blink-matching-paren-offscreen
  '((t :inherit show-paren-match))
  "Face for showing in the echo area matched open paren that is off-screen.
This face is used only when `blink-matching-paren-highlight-offscreen'
is non-nil."
  :version "30.1"
  :group 'paren-blinking)

;;;; minibuffer.el

(defface completions-group-title
  '((t :inherit shadow :slant italic))
  "Face used for the title text of the candidate group headlines."
  :version "28.1")

(defface completions-group-separator
  '((t :inherit shadow :strike-through t))
  "Face used for the separator lines between the candidate groups."
  :version "28.1")

(defface completions-annotations '((t :inherit (italic shadow)))
  "Face to use for annotations in the *Completions* buffer.
This face is only used if the strings used for completions
doesn't already specify a face.")

(defface completions-highlight
  '((t :inherit highlight))
  "Default face for highlighting the current completion candidate."
  :version "29.1")

(defface completions-first-difference
  '((t (:inherit bold)))
  "Face for the first character after point in completions.
See also the face `completions-common-part'.")

(defface completions-common-part
  '((((class color) (min-colors 16) (background light)) :foreground "blue3")
    (((class color) (min-colors 16) (background dark)) :foreground "lightblue"))
  "Face for the parts of completions which matched the pattern.
See also the face `completions-first-difference'.")

(defface minibuffer-nonselected
  '((t (:background "yellow" :foreground "dark red" :weight bold)))
  "Face for highlighting contents of non-selected minibuffer window.
Used by `minibuffer-nonselected-mode' for the contents of the minibuffer
window when the minibuffer remains active but its window is currently
not selected."
  :version "31.1")

;;;; font-lock.el

(defface font-lock-comment-face
  '((((class grayscale) (background light))
     :foreground "DimGray" :weight bold :slant italic)
    (((class grayscale) (background dark))
     :foreground "LightGray" :weight bold :slant italic)
    (((class color) (min-colors 88) (background light))
     :foreground "Firebrick")
    (((class color) (min-colors 88) (background dark))
     :foreground "chocolate1")
    (((class color) (min-colors 16) (background light))
     :foreground "red")
    (((class color) (min-colors 16) (background dark))
     :foreground "red1")
    (((class color) (min-colors 8) (background light))
     :foreground "red")
    (((class color) (min-colors 8) (background dark))
     :foreground "yellow")
    (t :weight bold :slant italic))
  "Font Lock mode face used to highlight comments."
  :group 'font-lock-faces)

(defface font-lock-comment-delimiter-face
  '((default :inherit font-lock-comment-face))
  "Font Lock mode face used to highlight comment delimiters."
  :group 'font-lock-faces)

(defface font-lock-string-face
  '((((class grayscale) (background light)) :foreground "DimGray" :slant italic)
    (((class grayscale) (background dark))  :foreground "LightGray" :slant italic)
    (((class color) (min-colors 88) (background light)) :foreground "VioletRed4")
    (((class color) (min-colors 88) (background dark))  :foreground "LightSalmon")
    (((class color) (min-colors 16) (background light)) :foreground "RosyBrown")
    (((class color) (min-colors 16) (background dark))  :foreground "LightSalmon")
    (((class color) (min-colors 8)) :foreground "green")
    (t :slant italic))
  "Font Lock mode face used to highlight strings."
  :group 'font-lock-faces)

(defface font-lock-doc-face
  '((t :inherit font-lock-string-face))
  "Font Lock mode face used to highlight documentation embedded in program code.
It is typically used for special documentation comments or strings."
  :group 'font-lock-faces)

(defface font-lock-doc-markup-face
  '((t :inherit font-lock-constant-face))
  "Font Lock mode face used to highlight embedded documentation mark-up.
It is meant for mark-up elements in text that uses `font-lock-doc-face', such
as the constructs of Haddock, Javadoc and similar systems."
  :version "28.1"
  :group 'font-lock-faces)

(defface font-lock-keyword-face
  '((((class grayscale) (background light)) :foreground "LightGray" :weight bold)
    (((class grayscale) (background dark))  :foreground "DimGray" :weight bold)
    (((class color) (min-colors 88) (background light)) :foreground "Purple")
    (((class color) (min-colors 88) (background dark))  :foreground "Cyan1")
    (((class color) (min-colors 16) (background light)) :foreground "Purple")
    (((class color) (min-colors 16) (background dark))  :foreground "Cyan")
    (((class color) (min-colors 8)) :foreground "cyan" :weight bold)
    (t :weight bold))
  "Font Lock mode face used to highlight keywords."
  :group 'font-lock-faces)

(defface font-lock-builtin-face
  '((((class grayscale) (background light)) :foreground "LightGray" :weight bold)
    (((class grayscale) (background dark))  :foreground "DimGray" :weight bold)
    (((class color) (min-colors 88) (background light)) :foreground "dark slate blue")
    (((class color) (min-colors 88) (background dark))  :foreground "LightSteelBlue")
    (((class color) (min-colors 16) (background light)) :foreground "Orchid")
    (((class color) (min-colors 16) (background dark)) :foreground "LightSteelBlue")
    (((class color) (min-colors 8)) :foreground "blue" :weight bold)
    (t :weight bold))
  "Font Lock mode face used to highlight builtins."
  :group 'font-lock-faces)

(defface font-lock-function-name-face
  '((((class color) (min-colors 88) (background light)) :foreground "Blue1")
    (((class color) (min-colors 88) (background dark))  :foreground "LightSkyBlue")
    (((class color) (min-colors 16) (background light)) :foreground "Blue")
    (((class color) (min-colors 16) (background dark))  :foreground "LightSkyBlue")
    (((class color) (min-colors 8)) :foreground "blue" :weight bold)
    (t :inverse-video t :weight bold))
  "Font Lock mode face used to highlight function names."
  :group 'font-lock-faces)

(defface font-lock-function-call-face
  '((t :inherit font-lock-function-name-face))
  "Font Lock mode face used to highlight function calls."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-variable-name-face
  '((((class grayscale) (background light))
     :foreground "Gray90" :weight bold :slant italic)
    (((class grayscale) (background dark))
     :foreground "DimGray" :weight bold :slant italic)
    (((class color) (min-colors 88) (background light)) :foreground "sienna")
    (((class color) (min-colors 88) (background dark))  :foreground "LightGoldenrod")
    (((class color) (min-colors 16) (background light)) :foreground "DarkGoldenrod")
    (((class color) (min-colors 16) (background dark))  :foreground "LightGoldenrod")
    (((class color) (min-colors 8)) :foreground "yellow" :weight light)
    (t :weight bold :slant italic))
  "Font Lock mode face used to highlight variable names."
  :group 'font-lock-faces)

(defface font-lock-variable-use-face
  '((t :inherit font-lock-variable-name-face))
  "Font Lock mode face used to highlight variable references."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-type-face
  '((((class grayscale) (background light)) :foreground "Gray90" :weight bold)
    (((class grayscale) (background dark))  :foreground "DimGray" :weight bold)
    (((class color) (min-colors 88) (background light)) :foreground "ForestGreen")
    (((class color) (min-colors 88) (background dark))  :foreground "PaleGreen")
    (((class color) (min-colors 16) (background light)) :foreground "ForestGreen")
    (((class color) (min-colors 16) (background dark))  :foreground "PaleGreen")
    (((class color) (min-colors 8)) :foreground "green")
    (t :weight bold :underline t))
  "Font Lock mode face used to highlight type and class names."
  :group 'font-lock-faces)

(defface font-lock-constant-face
  '((((class grayscale) (background light))
     :foreground "LightGray" :weight bold :underline t)
    (((class grayscale) (background dark))
     :foreground "Gray50" :weight bold :underline t)
    (((class color) (min-colors 88) (background light)) :foreground "dark cyan")
    (((class color) (min-colors 88) (background dark))  :foreground "Aquamarine")
    (((class color) (min-colors 16) (background light)) :foreground "CadetBlue")
    (((class color) (min-colors 16) (background dark))  :foreground "Aquamarine")
    (((class color) (min-colors 8)) :foreground "magenta")
    (t :weight bold :underline t))
  "Font Lock mode face used to highlight constants and labels."
  :group 'font-lock-faces)

(defface font-lock-warning-face
  '((t :inherit error))
  "Font Lock mode face used to highlight warnings."
  :group 'font-lock-faces)

(defface font-lock-negation-char-face
  '((t nil))
  "Font Lock mode face used to highlight easy to overlook negation."
  :group 'font-lock-faces)

(defface font-lock-preprocessor-face
  '((t :inherit font-lock-builtin-face))
  "Font Lock mode face used to highlight preprocessor directives."
  :group 'font-lock-faces)

(defface font-lock-regexp-face
  '((t :inherit font-lock-string-face))
  "Font Lock mode face used to highlight regexp literals."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-regexp-grouping-backslash
  '((t :inherit bold))
  "Font Lock mode face for backslashes in Lisp regexp grouping constructs."
  :group 'font-lock-faces)

(defface font-lock-regexp-grouping-construct
  '((t :inherit bold))
  "Font Lock mode face used to highlight grouping constructs in Lisp regexps."
  :group 'font-lock-faces)

(defface font-lock-escape-face
  '((t :inherit font-lock-regexp-grouping-backslash))
  "Font Lock mode face used to highlight escape sequences in strings."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-number-face
  '((t nil))
  "Font Lock mode face used to highlight numbers."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-operator-face
  '((t nil))
  "Font Lock mode face used to highlight operators."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-property-name-face
  '((t :inherit font-lock-variable-name-face))
  "Font Lock mode face used to highlight properties of an object.
For example, the declaration of fields in a struct."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-property-use-face
  '((t :inherit font-lock-property-name-face))
  "Font Lock mode face used to highlight property references.
For example, property lookup of fields in a struct."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-punctuation-face
  '((t nil))
  "Font Lock mode face used to highlight punctuation characters."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-bracket-face
  '((t :inherit font-lock-punctuation-face))
  "Font Lock mode face used to highlight brackets, braces, and parens."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-delimiter-face
  '((t :inherit font-lock-punctuation-face))
  "Font Lock mode face used to highlight delimiters.
What exactly is a delimiter depends on the major mode, but usually
these are characters like comma, colon, and semi-colon."
  :group 'font-lock-faces
  :version "29.1")

(defface font-lock-misc-punctuation-face
  '((t :inherit font-lock-punctuation-face))
  "Font Lock mode face used to highlight miscellaneous punctuation."
  :group 'font-lock-faces
  :version "29.1")

;;;; mouse.el

(defface mouse-drag-and-drop-region '((t :inherit region))
  "Face to highlight original text during dragging.
This face is used by `mouse-drag-and-drop-region' to temporarily
highlight the original region when
`mouse-drag-and-drop-region-show-cursor' is non-nil."
  :version "26.1")

;;;; isearch.el

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

;;;; rfn-eshadow.el

(defface file-name-shadow
  '((t :inherit shadow))
  "Face used by `file-name-shadow-mode' for the shadow."
  :group 'minibuffer
  :version "22.1")

;;;; tab-bar.el

(defface tab-bar-tab
  '((default
      :inherit tab-bar)
    (((class color) (min-colors 88) (background light))
     :box (:line-width 1 :style released-button))
    (((class color) (min-colors 88) (background dark))
     :box (:line-width 1 :style released-button)
     :background "grey40"
     :foreground "white")
    (t
     :inverse-video nil))
  "Tab bar face for selected tab."
  :version "31.1"
  :group 'tab-bar-faces)

(defface tab-bar-tab-inactive
  '((default
      :inherit tab-bar-tab)
    (((class color) (min-colors 88) (background light))
     :background "grey75")
    (((class color) (min-colors 88) (background dark))
     :background "grey20")
    (t
     :inverse-video t))
  "Tab bar face for non-selected tab."
  :version "31.1"
  :group 'tab-bar-faces)

(defface tab-bar-tab-group-current
  '((t :inherit tab-bar-tab :box nil :weight bold))
  "Tab bar face for current group tab."
  :version "28.1"
  :group 'tab-bar-faces)

(defface tab-bar-tab-group-inactive
  '((t :inherit (shadow tab-bar-tab-inactive)))
  "Tab bar face for inactive group tab."
  :version "28.1"
  :group 'tab-bar-faces)

(defface tab-bar-tab-ungrouped
  '((t :inherit (shadow tab-bar-tab-inactive)))
  "Tab bar face for ungrouped tab when tab groups are used."
  :version "28.1"
  :group 'tab-bar-faces)

(defface tab-bar-tab-highlight
  '((((class color) (min-colors 88) (background light))
     :box (:line-width 1 :style released-button)
     :background "grey85"
     :foreground "black")
    (((class color) (min-colors 88) (background dark))
     :box (:line-width 1 :style released-button)
     :background "grey40"
     :foreground "white")
    (t :inverse-video nil))
  "Tab bar face for highlighting."
  :version "31.1"
  :group 'tab-bar-faces)

;;;; replace.el

(defface query-replace
  '((t (:inherit isearch)))
  "Face for highlighting query replacement matches.
Used in `query-replace' and `query-replace-regexp'
when `query-replace-highlight' is non-nil"
  :group 'matching
  :version "22.1")

(defface match
  '((((class color) (min-colors 88) (background light))
     :background "khaki1")
    (((class color) (min-colors 88) (background dark))
     :background "RoyalBlue3")
    (((class color) (min-colors 8) (background light))
     :background "yellow" :foreground "black")
    (((class color) (min-colors 8) (background dark))
     :background "blue" :foreground "white")
    (((type tty) (class mono))
     :inverse-video t)
    (t :background "gray"))
  "Face used to highlight matches permanently."
  :group 'matching
  :group 'basic-faces
  :version "22.1")

;;;; tabulated-list.el

(defface tabulated-list-fake-header
  '((t :overline t :underline t :weight bold))
  "Face used on fake header lines."
  :version "27.1")

;;;; buff-menu.el

(defface buffer-menu-buffer
  '((t (:weight bold)))
  "Face for buffer names in the Buffer Menu."
  :group 'Buffer-menu)

;;;; elisp-mode.el

(defface elisp-symbol-at-mouse
  '((((background light)) :background "#fff6d8")
    (((background dark))  :background "#00422a"))
  "Face for highlighting the symbol at mouse in Emacs Lisp code."
  :version "31.1")

(defface elisp-free-variable
  '((t :underline t :foreground reset :inherit font-lock-variable-use-face))
  "Face for highlighting free (special) variables in Emacs Lisp code."
  :version "31.1")

(defface elisp-special-variable-declaration '((t :inherit elisp-free-variable))
  "Face for highlighting free variable declarations in Emacs Lisp code."
  :version "31.1")

(defface elisp-condition '((t :foreground "red"))
  "Face for highlighting `condition-case' conditions in Emacs Lisp code."
  :version "31.1")

(defface elisp-major-mode-name
  '((((background light)) :foreground "#006400")
    (((background dark))  :foreground "#4ade80"))
  "Face for highlighting major mode names in Emacs Lisp code.")

(defface elisp-face '((t :inherit font-lock-type-face))
  "Face for highlighting face names in Emacs Lisp code."
  :version "31.1")

(defface elisp-symbol-role
  '((default :inherit font-lock-function-call-face)
    (((background light)) :foreground "#00008b")
    (((background dark))  :foreground "#5c9cff"))
  "Face for highlighting symbol role names in Emacs Lisp code."
  :version "31.1")

(defface elisp-symbol-role-definition
  '((default :inherit font-lock-function-name-face)
    (((background light)) :foreground "#00008b")
    (((background dark))  :foreground "#5c9cff"))
  "Face for highlighting symbol role definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-function '((t :inherit font-lock-function-call-face))
  "Face for highlighting function calls in Emacs Lisp code."
  :version "31.1")

(defface elisp-non-local-exit '((t :inherit elisp-function :underline "red"))
  "Face for highlighting calls to functions that do not return."
  :version "31.1")

(defface elisp-unknown-call
  '((default :inherit elisp-function)
    (((background light)) :foreground "#2f4f4f")
    (((background dark))  :foreground "#7fa9a9"))
  "Face for highlighting unknown functions/macros in Emacs Lisp code."
  :version "31.1")

(defface elisp-macro '((t :inherit font-lock-keyword-face))
  "Face for highlighting macro calls in Emacs Lisp code."
  :version "31.1")

(defface elisp-special-form '((t :inherit elisp-macro))
  "Face for highlighting special forms in Emacs Lisp code."
  :version "31.1")

(defface elisp-throw-tag '((t :inherit font-lock-constant-face))
  "Face for highlighting `catch'/`throw' tags in Emacs Lisp code."
  :version "31.1")

(defface elisp-feature '((t :inherit font-lock-constant-face))
  "Face for highlighting feature names in Emacs Lisp code."
  :version "31.1")

(defface elisp-rx
  '((((background light)) :foreground "#00008b")
    (((background dark))  :foreground "#5c9cff"))
  "Face for highlighting `rx' constructs in Emacs Lisp code."
  :version "31.1")

(defface elisp-theme '((t :inherit font-lock-constant-face))
  "Face for highlighting custom theme names in Emacs Lisp code."
  :version "31.1")

(defface elisp-binding-variable
  '((t :slant italic :inherit font-lock-variable-name-face))
  "Face for highlighting binding occurrences of variables in Emacs Lisp code."
  :version "31.1")

(defface elisp-bound-variable
  '((t :slant italic :foreground reset :inherit font-lock-variable-use-face))
  "Face for highlighting bound occurrences of variables in Emacs Lisp code."
  :version "31.1")

(defface elisp-shadowing-variable
  '((t :inherit elisp-binding-variable :underline t))
  "Face for highlighting local bindings that shadow special variables."
  :version "31.1")

(defface elisp-shadowed-variable
  '((t :inherit elisp-bound-variable :underline t))
  "Face for highlighting special variables that are shadowed by a local binding."
  :version "31.1")

(defface elisp-variable-at-point '((t :inherit bold))
  "Face for highlighting (all occurrences of) the variable at point."
  :version "31.1")

(defface elisp-warning-type '((t :inherit font-lock-type-face))
  "Face for highlighting byte-compilation warning type names in Emacs Lisp."
  :version "31.1")

(defface elisp-function-property-declaration '((t :inherit font-lock-variable-use-face))
  "Face for highlighting function/macro property declaration type names."
  :version "31.1")

(defface elisp-thing '((t :inherit font-lock-type-face))
  "Face for highlighting `thing-at-point' \"thing\" names in Emacs Lisp."
  :version "31.1")

(defface elisp-slot '((t :inherit font-lock-builtin-face))
  "Face for highlighting EIEIO slot names."
  :version "31.1")

(defface elisp-widget-type '((t :inherit font-lock-type-face))
  "Face for highlighting widget type names in Emacs Lisp code."
  :version "31.1")

(defface elisp-type '((t :inherit font-lock-type-face))
  "Face for highlighting object type names in Emacs Lisp code."
  :version "31.1")

(defface elisp-group '((t :inherit font-lock-type-face))
  "Face for highlighting customization group names in Emacs Lisp code."
  :version "31.1")

(defface elisp-nnoo-backend '((t :inherit font-lock-type-face))
  "Face for highlighting `nnoo' backend names in Emacs Lisp code."
  :version "31.1")

(defface elisp-ampersand '((t :inherit font-lock-type-face))
  "Face for highlighting argument list markers, such as `&optional'."
  :version "31.1")

(defface elisp-constant '((t :inherit font-lock-builtin-face))
  "Face for highlighting self-evaluating symbols in Emacs Lisp code."
  :version "31.1")

(defface elisp-defun '((t :inherit font-lock-function-name-face))
  "Face for highlighting function definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-defmacro '((t :inherit elisp-defun))
  "Face for highlighting macro definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-defvar '((t :inherit font-lock-variable-name-face))
  "Face for highlighting variable definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-defface '((t :inherit font-lock-variable-name-face))
  "Face for highlighting face definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-icon '((t :inherit font-lock-type-face))
  "Face for highlighting icon names in Emacs Lisp code."
  :version "31.1")

(defface elisp-deficon '((t :inherit elisp-icon))
  "Face for highlighting icon definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-oclosure '((t :inherit font-lock-type-face))
  "Face for highlighting OClosure type names in Emacs Lisp code."
  :version "31.1")

(defface elisp-defoclosure '((t :inherit elisp-oclosure))
  "Face for highlighting OClosure type definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-coding '((t :inherit font-lock-type-face))
  "Face for highlighting coding system names in Emacs Lisp code."
  :version "31.1")

(defface elisp-defcoding '((t :inherit elisp-coding))
  "Face for highlighting coding system definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-charset '((t :inherit font-lock-type-face))
  "Face for highlighting charset names in Emacs Lisp code."
  :version "31.1")

(defface elisp-defcharset '((t :inherit elisp-charset))
  "Face for highlighting charset definitions in Emacs Lisp code."
  :version "31.1")

(defface elisp-completion-category '((t :inherit font-lock-type-face))
  "Face for highlighting completion category names in Emacs Lisp code."
  :version "31.1")

(defface elisp-completion-category-definition
  '((t :inherit elisp-completion-category))
  "Face for highlighting completion category definitions in Emacs Lisp code."
  :version "31.1")

;;;; vc-hooks.el

(defface vc-state-base
  '((default))
  "Base face for VC state indicator."
  :group 'vc-faces
  :group 'mode-line
  :version "25.1")

(defface vc-up-to-date-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file is up to date."
  :version "25.1")

(defface vc-needs-update-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file needs update."
  :version "25.1")

(defface vc-locked-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file locked."
  :version "25.1")

(defface vc-locally-added-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file is locally added."
  :version "25.1")

(defface vc-conflict-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file contains merge conflicts."
  :version "25.1")

(defface vc-removed-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file was removed from the VC system."
  :version "25.1")

(defface vc-missing-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file is missing from the file system."
  :version "25.1")

(defface vc-edited-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file is edited."
  :version "25.1")

(defface vc-ignored-state
  '((default :inherit vc-state-base))
  "Face for VC modeline state when the file is registered, but ignored."
  :version "30.1")

;;;; shorthands.el

(defface elisp-shorthand-font-lock-face
  '((t :inherit font-lock-keyword-face :foreground "cyan"))
  "Face for highlighting shorthands in Emacs Lisp."
  :version "28.1"
  :group 'font-lock-faces)

;;;; eldoc.el

(defface eldoc-highlight-function-argument
  '((t (:inherit bold)))
  "Face used for the argument at point in a function's argument list.
Note that this face has no effect unless the `eldoc-documentation-strategy'
handles it explicitly.")

;;;; tooltip.el

(defface tooltip
  '((((class color))
     :background "lightyellow"
     :foreground "black"
     :inherit variable-pitch)
    (t
     :inherit variable-pitch))
  "Face for tooltips.

When using the GTK toolkit, NS, or Haiku, this face will only
be used if `use-system-tooltips' is nil."
  :group 'tooltip
  :group 'basic-faces)
