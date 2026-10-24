;;; files-31.el --- file name handlers, truenames, deletion, attributes  -*- lexical-binding: t -*-

;; fileio.c's handler lookup and variables, ported, then verbatim
;; definitions from GNU Emacs 31.1 lisp/files.el in file order:
;; `file-truename', `file-chase-links', `delete-file', `delete-directory',
;; `file-name-non-special' and the `file-attribute-*' accessors.  The file
;; primitives themselves (src/builtins.rs) do not dispatch through
;; `file-name-handler-alist'; a quoted name "/:..." reaches them as the
;; file it quotes.  These functions dispatch as in Emacs.

;;;; fileio.c

(defvar inhibit-file-name-handlers nil
  "A list of file name handlers that temporarily should not be used.
This applies only to the operation `inhibit-file-name-operation'.")

(defvar inhibit-file-name-operation nil
  "The operation for which `inhibit-file-name-handlers' is applicable.")

;; The `operations' properties epa-file.el and jka-cmpr-hook.el put on
;; their handlers: `find-file-name-handler' offers them nothing else.
(put 'epa-file-handler 'operations '(write-region insert-file-contents))
(put 'jka-compr-handler 'operations
     '(byte-compiler-base-file-name write-region insert-file-contents
       file-local-copy load))

(defun find-file-name-handler (filename operation)
  "Return FILENAME's handler function for OPERATION, if it has one.
Otherwise, return nil.
A file name is handled if one of the regular expressions in
`file-name-handler-alist' matches it.

If OPERATION equals `inhibit-file-name-operation', then ignore
any handlers that are members of `inhibit-file-name-handlers',
but still do run any other handlers.  This lets handlers
use the standard functions without calling themselves recursively."
  ;; Ffind_file_name_handler: of the handlers whose regexp matches (by
  ;; `fast_string_match', case-sensitive and leaving the match data alone)
  ;; and whose `operations' property, if any, lists OPERATION, the one
  ;; matching furthest into FILENAME wins; the first such on a tie.
  (unless (stringp filename)
    (signal 'wrong-type-argument (list 'stringp filename)))
  (let ((inhibited (and (eq operation inhibit-file-name-operation)
                        inhibit-file-name-handlers))
        (pos -1)
        result)
    (dolist (elt file-name-handler-alist)
      (when (consp elt)
        (let* ((regexp (car elt))
               (handler (cdr elt))
               (operations (and (symbolp handler) (get handler 'operations)))
               (match-pos (and (stringp regexp)
                               (let ((case-fold-search nil))
                                 (save-match-data
                                   (string-match regexp filename))))))
          (when (and match-pos (> match-pos pos)
                     (or (null operations) (memq operation operations))
                     (not (memq handler inhibited)))
            (setq result handler
                  pos match-pos)))))
    result))

(defun unhandled-file-name-directory (filename)
  "Return a directly usable directory name somehow associated with FILENAME.
A `directly usable' directory name is one that may be used without the
intervention of any file name handler.
If FILENAME is a directly usable file itself, return
\(file-name-as-directory FILENAME).
If FILENAME refers to a file which is not accessible from a local process,
then this should return nil.
The `call-process' and `start-process' functions use this function to
get a current directory to run processes in."
  ;; Funhandled_file_name_directory.
  (let ((handler (find-file-name-handler filename
                                         'unhandled-file-name-directory)))
    (if handler
        (let ((handled (funcall handler 'unhandled-file-name-directory
                                filename)))
          (and (stringp handled) handled))
      (file-name-as-directory filename))))

(defvar delete-by-moving-to-trash nil
  "Specifies whether to use the system's trash can.
When non-nil, certain file deletion commands use the function
`move-file-to-trash' instead of deleting files outright.
This includes interactive calls to `delete-file' and
`delete-directory' and the Dired deletion commands.")

;; tramp-loaddefs.el's `tramp-mode', which `file-name-non-special' binds.
(defvar tramp-mode t
  "Whether Tramp is enabled.
If it is set to nil, all remote file names are used literally.")

;;;; files.el

(defun files--name-absolute-system-p (file)
  "Return non-nil if FILE is an absolute name to the operating system.
This is like `file-name-absolute-p', except that it returns nil for
names beginning with `~'."
  (and (file-name-absolute-p file)
       (not (eq (aref file 0) ?~))))

(defun files--splice-dirname-file (dirname file)
  "Splice DIRNAME to FILE like the operating system would.
If FILE is relative, return DIRNAME concatenated to FILE.
Otherwise return FILE, quoted as needed if DIRNAME and FILE have
different file name handlers; although this quoting is dubious if
DIRNAME is magic, it is not clear what would be better.  This
function differs from `expand-file-name' in that DIRNAME must be
a directory name and leading `~' and `/:' are not special in
FILE."
  (let ((unquoted (if (files--name-absolute-system-p file)
		      file
		    (concat dirname file))))
    (if (eq (find-file-name-handler dirname 'file-symlink-p)
	    (find-file-name-handler unquoted 'file-symlink-p))
	unquoted
      (let (file-name-handler-alist) (file-name-quote unquoted)))))

(defun file-truename (filename &optional counter prev-dirs)
  "Return the truename of FILENAME.
If FILENAME is not absolute, first expands it against `default-directory'.
The truename of a file name is found by chasing symbolic links
both at the level of the file and at the level of the directories
containing it, until no links are left at any level.

\(fn FILENAME)"  ;; Don't document the optional arguments.
  ;; COUNTER and PREV-DIRS are used only in recursive calls.
  ;; COUNTER can be a cons cell whose car is the count of how many
  ;; more links to chase before getting an error.
  ;; PREV-DIRS can be a cons cell whose car is an alist
  ;; of truenames we've just recently computed.
  (cond ((or (string= filename "") (string= filename "~"))
	 (setq filename (expand-file-name filename))
	 (if (string= filename "")
	     (setq filename "/")))
	((and (string= (substring filename 0 1) "~")
	      (string-match "~[^/]*/?" filename))
	 (let ((first-part
		(substring filename 0 (match-end 0)))
	       (rest (substring filename (match-end 0))))
	   (setq filename (concat (expand-file-name first-part) rest)))))

  (or counter (setq counter (list 100)))
  (let (done
	;; For speed, remove the ange-ftp completion handler from the list.
	;; We know it's not needed here.
	;; For even more speed, do this only on the outermost call.
	(file-name-handler-alist
	 (if prev-dirs file-name-handler-alist
	   (let ((tem (copy-sequence file-name-handler-alist)))
	     (delq (rassq 'ange-ftp-completion-hook-function tem) tem)))))
    (or prev-dirs (setq prev-dirs (list nil)))

    ;; andrewi@harlequin.co.uk - on Windows, there is an issue with
    ;; case differences being ignored by the OS, and short "8.3 DOS"
    ;; name aliases existing for all files.  (The short names are not
    ;; reported by directory-files, but can be used to refer to files.)
    ;; It seems appropriate for file-truename to resolve these issues in
    ;; the most natural way, which on Windows is to call the function
    ;; `w32-long-file-name' - this returns the exact name of a file as
    ;; it is stored on disk (expanding short name aliases with the full
    ;; name in the process).
    (if (eq system-type 'windows-nt)
	(unless (string-match "[[*?]" filename)
	  ;; If filename exists, use its long name.  If it doesn't
	  ;; exist, the recursion below on the directory of filename
	  ;; will drill down until we find a directory that exists,
	  ;; and use the long name of that, with the extra
	  ;; non-existent path components concatenated.
	  (let ((longname (w32-long-file-name filename)))
	    (if longname
		(setq filename longname)))))

    ;; If this file directly leads to a link, process that iteratively
    ;; so that we don't use lots of stack.
    (while (not done)
      (setcar counter (1- (car counter)))
      (if (< (car counter) 0)
	  (error "Apparent cycle of symbolic links for %s" filename))
      (let ((handler (find-file-name-handler filename 'file-truename)))
	;; For file name that has a special handler, call handler.
	;; This is so that ange-ftp can save time by doing a no-op.
	(if handler
	    (setq filename (funcall handler 'file-truename filename)
		  done t)
	  (let ((dir (or (file-name-directory filename) default-directory))
		target dirfile)
	    ;; Get the truename of the directory.
	    (setq dirfile (directory-file-name dir))
	    ;; If these are equal, we have the (or a) root directory.
	    (or (string= dir dirfile)
		(and (file-name-case-insensitive-p dir)
		     (string-equal-ignore-case dir dirfile))
		;; If this is the same dir we last got the truename for,
		;; save time--don't recalculate.
		(if (assoc dir (car prev-dirs))
		    (setq dir (cdr (assoc dir (car prev-dirs))))
		  (let ((old dir)
			(new (file-name-as-directory (file-truename dirfile counter prev-dirs))))
		    (setcar prev-dirs (cons (cons old new) (car prev-dirs)))
		    (setq dir new))))
            (let ((filename-no-dir (file-name-nondirectory filename)))
	      (if (equal ".." filename-no-dir)
		  (setq filename
		        (directory-file-name (file-name-directory (directory-file-name dir)))
		        done t)
	        (if (equal "." filename-no-dir)
		    (setq filename (directory-file-name dir)
			  done t)
		  ;; Put it back on the file name.
		  (setq filename (concat dir filename-no-dir))
		  ;; Is the file name the name of a link?
		  (setq target (file-symlink-p filename))
		  (if target
		      ;; Yes => chase that link, then start all over
		      ;; since the link may point to a directory name that uses links.
		      ;; We can't safely use expand-file-name here
		      ;; since target might look like foo/../bar where foo
		      ;; is itself a link.  Instead, we handle . and .. above.
		      (setq filename (files--splice-dirname-file dir target)
			    done nil)
		    ;; No, we are done!
		    (setq done t)))))))))
    filename))

(defun file-chase-links (filename &optional limit)
  "Chase links in FILENAME until a name that is not a link.
Unlike `file-truename', this does not check whether a parent
directory name is a symbolic link.
If the optional argument LIMIT is a number,
it means chase no more than that many links and then stop."
  (let (tem (newname filename)
	    (count 0))
    (while (and (or (null limit) (< count limit))
		(setq tem (file-symlink-p newname)))
      (save-match-data
	(if (and (null limit) (= count 100))
	    (error "Apparent cycle of symbolic links for %s" filename))
	;; In the context of a link, `//' doesn't mean what Emacs thinks.
	(while (string-match "//+" tem)
	  (setq tem (replace-match "/" nil nil tem)))
	;; Handle `..' by hand, since it needs to work in the
	;; target of any directory symlink.
	;; This code is not quite complete; it does not handle
	;; embedded .. in some cases such as ./../foo and foo/bar/../../../lose.
	(while (string-match "\\`\\.\\./" tem)
	  (setq tem (substring tem 3))
	  (setq newname (expand-file-name newname))
	  ;; Chase links in the default dir of the symlink.
	  (setq newname
		(file-chase-links
		 (directory-file-name (file-name-directory newname))))
	  ;; Now find the parent of that dir.
	  (setq newname (file-name-directory newname)))
	(setq newname (files--splice-dirname-file (file-name-directory newname)
						  tem))
	(setq count (1+ count))))
    newname))


(defconst directory-files-no-dot-files-regexp
  "[^.]\\|\\.\\.\\."
  "Regexp matching any file name except \".\" and \"..\".
More precisely, it matches parts of any nonempty string except those two.
It is useful as the regexp argument to `directory-files' and
`directory-files-and-attributes'.")

(defun files--force (no-such fn &rest args)
  "Use NO-SUCH to affect behavior of function FN applied to list ARGS.
This acts like (apply FN ARGS) except it returns NO-SUCH if it is
non-nil and if FN fails due to a missing file or directory."
  (condition-case err
      (apply fn args)
    (file-missing (or no-such (signal err)))))

(defun delete-file (filename &optional trash)
  "Delete file named FILENAME.  If it is a symlink, remove the symlink.
If file has multiple names, it continues to exist with the other names.
TRASH non-nil means to trash the file instead of deleting, provided
`delete-by-moving-to-trash' is non-nil.

When called interactively, TRASH is t if no prefix argument is given.
With a prefix argument, TRASH is nil."
  (interactive (list (read-file-name
                      (if (and delete-by-moving-to-trash (null current-prefix-arg))
                          "Move file to trash: " "Delete file: ")
                      nil default-directory (confirm-nonexistent-file-or-buffer))
                     (null current-prefix-arg)))
  (if (and (file-directory-p filename) (not (file-symlink-p filename)))
      (signal 'file-error (list "Removing old name: is a directory" filename)))
  (let* ((filename (expand-file-name filename))
         (handler (find-file-name-handler filename 'delete-file)))
    (cond (handler (funcall handler 'delete-file filename trash))
          ((and delete-by-moving-to-trash trash) (move-file-to-trash filename))
          (t (delete-file-internal filename)))))

(defun delete-directory (directory &optional recursive trash)
  "Delete the directory named DIRECTORY.  Does not follow symlinks.
If RECURSIVE is non-nil, delete files in DIRECTORY as well, with
no error if something else is simultaneously deleting them.
TRASH non-nil means to trash the directory instead, provided
`delete-by-moving-to-trash' is non-nil.

When called interactively, TRASH is nil if and only if a prefix
argument is given, and a further prompt asks the user for
RECURSIVE if DIRECTORY is nonempty."
  (interactive
   (let* ((trashing (and delete-by-moving-to-trash
			 (null current-prefix-arg)))
	  (dir (expand-file-name
		(read-directory-name
		 (if trashing
		     "Move directory to trash: "
		   "Delete directory: ")
		 default-directory default-directory nil nil))))
     (list dir
	   (if (directory-files	dir nil directory-files-no-dot-files-regexp)
	       (y-or-n-p
		(format-message "Directory `%s' is not empty, really %s? "
                                dir (if trashing "trash" "delete")))
	     nil)
	   (null current-prefix-arg))))
  ;; If default-directory is a remote directory, make sure we find its
  ;; delete-directory handler.
  (setq directory (directory-file-name (expand-file-name directory)))
  (let ((handler (find-file-name-handler directory 'delete-directory)))
    (cond
     (handler
      (funcall handler 'delete-directory directory recursive trash))
     ((and delete-by-moving-to-trash trash)
      ;; Move non-empty dir to trash only if recursive deletion was
      ;; requested.  This mimics the non-`delete-by-moving-to-trash'
      ;; case, where the operation fails in delete-directory-internal.
      ;; As `move-file-to-trash' trashes directories (empty or
      ;; otherwise) as a unit, we do not need to recurse here.
      (if (not (or recursive (directory-empty-p directory)))
	  (error "Directory is not empty, not moving to trash")
	(move-file-to-trash directory)))
     ;; Otherwise, call ourselves recursively if needed.
     (t
      (when (or (not recursive) (file-symlink-p directory)
		(let* ((files
			(files--force t #'directory-files directory 'full
				      directory-files-no-dot-files-regexp))
		       (directory-exists (listp files)))
		  (when directory-exists
		    (mapc (lambda (file)
			    ;; This test is equivalent to but more efficient
			    ;; than (and (file-directory-p fn)
			    ;;		 (not (file-symlink-p fn))).
			    (if (eq t (car (file-attributes file)))
				(delete-directory file recursive)
			      (files--force t #'delete-file file)))
			  files))
		  directory-exists))
	(files--force recursive #'delete-directory-internal directory))))))


(defun file-name-non-special (operation &rest arguments)
  (let ((inhibit-file-name-handlers
         (cons 'file-name-non-special
               (and (eq inhibit-file-name-operation operation)
                    inhibit-file-name-handlers)))
        (inhibit-file-name-operation operation)
        ;; Some operations respect file name handlers in
        ;; `default-directory'.  Because core function like
        ;; `call-process' don't care about file name handlers in
        ;; `default-directory', we here have to resolve the directory
        ;; into a local one.  For `process-file',
        ;; `start-file-process', and `shell-command', this fixes
        ;; Bug#25949.
        (default-directory
	  (if (memq operation
                    '(insert-directory process-file start-file-process
                                       make-process shell-command
                                       temporary-file-directory))
	      (directory-file-name
	       (expand-file-name
		(unhandled-file-name-directory default-directory)))
	    default-directory))
	;; Get a list of the indices of the args that are file names.
	(file-arg-indices
	 (cdr (or (assq operation
			'(;; The first seven are special because they
			  ;; return a file name.  We want to include
			  ;; the /: in the return value.  So just
			  ;; avoid stripping it in the first place.
                          (abbreviate-file-name)
                          (directory-file-name)
                          (file-name-as-directory)
                          (file-name-directory)
                          (file-name-sans-versions)
                          (file-remote-p)
                          (find-backup-file-name)
	                  ;; `identity' means just return the first
			  ;; arg not stripped of its quoting.
			  (substitute-in-file-name identity)
                          ;; `expand-file-name' shall do special case
                          ;; for the first argument starting with
                          ;; "/:~".  (Bug#65685)
                          (expand-file-name expand-file-name)
			  ;; `add' means add "/:" to the result.
			  (file-truename add 0)
                          ;;`insert-file-contents' needs special handling.
			  (insert-file-contents insert-file-contents 0)
			  ;; `unquote-then-quote' means set buffer-file-name
			  ;; temporarily to unquoted filename.
			  (verify-visited-file-modtime unquote-then-quote)
                          ;; Unquote `buffer-file-name' temporarily.
                          (make-auto-save-file-name buffer-file-name)
                          (set-visited-file-modtime buffer-file-name)
                          ;; Use a temporary local copy.
			  (copy-file local-copy)
			  (rename-file local-copy)
                          (copy-directory local-copy)
			  ;; List the arguments that are filenames.
			  (file-name-completion 0 1)
			  (file-name-all-completions 0 1)
                          (file-equal-p 0 1)
                          (file-newer-than-file-p 0 1)
			  (write-region 2 5)
			  (file-in-directory-p 0 1)
			  (make-symbolic-link 0 1)
			  (add-name-to-file 0 1)
                          ;; These file-notify-* operations take a
                          ;; descriptor.
                          (file-notify-rm-watch)
                          (file-notify-valid-p)
                          ;; `make-process' uses keyword arguments and
                          ;; doesn't mangle its filenames in any way.
                          ;; It already strips /: from the binary
                          ;; filename, so we don't have to do this
                          ;; here.
                          (make-process)))
		  ;; For all other operations, treat the first
		  ;; argument only as the file name.
		  '(nil 0))))
	method
	;; Copy ARGUMENTS so we can replace elements in it.
	(arguments (copy-sequence arguments)))
    (if (symbolp (car file-arg-indices))
	(setq method (pop file-arg-indices)))
    ;; Strip off the /: from the file names that have it.
    (save-match-data                    ;FIXME: Why?
      (while (consp file-arg-indices)
	(let ((pair (nthcdr (car file-arg-indices) arguments)))
	  (when (car pair)
	    (setcar pair (file-name-unquote (car pair) t))))
	(setq file-arg-indices (cdr file-arg-indices))))
    ;; In general, we don't want any file name handler, see Bug#47625,
    ;; Bug#48349.  For some few cases, operations with two file name
    ;; arguments which might be bound to different file name handlers,
    ;; we still need this.
    (let ((tramp-mode (and tramp-mode (eq method 'local-copy))))
      (pcase method
        ('identity (car arguments))
        ('expand-file-name
         (when (string-prefix-p "/:~" (car arguments))
           (setcar arguments (file-name-unquote (car arguments) t)))
         (apply operation arguments))
        ('add (file-name-quote (apply operation arguments) t))
        ('buffer-file-name
         (let ((buffer-file-name (file-name-unquote buffer-file-name t)))
           (apply operation arguments)))
        ('insert-file-contents
         (let ((visit (nth 1 arguments)))
           (unwind-protect
               (apply operation arguments)
             (when (and visit buffer-file-name)
               (setq buffer-file-name (file-name-quote buffer-file-name t))))))
        ('unquote-then-quote
         ;; We can't use `cl-letf' with `(buffer-local-value)' here
         ;; because it wouldn't work during bootstrapping.
         (let ((buffer (current-buffer)))
           ;; `unquote-then-quote' is used only for the
           ;; `verify-visited-file-modtime' action, which takes a
           ;; buffer as only optional argument.
           (with-current-buffer (or (car arguments) buffer)
             (let ((buffer-file-name (file-name-unquote buffer-file-name t)))
               ;; Make sure to hide the temporary buffer change from
               ;; the underlying operation.
               (with-current-buffer buffer
                 (apply operation arguments))))))
        ('local-copy
         (let ((source (car arguments))
               (target (car (cdr arguments)))
               (prefix (expand-file-name
                        "file-name-non-special" temporary-file-directory))
               tmpfile)
           (cond
            ;; If source is remote, we must create a local copy.
            ((file-remote-p source)
             (setq tmpfile (make-temp-name prefix))
             (apply operation source tmpfile (cddr arguments))
             (setq source tmpfile))
            ;; If source is quoted, and the unquoted source looks
            ;; remote, we must create a local copy.
            ((file-name-quoted-p source t)
             (setq source (file-name-unquote source t))
             (when (file-remote-p source)
               (setq tmpfile (make-temp-name prefix))
               (let (file-name-handler-alist)
                 (apply operation source tmpfile (cddr arguments)))
               (setq source tmpfile))))
           ;; If target is quoted, and the unquoted target looks
           ;; remote, we must disable the file name handler.
           (when (file-name-quoted-p target t)
             (setq target (file-name-unquote target t))
             (when (file-remote-p target)
               (setq file-name-handler-alist nil)))
           ;; Do it.
           (setcar arguments source)
           (setcar (cdr arguments) target)
           (apply operation arguments)
           ;; Cleanup.
           (when (and tmpfile (file-exists-p tmpfile))
             (if (file-directory-p tmpfile)
                 (delete-directory tmpfile 'recursive) (delete-file tmpfile)))))
        (_
         (apply operation arguments))))))


(defsubst file-attribute-type (attributes)
  "The type field in ATTRIBUTES returned by `file-attributes'.
The value is either t for directory, string (name linked to) for
symbolic link, or nil."
  (nth 0 attributes))

(defsubst file-attribute-link-number (attributes)
  "Return the number of links in ATTRIBUTES returned by `file-attributes'."
  (nth 1 attributes))

(defsubst file-attribute-user-id (attributes)
  "The UID field in ATTRIBUTES returned by `file-attributes'.
This is either a string or a number.  If a string value cannot be
looked up, a numeric value, either an integer or a float, is
returned."
  (nth 2 attributes))

(defsubst file-attribute-group-id (attributes)
  "The GID field in ATTRIBUTES returned by `file-attributes'.
This is either a string or a number.  If a string value cannot be
looked up, a numeric value, either an integer or a float, is
returned."
  (nth 3 attributes))

(defsubst file-attribute-access-time (attributes)
  "The last access time in ATTRIBUTES returned by `file-attributes'.
This a Lisp timestamp in the style of `current-time'."
  (nth 4 attributes))

(defsubst file-attribute-modification-time (attributes)
  "The modification time in ATTRIBUTES returned by `file-attributes'.
This is the time of the last change to the file's contents, and
is a Lisp timestamp in the style of `current-time'."
  (nth 5 attributes))

(defsubst file-attribute-status-change-time (attributes)
  "The status modification time in ATTRIBUTES returned by `file-attributes'.
This is the time of last change to the file's attributes: owner
and group, access mode bits, etc., and is a Lisp timestamp in the
style of `current-time'."
  (nth 6 attributes))

(defsubst file-attribute-size (attributes)
  "The integer size (in bytes) in ATTRIBUTES returned by `file-attributes'."
  (nth 7 attributes))

(defsubst file-attribute-modes (attributes)
  "The file modes in ATTRIBUTES returned by `file-attributes'.
This is a string of ten letters or dashes as in ls -l."
  (nth 8 attributes))

(defsubst file-attribute-inode-number (attributes)
  "The inode number in ATTRIBUTES returned by `file-attributes'.
It is a nonnegative integer."
  (nth 10 attributes))

(defsubst file-attribute-device-number (attributes)
  "The file system device number in ATTRIBUTES returned by `file-attributes'.
It is an integer or a cons cell of integers."
  (nth 11 attributes))

(defsubst file-attribute-file-identifier (attributes)
  "The inode and device numbers in ATTRIBUTES returned by `file-attributes'.
The value is a list of the form (INODENUM DEVICE), where DEVICE could be
either a single number or a cons cell of two numbers.
This tuple of numbers uniquely identifies the file."
  (nthcdr 10 attributes))

(defun file-attribute-collect (attributes &rest attr-names)
  "Return a sublist of ATTRIBUTES returned by `file-attributes'.
ATTR-NAMES are symbols with the selected attribute names.

Valid attribute names are: type, link-number, user-id, group-id,
access-time, modification-time, status-change-time, size, modes,
inode-number, device-number and file-number."
  (let ((all '(type link-number user-id group-id access-time
               modification-time status-change-time
               size modes inode-number device-number file-number))
        result)
    (while attr-names
      (let ((attr (pop attr-names)))
        (if (memq attr all)
            (push (funcall
                   (intern (format "file-attribute-%s" (symbol-name attr)))
                   attributes)
                  result)
          (error "Wrong attribute name '%S'" attr))))
    (nreverse result)))

