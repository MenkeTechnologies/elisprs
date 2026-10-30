;;; Verbatim definitions from Emacs 31.1 lisp/subr.el, lisp/files.el,
;;; lisp/minibuffer.el, lisp/font-core.el, lisp/startup.el, lisp/emacs-lisp/syntax.el
;;; and lisp/emacs-lisp/easy-mmode.el that prog-mode.el, lisp-mode.el and
;;; elisp-mode.el need while they load.  -*- lexical-binding: t -*-

;;;; subr.el

(defconst menu-bar-separator '("--")
  "Separator for menus.")

;;;; files.el

(defcustom require-final-newline nil
  "Whether to add a newline automatically at the end of the file.

A value of t means do this only when the file is about to be saved.
A value of `visit' means do this right after the file is visited.
A value of `visit-save' means do it at both of those times.
Any other non-nil value means ask user whether to add a newline, when saving.
A value of nil means don't add newlines.

Certain major modes set this locally to the value obtained
from `mode-require-final-newline'.

This variable is heeded only when visiting files (or saving
buffers into files they visit).  Writing data to the file system
with `write-region' and the like is not influenced by this variable."
  :safe #'symbolp
  :type '(choice (const :tag "When visiting" visit)
		 (const :tag "When saving" t)
		 (const :tag "When visiting or saving" visit-save)
		 (const :tag "Don't add newlines" nil)
		 (other :tag "Ask each time" ask))
  :group 'editing-basics)

(defcustom mode-require-final-newline t
  "Whether to add a newline at end of file, in certain major modes.
Those modes set `require-final-newline' to this value when you enable them.
They do so because they are often used for files that are supposed
to end in newlines, and the question is how to arrange that.

A value of t means do this only when the file is about to be saved.
A value of `visit' means do this right after the file is visited.
A value of `visit-save' means do it at both of those times.
Any other non-nil value means ask user whether to add a newline, when saving.

A value of nil means do not add newlines.  That is a risky choice in this
variable since this value is used for modes for files that ought to have
final newlines.  So if you set this to nil, you must explicitly check and
add a final newline, whenever you save a file that really needs one."
  :type '(choice (const :tag "When visiting" visit)
		 (const :tag "When saving" t)
		 (const :tag "When visiting or saving" visit-save)
		 (const :tag "Don't add newlines" nil)
		 (other :tag "Ask each time" ask))
  :group 'editing-basics
  :version "22.1")

(defvar auto-mode-alist
  ;; Note: The entries for the modes defined in cc-mode.el (c-mode,
  ;; c++-mode, java-mode and more) are added through autoload
  ;; directives in that file.
  `(;; do this first, so that .html.pl is Polish html, not Perl
    ("\\.[sx]?html?\\(\\.[a-zA-Z_]+\\)?\\'" . mhtml-mode)
    ("\\.svgz?\\'" . image-mode)
    ("\\.svgz?\\'" . xml-mode)
    ("\\.x[bp]m\\'" . image-mode)
    ("\\.x[bp]m\\'" . c-mode)
    ("\\.p[bpgn]m\\'" . image-mode)
    ("\\.tiff?\\'" . image-mode)
    ("\\.gif\\'" . image-mode)
    ("\\.png\\'" . image-mode)
    ("\\.jpe?g\\'" . image-mode)
    ("\\.webp\\'" . image-mode)
    ("\\.te?xt\\'" . text-mode)
    ("\\.[tT]e[xX]\\'" . tex-mode)
    ("\\.ins\\'" . tex-mode)		;Installation files for TeX packages.
    ("\\.ltx\\'" . latex-mode)
    ("\\.dtx\\'" . doctex-mode)
    ("\\.org\\'" . org-mode)
    ;; .dir-locals.el is not really Elisp.  Could use the
    ;; `dir-locals-file' constant if it weren't defined below.
    ("\\.dir-locals\\(?:-2\\)?\\.el\\'" . lisp-data-mode)
    ("\\.eld\\'" . lisp-data-mode)
    ;; FIXME: The lisp-data-mode files below should use the `.eld' extension
    ;; (or a -*- mode cookie) so we don't need ad-hoc entries here.
    ("eww-bookmarks\\'" . lisp-data-mode)
    ("tramp\\'" . lisp-data-mode)
    ("/archive-contents\\'" . lisp-data-mode)
    ("places\\'" . lisp-data-mode)
    ("\\.emacs-places\\'" . lisp-data-mode)
    ("\\.el\\'" . emacs-lisp-mode)
    ("Project\\.ede\\'" . emacs-lisp-mode)
    ("\\(?:\\.\\(?:scm\\|sls\\|sld\\|stk\\|ss\\|sch\\)\\|/\\.guile\\)\\'" . scheme-mode)
    ("\\.l\\'" . lisp-mode)
    ("\\.li?sp\\'" . lisp-mode)
    ("\\.[fF]\\'" . fortran-mode)
    ("\\.for\\'" . fortran-mode)
    ("\\.p\\'" . pascal-mode)
    ("\\.pas\\'" . pascal-mode)
    ("\\.\\(dpr\\|DPR\\)\\'" . opascal-mode)
    ("\\.\\([pP]\\([Llm]\\|erl\\|od\\)\\|al\\)\\'" . perl-mode)
    ("Imakefile\\'" . makefile-imake-mode)
    ("Makeppfile\\(?:\\.mk\\)?\\'" . makefile-makepp-mode) ; Put this before .mk
    ("\\.makepp\\'" . makefile-makepp-mode)
    ,@(if (memq system-type '(berkeley-unix darwin))
	  '(("\\.mk\\'" . makefile-bsdmake-mode)
	    ("\\.make\\'" . makefile-bsdmake-mode)
	    ("GNUmakefile\\'" . makefile-gmake-mode)
	    ("[Mm]akefile\\'" . makefile-bsdmake-mode))
	'(("\\.mk\\'" . makefile-gmake-mode)	; Might be any make, give GNU the host advantage
	  ("\\.make\\'" . makefile-gmake-mode)
	  ("[Mm]akefile\\'" . makefile-gmake-mode)))
    ("\\.am\\'" . makefile-automake-mode)
    ;; Less common extensions come here
    ;; so more common ones above are found faster.
    ("\\.texinfo\\'" . texinfo-mode)
    ("\\.te?xi\\'" . texinfo-mode)
    ("\\.[sS]\\'" . asm-mode)
    ("\\.asm\\'" . asm-mode)
    ("\\.css\\'" . css-mode)
    ("\\.mixal\\'" . mixal-mode)
    ("\\.gcov\\'" . compilation-mode)
    ;; Besides .gdbinit, gdb documents other names to be usable for init
    ;; files, cross-debuggers can use something like
    ;; .PROCESSORNAME-gdbinit so that the host and target gdbinit files
    ;; don't interfere with each other.
    ("/[._]?[A-Za-z0-9-]*\\(?:gdbinit\\(?:\\.\\(?:ini?\\|loader\\)\\)?\\|gdb\\.ini\\)\\'" . gdb-script-mode)
    ;; GDB 7.5 introduced OBJFILE-gdb.gdb script files; e.g. a file
    ;; named 'emacs-gdb.gdb', if it exists, will be automatically
    ;; loaded when GDB reads an objfile called 'emacs'.
    ("-gdb\\.gdb" . gdb-script-mode)
    ("[cC]hange\\.?[lL]og?\\'" . change-log-mode)
    ("[cC]hange[lL]og[-.][0-9]+\\'" . change-log-mode)
    ("\\$CHANGE_LOG\\$\\.TXT" . change-log-mode)
    ("\\.scm\\.[0-9]*\\'" . scheme-mode)
    ("\\.[ckz]?sh\\'\\|\\.shar\\'\\|/\\.z?profile\\'" . sh-mode)
    ("\\.bash\\'" . sh-mode)
    ;; Bash builtin 'fc' creates a temp file named "bash-fc.XXXXXX"
    ;; to edit shell commands from its history list.
    ("/bash-fc\\.[0-9A-Za-z]\\{6\\}\\'" . sh-mode)
    ;; A login sh first attempts to read /etc/profile.
    ("\\`/etc/profile\\'" . sh-mode)
    ("/PKGBUILD\\'" . sh-mode)
    ("\\(/\\|\\`\\)\\.\\(bash_\\(profile\\|history\\|log\\(in\\|out\\)\\)\\|z?log\\(in\\|out\\)\\)\\'" . sh-mode)
    ("\\(/\\|\\`\\)\\.\\(shrc\\|zshrc\\|m?kshrc\\|bashrc\\|t?cshrc\\|esrc\\)\\'" . sh-mode)
    ("\\(/\\|\\`\\)\\.\\([kz]shenv\\|xinitrc\\|startxrc\\|xsession\\)\\'" . sh-mode)
    ("\\.m?spec\\'" . sh-mode)
    ("\\.m[mes]\\'" . nroff-mode)
    ("\\.man\\'" . nroff-mode)
    ("\\.sty\\'" . latex-mode)
    ("\\.cl[so]\\'" . latex-mode)		;LaTeX 2e class option
    ("\\.bbl\\'" . latex-mode)
    ("\\.bib\\'" . bibtex-mode)
    ("\\.bst\\'" . bibtex-style-mode)
    ("\\.sql\\'" . sql-mode)
    ;; These .m4 files are Autoconf files.
    ("\\(acinclude\\|aclocal\\|acsite\\)\\.m4\\'" . autoconf-mode)
    ("\\.m[4c]\\'" . m4-mode)
    ("\\.mf\\'" . metafont-mode)
    ("\\.mp\\'" . metapost-mode)
    ("\\.vhdl?\\'" . vhdl-mode)
    ("\\.article\\'" . text-mode)
    ("\\.letter\\'" . text-mode)
    ("\\.i?tcl\\'" . tcl-mode)
    ("\\.exp\\'" . tcl-mode)
    ("\\.itk\\'" . tcl-mode)
    ("\\.icn\\'" . icon-mode)
    ("\\.sim\\'" . simula-mode)
    ("\\.mss\\'" . scribe-mode)
    ;; The Fortran standard does not say anything about file extensions.
    ;; .f90 was widely used for F90, now we seem to be trapped into
    ;; using a different extension for each language revision.
    ;; Anyway, the following extensions are supported by gfortran.
    ("\\.f9[05]\\'" . f90-mode)
    ("\\.f0[38]\\'" . f90-mode)
    ("\\.srt\\'" . srecode-template-mode)
    ("\\.prolog\\'" . prolog-mode)
    ("\\.tar\\'" . tar-mode)
    ;; The list of archive file extensions should be in sync with
    ;; `auto-coding-alist' with `no-conversion' coding system.
    ("\\.\\(\
arc\\|zip\\|lzh\\|lha\\|zoo\\|[jew]ar\\|xpi\\|rar\\|cbr\\|7z\\|squashfs\\|\
ARC\\|ZIP\\|LZH\\|LHA\\|ZOO\\|[JEW]AR\\|XPI\\|RAR\\|CBR\\|7Z\\|SQUASHFS\\)\\'" . archive-mode)
    ("\\.oxt\\'" . archive-mode) ;(Open|Libre)Office extensions.
    ("\\.\\(deb\\|[oi]pk\\)\\'" . archive-mode) ; Debian/Opkg packages.
    ;; Mailer puts message to be edited in
    ;; /tmp/Re.... or Message
    ("\\`/tmp/Re" . text-mode)
    ("/Message[0-9]*\\'" . text-mode)
    ;; some news reader is reported to use this
    ("\\`/tmp/fol/" . text-mode)
    ("\\.oak\\'" . scheme-mode)
    ("\\.sgml?\\'" . sgml-mode)
    ("\\.x[ms]l\\'" . xml-mode)
    ("\\.slnx\\'" . xml-mode)
    ("\\.dbk\\'" . xml-mode)
    ("\\.dtd\\'" . sgml-mode)
    ("\\.ds\\(ss\\)?l\\'" . dsssl-mode)
    ("\\.js[mx]?\\'" . javascript-mode)
    ;; https://en.wikipedia.org/wiki/.har
    ("\\.har\\'" . javascript-mode)
    ("\\.json\\'" . js-json-mode)
    ("\\.[ds]?va?h?\\'" . verilog-mode)
    ("\\.by\\'" . bovine-grammar-mode)
    ("\\.wy\\'" . wisent-grammar-mode)
    ("\\.erts\\'" . erts-mode)
    ;; .emacs or .gnus or .viper following a directory delimiter in
    ;; Unix or MS-DOS syntax.
    ("[:/\\]\\..*\\(emacs\\|gnus\\|viper\\)\\'" . emacs-lisp-mode)
    ("\\`\\..*emacs\\'" . emacs-lisp-mode)
    ;; _emacs following a directory delimiter in MS-DOS syntax
    ("[:/]_emacs\\'" . emacs-lisp-mode)
    ("/crontab\\.X*[0-9]+\\'" . shell-script-mode)
    ("\\.ml\\'" . lisp-mode)
    ;; Linux-2.6.9 uses some different suffix for linker scripts:
    ;; "ld", "lds", "lds.S", "lds.in", "ld.script", and "ld.script.balo".
    ;; eCos uses "ld" and "ldi".  Netbsd uses "ldscript.*".
    ("\\.ld[si]?\\'" . ld-script-mode)
    ("ld\\.?script\\'" . ld-script-mode)
    ;; .xs is also used for ld scripts, but seems to be more commonly
    ;; associated with Perl .xs files (C with Perl bindings).  (Bug#7071)
    ("\\.xs\\'" . c-mode)
    ;; Explained in binutils ld/genscripts.sh.  Eg:
    ;; A .x script file is the default script.
    ;; A .xr script is for linking without relocation (-r flag).  Etc.
    ("\\.x[abdsru]?[cnw]?\\'" . ld-script-mode)
    ("\\.zone\\'" . dns-mode)
    ("\\.soa\\'" . dns-mode)
    ;; Common Lisp ASDF package system.
    ("\\.asd\\'" . lisp-mode)
    ("\\.\\(asn\\|mib\\|smi\\)\\'" . snmp-mode)
    ("\\.\\(as\\|mi\\|sm\\)2\\'" . snmpv2-mode)
    ("\\.\\(diffs?\\|patch\\|rej\\)\\'" . diff-mode)
    ("\\.\\(dif\\|pat\\)\\'" . diff-mode) ; for MS-DOS
    ("\\.[eE]?[pP][sS]\\'" . ps-mode)
    ("\\.\\(?:PDF\\|EPUB\\|CBZ\\|FB2\\|O?XPS\\|DVI\\|OD[FGPST]\\|DOCX\\|XLSX?\\|PPTX?\\|pdf\\|epub\\|cbz\\|fb2\\|o?xps\\|djvu\\|dvi\\|od[fgpst]\\|docx\\|xlsx?\\|pptx?\\)\\'" . doc-view-mode-maybe)
    ("configure\\.\\(ac\\|in\\)\\'" . autoconf-mode)
    ("\\.s\\(v\\|iv\\|ieve\\)\\'" . sieve-mode)
    ("BROWSE\\'" . ebrowse-tree-mode)
    ("\\.ebrowse\\'" . ebrowse-tree-mode)
    ("#\\*mail\\*" . mail-mode)
    ("\\.g\\'" . antlr-mode)
    ("\\.g4\\'" . antlr-v4-mode)
    ("\\.mod\\'" . m2-mode)
    ("\\.ses\\'" . ses-mode)
    ("\\.docbook\\'" . sgml-mode)
    ("\\.com\\'" . dcl-mode)
    ("/config\\.\\(?:bat\\|log\\)\\'" . fundamental-mode)
    ("/\\.?\\(authinfo\\|netrc\\)\\'" . authinfo-mode)
    ;; Windows candidates may be opened case sensitively on Unix
    ("\\.\\(?:[iI][nN][iI]\\|[lL][sS][tT]\\|[rR][eE][gG]\\|[sS][yY][sS]\\)\\'" . conf-mode)
    ("\\.la\\'" . conf-unix-mode)
    ("\\.ppd\\'" . conf-ppd-mode)
    ("java.+\\.conf\\'" . conf-javaprop-mode)
    ("\\.properties\\(?:\\.[a-zA-Z0-9._-]+\\)?\\'" . conf-javaprop-mode)
    ("\\.toml\\'" . conf-toml-mode)
    ("\\.desktop\\'" . conf-desktop-mode)
    ;; Dot is excluded from npmrc, because global configs may lack it,
    ;; e.g. in /etc/npmrc files.
    ("npmrc\\'" . conf-npmrc-mode)
    ("/\\.redshift\\.conf\\'" . conf-windows-mode)
    ("\\`/etc/\\(?:DIR_COLORS\\|ethers\\|.?fstab\\|.*hosts\\|lesskey\\|login\\.?de\\(?:fs\\|vperm\\)\\|magic\\|mtab\\|pam\\.d/.*\\|permissions\\(?:\\.d/.+\\)?\\|protocols\\|rpc\\|services\\)\\'" . conf-space-mode)
    ("\\`/etc/\\(?:acpid?/.+\\|aliases\\(?:\\.d/.+\\)?\\|default/.+\\|group-?\\|hosts\\..+\\|inittab\\|ksysguarddrc\\|opera6rc\\|passwd-?\\|shadow-?\\|sysconfig/.+\\)\\'" . conf-mode)
    ;; ChangeLog.old etc.  Other change-log-mode entries are above;
    ;; this has lower priority to avoid matching changelog.sgml etc.
    ("[cC]hange[lL]og[-.][-0-9a-z]+\\'" . change-log-mode)
    ;; either user's dot-files or under /etc or some such
    ("/\\.?\\(?:gitconfig\\|gnokiirc\\|hgrc\\|kde.*rc\\|mime\\.types\\|wgetrc\\)\\'" . conf-mode)
    ("/\\.mailmap\\'" . conf-unix-mode)
    ;; alas not all ~/.*rc files are like this
    ("/\\.\\(?:asound\\|enigma\\|fetchmail\\|gltron\\|gtk\\|hxplayer\\|mairix\\|mbsync\\|msmtp\\|net\\|neverball\\|nvidia-settings-\\|offlineimap\\|qt/.+\\|realplayer\\|reportbug\\|rtorrent\\.\\|screen\\|scummvm\\|sversion\\|sylpheed/.+\\|xmp\\)rc\\'" . conf-mode)
    ("/\\.\\(?:gdbtkinit\\|grip\\|mpdconf\\|notmuch-config\\|orbital/.+txt\\|rhosts\\|tuxracer/options\\)\\'" . conf-mode)
    ("/\\.?X\\(?:default\\|resource\\|re\\)s\\>" . conf-xdefaults-mode)
    ("/X11.+app-defaults/\\|\\.ad\\'" . conf-xdefaults-mode)
    ("/X11.+locale/.+/Compose\\'" . conf-colon-mode)
    ;; this contains everything twice, with space and with colon :-(
    ("/X11.+locale/compose\\.dir\\'" . conf-javaprop-mode)
    ;; Get rid of any trailing .n.m and try again.
    ;; This is for files saved by cvs-merge that look like .#<file>.<rev>
    ;; or .#<file>.<rev>-<rev> or VC's <file>.~<rev>~.
    ;; Using mode nil rather than `ignore' would let the search continue
    ;; through this list (with the shortened name) rather than start over.
    ("\\.~?[0-9]+\\.[0-9][-.0-9]*~?\\'" nil t)
    ("\\.\\(?:orig\\|in\\|[bB][aA][kK]\\)\\'" nil t)
    ;; This should come after "in" stripping (e.g. config.h.in).
    ;; *.cf, *.cfg, *.conf, *.config[.local|.de_DE.UTF8|...], */config
    ("[/.]c\\(?:on\\)?f\\(?:i?g\\)?\\(?:\\.[a-zA-Z0-9._-]+\\)?\\'" . conf-mode-maybe)
    ;; The following should come after the ChangeLog pattern
    ;; for the sake of ChangeLog.1, etc.
    ;; and after the .scm.[0-9] and CVS' <file>.<rev> patterns too.
    ("\\.[1-9]\\'" . nroff-mode)
    ;; Image file types probably supported by `image-convert'.
    ("\\.avif\\'" . image-mode)
    ("\\.art\\'" . image-mode)
    ("\\.avs\\'" . image-mode)
    ("\\.bmp\\'" . image-mode)
    ("\\.cmyk\\'" . image-mode)
    ("\\.cmyka\\'" . image-mode)
    ("\\.crw\\'" . image-mode)
    ("\\.dcm\\'" . image-mode)
    ("\\.dcr\\'" . image-mode)
    ("\\.dcx\\'" . image-mode)
    ("\\.dng\\'" . image-mode)
    ("\\.dpx\\'" . image-mode)
    ("\\.fax\\'" . image-mode)
    ("\\.heic\\'" . image-mode)
    ("\\.hrz\\'" . image-mode)
    ("\\.icb\\'" . image-mode)
    ("\\.icc\\'" . image-mode)
    ("\\.icm\\'" . image-mode)
    ("\\.ico\\'" . image-mode)
    ("\\.icon\\'" . image-mode)
    ("\\.jbg\\'" . image-mode)
    ("\\.jbig\\'" . image-mode)
    ("\\.jng\\'" . image-mode)
    ("\\.jnx\\'" . image-mode)
    ("\\.miff\\'" . image-mode)
    ("\\.mng\\'" . image-mode)
    ("\\.mvg\\'" . image-mode)
    ("\\.otb\\'" . image-mode)
    ("\\.p7\\'" . image-mode)
    ("\\.pcx\\'" . image-mode)
    ("\\.pdb\\'" . image-mode)
    ("\\.pfa\\'" . image-mode)
    ("\\.pfb\\'" . image-mode)
    ("\\.picon\\'" . image-mode)
    ("\\.pict\\'" . image-mode)
    ("\\.rgb\\'" . image-mode)
    ("\\.rgba\\'" . image-mode)
    ("\\.six\\'" . image-mode)
    ("\\.tga\\'" . image-mode)
    ("\\.wbmp\\'" . image-mode)
    ("\\.wmf\\'" . image-mode)
    ("\\.wpg\\'" . image-mode)
    ("\\.xcf\\'" . image-mode)
    ("\\.xmp\\'" . image-mode)
    ("\\.xwd\\'" . image-mode)
    ("\\.yuv\\'" . image-mode))
  "Alist of file name patterns vs corresponding major mode functions.
Each element looks like (REGEXP . FUNCTION) or (REGEXP FUNCTION NON-NIL).
\(NON-NIL stands for anything that is not nil; the value does not matter.)
Visiting a file whose name matches REGEXP specifies FUNCTION as the
mode function to use.  FUNCTION will be called, unless it is nil.

If the element has the form (REGEXP FUNCTION NON-NIL), then after
calling FUNCTION (if it's not nil), we delete the suffix that matched
REGEXP and search the list again for another match.

The extensions whose FUNCTION is `archive-mode' should also
appear in `auto-coding-alist' with `no-conversion' coding system.

See also `interpreter-mode-alist', which detects executable script modes
based on the interpreters they specify to run,
and `magic-mode-alist', which determines modes based on file contents.")

;;;; minibuffer.el

(defun completion-table-dynamic (fun &optional switch-buffer)
  "Use function FUN as a dynamic completion table.
FUN is called with one argument, the string for which completion is requested,
and it should return a completion table containing all the intended possible
completions.
This table is allowed to include elements that do not actually match the
string: they will be automatically filtered out.
The completion table returned by FUN can use any of the usual formats of
completion tables such as lists, alists, and hash-tables.

If SWITCH-BUFFER is non-nil and completion is performed in the
minibuffer, FUN will be called in the buffer from which the minibuffer
was entered.

The result of the `completion-table-dynamic' form is a function
that can be used as the COLLECTION argument to `try-completion' and
`all-completions'.  See Info node `(elisp)Programmed Completion'.
The completion table returned by `completion-table-dynamic' has empty
metadata and trivial boundaries.

See also the related function `completion-table-with-cache'."
  (lambda (string pred action)
    (if (or (eq (car-safe action) 'boundaries) (eq action 'metadata))
        ;; `fun' is not supposed to return another function but a plain old
        ;; completion table, whose boundaries are always trivial.
        nil
      (with-current-buffer (if (not switch-buffer) (current-buffer)
                             (let ((win (minibuffer-selected-window)))
                               (if (window-live-p win) (window-buffer win)
                                 (current-buffer))))
        (complete-with-action action (funcall fun string) string pred)))))

;;;; font-core.el

(defvar-local font-lock-defaults nil
  "Defaults for Font Lock mode specified by the major mode.
Defaults should be of the form:

 (KEYWORDS [KEYWORDS-ONLY [CASE-FOLD [SYNTAX-ALIST ...]]])

KEYWORDS may be a symbol (a variable or function whose value is the keywords
to use for fontification) or a list of symbols (specifying different levels
of fontification).

If KEYWORDS-ONLY is non-nil, syntactic fontification (strings and
comments) is not performed.

If CASE-FOLD is non-nil, the case of the keywords is ignored when fontifying.

If SYNTAX-ALIST is non-nil, it should be a list of cons pairs of the form
\(CHAR-OR-STRING . STRING) used to modify the syntax table used during Font
Lock  (see `modify-syntax-entry').
For performance reasons, if `syntax-ppss-table' is not set, make sure the
changes described by SYNTAX-ALIST cannot affect syntactic fontification.

These item elements are used by Font Lock mode to set the variables
`font-lock-keywords', `font-lock-keywords-only',
`font-lock-keywords-case-fold-search', `font-lock-syntax-table'.

Further item elements are alists of the form (VARIABLE . VALUE) and are in no
particular order.  Each VARIABLE is made buffer-local before set to VALUE.

Currently, appropriate variables include `font-lock-mark-block-function'.
If this is non-nil, it should be a function with no args used to mark any
enclosing block of text, for fontification via \\[font-lock-fontify-block].
Typical values are `mark-defun' for programming modes or `mark-paragraph' for
textual modes (i.e., the mode-dependent function is known to put point and mark
around a text block relevant to that mode).

Other variables include that for syntactic keyword fontification,
`font-lock-syntactic-keywords' and those for buffer-specialized fontification
functions, `font-lock-fontify-buffer-function',
`font-lock-unfontify-buffer-function', `font-lock-fontify-region-function',
`font-lock-unfontify-region-function'.")

(put 'font-lock-defaults 'risky-local-variable t)

;;;; syntax.el

(defvar syntax-wholeline-max 10000
  "Maximum line length for syntax operations.
If lines are longer than that, syntax operations will treat them as chunks
of this size.  Misfontification may then occur.
This is a tradeoff between correctly applying the syntax rules,
and avoiding major slowdown on pathologically long lines.")

(defun syntax--lbp (&optional arg)
  "Like `line-beginning-position' but obeying `syntax-wholeline-max'."
  (let ((pos (point))
        (res (line-beginning-position arg)))
    (cond
     ((< (abs (- pos res)) syntax-wholeline-max) res)
     ;; For lines that are too long, round to the nearest multiple of
     ;; `syntax-wholeline-max'.  We use rounding rather than just
     ;; (min res (+ pos syntax-wholeline-max)) so that repeated calls
     ;; to `syntax-propertize-wholelines' don't keep growing the bounds,
     ;; i.e. it really behaves like additional line-breaks.
     ((< res pos)
      (let ((max syntax-wholeline-max))
        (max (point-min) (* max (truncate pos max)))))
     (t
      (let ((max syntax-wholeline-max))
        (min (point-max) (* max (ceiling pos max))))))))

(defun syntax-propertize-wholelines (beg end)
  "Extend the region delimited by BEG and END to whole lines.
This function is useful for
`syntax-propertize-extend-region-functions';
see Info node `(elisp) Syntax Properties'."
  ;; This let-binding was taken from
  ;; `font-lock-extend-region-wholelines' where it was used to avoid
  ;; inf-looping (Bug#21615) but for some reason it was not applied
  ;; here in syntax.el and was used only for the "beg" side.
  (let ((inhibit-field-text-motion t))
    (let ((new-beg (progn (goto-char beg)
                          (if (bolp) beg
                            (syntax--lbp))))
          (new-end (progn (goto-char end)
                          (if (bolp) end
                            (syntax--lbp 2)))))
      (unless (and (eql beg new-beg) (eql end new-end))
        (cons new-beg new-end)))))

(defun syntax-propertize-multiline (beg end)
  "Let `syntax-propertize' pay attention to the syntax-multiline property."
  (when (and (> beg (point-min))
	     (get-text-property (1- beg) 'syntax-multiline))
    (setq beg (or (previous-single-property-change beg 'syntax-multiline)
		  (point-min))))
  ;;
  (when (get-text-property end 'syntax-multiline)
    (setq end (or (text-property-any end (point-max)
				     'syntax-multiline nil)
		  (point-max))))
  (cons beg end))

(defun syntax-propertize--shift-groups-and-backrefs (re n)
  (let ((new-re (replace-regexp-in-string
                 "\\\\(\\?\\([0-9]+\\):"
                 (lambda (s)
                   (replace-match
                    (number-to-string
                     (+ n (string-to-number (match-string 1 s))))
                    t t s 1))
                 re t t))
        (pos 0))
    (while (string-match "\\\\\\([0-9]+\\)" new-re pos)
      (setq pos (+ 1 (match-beginning 1)))
      (when (save-match-data
              ;; With \N, the \ must be in a subregexp context, i.e.,
              ;; not in a character class or in a \{\} repetition.
              (subregexp-context-p new-re (match-beginning 0)))
        (let ((shifted (+ n (string-to-number (match-string 1 new-re)))))
          (when (> shifted 9)
            (error "There may be at most nine back-references"))
          (setq new-re (replace-match (number-to-string shifted)
                                      t t new-re 1)))))
    new-re))

(defmacro syntax-propertize-precompile-rules (&rest rules)
  "Return a precompiled form of RULES to pass to `syntax-propertize-rules'.
The arg RULES can be of the same form as in `syntax-propertize-rules'.
The return value is an object that can be passed as a rule to
`syntax-propertize-rules'.
I.e. this is useful only when you want to share rules among several
`syntax-propertize-function's."
  (declare (debug syntax-propertize-rules))
  ;; Precompile?  Yeah, right!
  ;; Seriously, tho, this is a macro for 2 reasons:
  ;; - we could indeed do some pre-compilation at some point in the future,
  ;;   e.g. fi/when we switch to a DFA-based implementation of
  ;;   syntax-propertize-rules.
  ;; - this lets Edebug properly annotate the expressions inside RULES.
  `',rules)

(defmacro syntax-propertize-rules (&rest rules)
  "Make a function that applies RULES for use in `syntax-propertize-function'.
The function will scan the buffer, applying the rules where they match.
The buffer is scanned a single time, like \"lex\" would, rather than once
per rule.

Each RULE can be a symbol, in which case that symbol's value should be,
at macro-expansion time, a precompiled set of rules, as returned
by `syntax-propertize-precompile-rules'.

Otherwise, RULE should have the form (REGEXP HIGHLIGHT1 ... HIGHLIGHTn), where
REGEXP is an expression (evaluated at time of macro-expansion) that returns
a regexp, and where HIGHLIGHTs have the form (NUMBER SYNTAX) which means to
apply the property SYNTAX to the chars matched by the subgroup NUMBER
of the regular expression, if NUMBER did match.
SYNTAX is an expression that returns a value to apply as `syntax-table'
property.  Some expressions are handled specially:
- if SYNTAX is a string, then it is converted with `string-to-syntax';
- if SYNTAX has the form (prog1 EXP . EXPS) then the value returned by EXP
  will be applied to the buffer before running EXPS and if EXP is a string it
  is also converted with `string-to-syntax'.
The SYNTAX expression is responsible to save the `match-data' if needed
for subsequent HIGHLIGHTs.
Also SYNTAX is free to move point, in which case RULES may not be applied to
some parts of the text or may be applied several times to other parts.

Note: There may be at most nine back-references in the REGEXPs of
all RULES in total."
  (declare
   (debug (&rest &or symbolp            ;FIXME: edebug this eval step.
                 (def-form ;; `def-' needed to debug during macroexpansion.
                  &rest (numberp
                         [&or stringp   ;FIXME: Use &wrap
                              ;; `def-' because this is the body of a function.
                              ("prog1" [&or stringp def-form] def-body)
                              def-form])))))
  (let ((newrules nil))
    (while rules
      (if (symbolp (car rules))
          (setq rules (append (symbol-value (pop rules)) rules))
        (push (pop rules) newrules)))
    (setq rules (nreverse newrules)))
  (let* ((offset 0)
         (branches '())
         ;; We'd like to use a real DFA-based lexer, usually, but since Emacs
         ;; doesn't have one yet, we fallback on building one large regexp
         ;; and use groups to determine which branch of the regexp matched.
         (re
          (mapconcat
           (lambda (rule)
             (let* ((orig-re (eval (car rule) t))
                    (re orig-re))
               (when (and (assq 0 rule) (cdr rules))
                 ;; If there's more than 1 rule, and the rule want to apply
                 ;; highlight to match 0, create an extra group to be able to
                 ;; tell when *this* match 0 has succeeded.
                 (incf offset)
                 (setq re (concat "\\(" re "\\)")))
               (setq re (syntax-propertize--shift-groups-and-backrefs re offset))
               (let ((code '())
                     (condition
                      (cond
                       ((assq 0 rule) (if (zerop offset) t
                                        `(match-beginning ,offset)))
                       ((and (cdr rule) (null (cddr rule)))
                        `(match-beginning ,(+ offset (car (cadr rule)))))
                       (t
                        `(or ,@(mapcar
                                (lambda (case)
                                  `(match-beginning ,(+ offset (car case))))
                                (cdr rule))))))
                     (nocode t)
                     (offset offset))
                 ;; If some of the subgroup rules include Elisp code, then we
                 ;; need to set the match-data so it's consistent with what the
                 ;; code expects.  If not, then we can simply use shifted
                 ;; offset in our own code.
                 (unless (zerop offset)
                   (dolist (case (cdr rule))
                     (unless (stringp (cadr case))
                       (setq nocode nil)))
                   (unless nocode
                     (push `(let ((md (match-data 'ints)))
                              ;; Keep match 0 as is, but shift everything else.
                              (setcdr (cdr md) (nthcdr ,(* (1+ offset) 2) md))
                              (set-match-data md))
                           code)
                     (setq offset 0)))
                 ;; Now construct the code for each subgroup rules.
                 (dolist (case (cdr rule))
                   (cl-assert (null (cddr case)))
                   (let* ((gn (+ offset (car case)))
                          (action (nth 1 case))
                          (thiscode
                           (cond
                            ((stringp action)
                             `((put-text-property
                                (match-beginning ,gn) (match-end ,gn)
                                'syntax-table
                                ',(string-to-syntax action))))
                            ((eq (car-safe action) 'ignore)
                             (cdr action))
                            ((eq (car-safe action) 'prog1)
                             (if (stringp (nth 1 action))
                                 `((put-text-property
                                    (match-beginning ,gn) (match-end ,gn)
                                    'syntax-table
                                    ',(string-to-syntax (nth 1 action)))
                                   ,@(nthcdr 2 action))
                               `((let ((mb (match-beginning ,gn))
                                       (me (match-end ,gn)))
                                   ,(macroexp-let2 nil syntax (nth 1 action)
                                      `(progn
                                         (if ,syntax
                                             (put-text-property
                                              mb me 'syntax-table ,syntax))
                                         ,@(nthcdr 2 action)))))))
                            (t
                             `((let ((mb (match-beginning ,gn))
                                     (me (match-end ,gn))
                                     (syntax ,action))
                                 (if syntax
                                     (put-text-property
                                      mb me 'syntax-table syntax))))))))

                     (if (or (not (cddr rule)) (zerop gn))
                         (setq code (nconc (nreverse thiscode) code))
                       (push `(if (match-beginning ,gn)
                                  ;; Try and generate clean code with no
                                  ;; extraneous progn.
                                  ,(if (null (cdr thiscode))
                                       (car thiscode)
                                     `(progn ,@thiscode)))
                             code))))
                 (push (cons condition (nreverse code))
                       branches))
               (incf offset (regexp-opt-depth orig-re))
               re))
           rules
           "\\|")))
    `(lambda (start end)
       (goto-char start)
       (while (and (< (point) end)
                   (re-search-forward ,re end t))
         (cond ,@(nreverse branches))))))

;;;; easy-mmode.el

(defmacro define-globalized-minor-mode (global-mode mode turn-on &rest body)
  "Make a global mode GLOBAL-MODE corresponding to buffer-local minor MODE.
TURN-ON is a function that will be called with no args in every buffer
and that should try to turn MODE on if applicable for that buffer.

Each of KEY VALUE is a pair of CL-style keyword arguments.
The :predicate key specifies in which major modes should the
globalized minor mode be switched on.  The value should be t (meaning
switch on the minor mode in all major modes), nil (meaning don't
switch on in any major mode), a list of modes (meaning switch on only
in those modes and their descendants), or a list (not MODES...),
meaning switch on in any major mode except MODES.  The value can also
mix all of these forms, see the Info node `(elisp)Defining Minor Modes'
for details.  The :predicate key causes the macro to create a user option
named the same as MODE, but ending with \"-modes\" instead of \"-mode\".
That user option can then be used to customize in which modes this
globalized minor mode will be switched on.
As the minor mode defined by this function is always global, any
:global keyword is ignored.
Other keywords have the same meaning as in `define-minor-mode',
which see.  In particular, :group specifies the custom group.
The most useful keywords are those that are passed on to the `defcustom'.
It normally makes no sense to pass the :lighter or :keymap keywords
to `define-globalized-minor-mode', since these are usually passed to
the buffer-local version of the minor mode.

BODY contains code to execute each time the mode is enabled or disabled.
It is executed after toggling the mode, and before running
GLOBAL-MODE-hook.

If MODE's set-up depends on the major mode in effect when it was
enabled, then disabling and reenabling MODE should make MODE work
correctly with the current major mode.  This is important to
prevent problems with derived modes, that is, major modes that
call another major mode in their body.

When a major mode is initialized, MODE is actually turned on just
after running the major mode's hook.  However, MODE is not turned
on if the hook has explicitly disabled it.

\(fn GLOBAL-MODE MODE TURN-ON [KEY VALUE]... BODY...)"
  (declare (doc-string 2) (indent defun)
           ;; expand to the minor-mode definition on autoload gen
           (autoload-macro expand))
  (let* ((global-mode-name (symbol-name global-mode))
	 (mode-name (symbol-name mode))
	 (pretty-name (easy-mmode-pretty-mode-name mode))
	 (pretty-global-name (easy-mmode-pretty-mode-name global-mode))
	 (group nil)
	 (extra-keywords nil)
         (MODE-variable mode)
	 (MODE-enable-in-buffer
	  (intern (concat global-mode-name "-enable-in-buffer")))
	 (minor-MODE-hook (intern (concat mode-name "-hook")))
	 (MODE-set-explicitly (intern (concat mode-name "--set-explicitly")))
         (MODE-suppress-set-explicitly (intern (concat mode-name
                                                       "--suppress-set-explicitly")))
         (MODE-predicate (intern (concat (replace-regexp-in-string
                                          "-mode\\'" "" global-mode-name)
                                         "-modes")))
         (turn-on-function `#',turn-on)
	 keyw predicate)

    ;; Check keys.
    (while (keywordp (setq keyw (car body)))
      (pop body)
      (pcase keyw
        (:group (setq group (nconc group (list :group (pop body)))))
        (:global (pop body))
        (:variable (setq MODE-variable (pop body)))
        (:predicate
         (setq predicate (list (pop body)))
         (setq turn-on-function
               `(lambda ()
                  (require 'easy-mmode)
                  (when (easy-mmode--globalized-predicate-p ,MODE-predicate)
                    (funcall ,turn-on-function)))))
        (_ (push keyw extra-keywords) (push (pop body) extra-keywords))))
    (setq extra-keywords (nreverse extra-keywords))

    (when (and (plist-get extra-keywords :init-value)
               (null (plist-get extra-keywords :initialize)))
      (setq extra-keywords `(:initialize #'custom-initialize-after-file-load
                             . ,extra-keywords)))

    `(progn
       (progn
         (put ',global-mode 'globalized-minor-mode t)
         :autoload-end
         ,@(when predicate `((defvar ,MODE-predicate))))
       ;; The actual global minor-mode
       (define-minor-mode ,global-mode
         ,(concat (format "Toggle %s in many buffers.\n" pretty-name)
                  (internal--format-docstring-line
                   "Specifically, %s is enabled in all buffers where `%S' would do it."
                   pretty-name turn-on)
                  "\n\n"
                  (internal--format-docstring-line
                   (concat "With prefix ARG, enable %s if ARG is positive; "
                           "otherwise, disable it.")
                   pretty-global-name)
                  "\n\n"
                  "If called from Lisp, toggle the mode if ARG is `toggle'.
Enable the mode if ARG is nil, omitted, or is a positive number.
Disable the mode if ARG is a negative number.\n\n"
                  (internal--format-docstring-line
                   "See `%s' for more information on %s."
                   mode pretty-name)
                  (if predicate
                      (concat
                       "\n\n"
                       (internal--format-docstring-line
                        "`%s' is used to control which modes this minor mode is used in."
                        MODE-predicate))
                    ""))
         :global t ,@group ,@extra-keywords

	 ;; Setup hook to handle future mode changes and new buffers.
	 (if ,global-mode
	     (add-hook 'after-change-major-mode-hook
		       #',MODE-enable-in-buffer)
	   (remove-hook 'after-change-major-mode-hook #',MODE-enable-in-buffer))

	 ;; Go through existing buffers.
	 (dolist (buf (buffer-list))
	   (with-current-buffer buf
             (if ,global-mode (funcall ,turn-on-function)
               (when (bound-and-true-p ,MODE-variable) (,mode -1)))))
         ,@body)

       ,(when predicate
          `(defcustom ,MODE-predicate ,(car predicate)
             ,(format "Which major modes `%s' is switched on in.
This variable can be either t (all major modes), nil (no major modes),
or a list of modes and (not modes) to switch use this minor mode or
not.  For instance

  (c-mode (not message-mode mail-mode) text-mode)

means \"use this mode in all modes derived from `c-mode', don't use in
modes derived from `message-mode' or `mail-mode', but do use in other
modes derived from `text-mode'\".  An element with value t means \"use\"
and nil means \"don't use\".  There's an implicit nil at the end of the
list."
                      mode)
             :type '(choice
                     (const :tag "Enable in all major modes" t)
                     (const :tag "Don't enable in any major mode" nil)
                     (repeat
                      :tag "Rules (earlier takes precedence)..."
                      (choice
                       (const :tag "Enable in all (other) modes" t)
                       (const :tag "Don't enable in any (other) mode" nil)
                       (symbol :value fundamental-mode
                               :tag "Enable in major mode")
                       (cons :tag "Don't enable in major modes"
                             (const :tag "Don't enable in..." not)
                             (repeat (symbol :value fundamental-mode
                                             :tag "Major mode"))))))
             ,@group))

       ;; Autoloading define-globalized-minor-mode autoloads everything
       ;; up-to-here.
       :autoload-end

       ;; MODE-set-explicitly is set in MODE-set-explicitly and cleared by
       ;; kill-all-local-variables.
       (defvar-local ,MODE-set-explicitly nil)
       (defvar ,MODE-suppress-set-explicitly nil)
       (defun ,MODE-set-explicitly ()
         (unless ,MODE-suppress-set-explicitly
           (setq ,MODE-set-explicitly t)))
       (put ',MODE-set-explicitly 'definition-name ',global-mode)

       ;; A function which checks whether MODE has been disabled in the major
       ;; mode hook which has just been run.
       (add-hook ',minor-MODE-hook #',MODE-set-explicitly)

       ;; The function that calls TURN-ON in the current buffer.
       (defun ,MODE-enable-in-buffer ()
         (unless ,MODE-set-explicitly
           (let (;; We are not part of the major mode hook so we don't
                 ;; want to set MODE-set-explicitly to t.
                 ;; In particular this is necessary when there are
                 ;; multiple globalized versions of a single minor mode.
                 ;; If one of them declines to turn the minor mode on,
                 ;; that should not mean the others can't.
                 (,MODE-suppress-set-explicitly t))
             (funcall ,turn-on-function))))
       (put ',MODE-enable-in-buffer 'definition-name ',global-mode))))

(defun easy-mmode--globalized-predicate-p (predicate)
  (cond
   ((eq predicate t)
    t)
   ((eq predicate nil)
    nil)
   ((listp predicate)
    ;; Legacy support for (not a b c).
    (when (eq (car predicate) 'not)
      (setq predicate (nconc (mapcar (lambda (e) (list 'not e))
                                     (cdr predicate))
                             (list t))))
    (catch 'found
      (dolist (elem predicate)
        (cond
         ((eq elem t)
          (throw 'found t))
         ((eq elem nil)
          (throw 'found nil))
         ((and (consp elem)
               (eq (car elem) 'not))
          (when (derived-mode-p (cdr elem))
            (throw 'found nil)))
         ((symbolp elem)
          (when (derived-mode-p elem)
            (throw 'found t)))))))))


;;;; startup.el

(defcustom initial-major-mode 'lisp-interaction-mode
  "Major mode command symbol to use for the initial `*scratch*' buffer."
  :type 'function)
