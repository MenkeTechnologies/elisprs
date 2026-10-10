;;; map-31.el --- map.el (Emacs 31.1) generic functions, dispatched by hand  -*- lexical-binding: t -*-

;; map.el defines these with `cl-defgeneric'; the methods for `list',
;; `hash-table' and `array' are reproduced here as one function per generic that
;; dispatches on the map's type, in the order cl-generic would pick them.  A
;; value no method accepts signals `cl-no-applicable-method' with the call's
;; arguments, as the generic's dispatcher does.

(define-error 'map-not-inplace "Cannot modify map in-place")

(defun map--no-method (name &rest args)
  (signal 'cl-no-applicable-method (cons name args)))

(defsubst map--plist-p (list)
  "Return non-nil if LIST is the start of a nonempty plist map."
  (and (consp list) (atom (car list))))

(defun map--plist-member (plist prop &optional predicate)
  (plist-member plist prop predicate))

(defun map--plist-put (plist prop val &optional predicate)
  (plist-put plist prop val predicate))

(defun mapp (map)
  "Return non-nil if MAP is a map (alist/plist, hash-table, array, ...)."
  (or (listp map)
      (hash-table-p map)
      (arrayp map)))

(defun map-contains-key (map key &optional testfn)
  "Return non-nil if and only if MAP contains KEY.
TESTFN is deprecated.  Its default depends on MAP."
  (cond
   ((listp map)
    (if (map--plist-p map)
        (map--plist-member map key testfn)
      (let ((v '(nil)))
        (not (eq v (alist-get key map v nil (or testfn #'equal)))))))
   ((hash-table-p map) (hash-table-contains-p key map))
   ((arrayp map) (and (natnump key) (< key (length map))))
   (t
    (unless testfn (setq testfn #'equal))
    (map-some (lambda (k _v) (funcall testfn key k)) map))))

(defun map-elt (map key &optional default testfn)
  "Look up KEY in MAP and return its associated value.
If KEY is not found, return DEFAULT which defaults to nil."
  (cond
   ((listp map)
    (if (map--plist-p map)
        (let ((res (map--plist-member map key testfn)))
          (if res (cadr res) default))
      (alist-get key map default nil (or testfn #'equal))))
   ((hash-table-p map) (gethash key map default))
   ((arrayp map)
    (if (map-contains-key map key)
        (aref map key)
      default))
   (t (apply #'map--no-method 'map-elt map key
             (and (or default testfn) (list default))))))

(defun map--plist-delete (map key)
  (let ((tail map) last)
    (while (consp tail)
      (cond
       ((not (eq key (car tail)))
        (setq last tail)
        (setq tail (cddr last)))
       (last
        (setq tail (cddr tail))
        (setf (cddr last) tail))
       (t
        (setq map (cddr map))
        (setq tail map))))
    map))

(defun map-delete (map key)
  "Delete KEY in-place from MAP and return MAP."
  (cond
   ((listp map)
    (if (map--plist-p map)
        (map--plist-delete map key)
      (setf (alist-get key map nil t #'equal) nil)
      map))
   ((hash-table-p map) (remhash key map) map)
   ((arrayp map)
    (when (map-contains-key map key)
      (aset map key nil))
    map)
   (t (map--no-method 'map-delete map key))))

(defun map-nested-elt (map keys &optional default)
  "Traverse MAP using KEYS and return the looked up value or DEFAULT if nil."
  (or (seq-reduce (lambda (acc key)
                    (when (mapp acc)
                      (map-elt acc key)))
                  keys
                  map)
      default))

(defun map-do (function map)
  "Apply FUNCTION to each element of MAP and return nil."
  (cond
   ((listp map)
    (if (map--plist-p map)
        (while map
          (funcall function (pop map) (pop map)))
      (mapc (lambda (pair)
              (funcall function (car pair) (cdr pair)))
            map)
      nil))
   ((hash-table-p map) (maphash function map))
   ((arrayp map)
    (seq-do-indexed (lambda (elt index)
                      (funcall function index elt))
                    map))
   (t (map--no-method 'map-do function map))))

(defun map-apply (function map)
  "Apply FUNCTION to each element of MAP and return the result as a list."
  (cond
   ((and (listp map) (not (map--plist-p map)))
    (mapcar (lambda (pair)
              (funcall function (car pair) (cdr pair)))
            map))
   ((hash-table-p map)
    (let (result)
      (maphash (lambda (key value)
                 (push (funcall function key value) result))
               map)
      (nreverse result)))
   ((and (arrayp map) (not (listp map)))
    (seq-map-indexed (lambda (elt index)
                       (funcall function index elt))
                     map))
   (t
    (let ((res '()))
      (map-do (lambda (k v) (push (funcall function k v) res)) map)
      (nreverse res)))))

(defun map-keys (map)
  "Return the list of keys in MAP."
  (map-apply (lambda (key _) key) map))

(defun map-values (map)
  "Return the list of values in MAP."
  (if (and (arrayp map) (not (listp map)))
      (append map ())
    (map-apply (lambda (_ value) value) map)))

(defun map-pairs (map)
  "Return the key/value pairs in MAP as an alist."
  (map-apply #'cons map))

(defun map-length (map)
  "Return the number of key/value pairs in MAP."
  (cond
   ((hash-table-p map) (hash-table-count map))
   ((listp map)
    (if (map--plist-p map)
        (/ (length map) 2)
      (length map)))
   ((arrayp map) (length map))
   (t
    (let ((size 0))
      (map-do (lambda (_k _v) (setq size (1+ size))) map)
      size))))

(defun map-copy (map)
  "Return a copy of MAP."
  (cond
   ((listp map)
    (if (map--plist-p map)
        (copy-sequence map)
      (copy-alist map)))
   ((hash-table-p map) (copy-hash-table map))
   ((arrayp map) (copy-sequence map))
   (t (map--no-method 'map-copy map))))

(defun map-keys-apply (function map)
  "Return the result of applying FUNCTION to each key in MAP."
  (map-apply (lambda (key _)
               (funcall function key))
             map))

(defun map-values-apply (function map)
  "Return the result of applying FUNCTION to the value of each key in MAP."
  (if (and (arrayp map) (not (listp map)))
      (mapcar function map)
    (map-apply (lambda (_ val)
                 (funcall function val))
               map)))

(defun map-filter (pred map)
  "Return an alist of key/val pairs for which (PRED key val) is non-nil in MAP."
  (delq nil (map-apply (lambda (key val)
                         (and (funcall pred key val)
                              (cons key val)))
                       map)))

(defun map-remove (pred map)
  "Return an alist of the key/val pairs for which (PRED key val) is nil in MAP."
  (map-filter (lambda (key val) (not (funcall pred key val)))
              map))

(defun map-empty-p (map)
  "Return non-nil if MAP is empty."
  (if (listp map)
      (null map)
    (zerop (map-length map))))

(defun map-some (pred map)
  "Return the first non-nil value from applying PRED to elements of MAP."
  (catch 'map--break
    (map-do (lambda (key value)
              (let ((result (funcall pred key value)))
                (when result
                  (throw 'map--break result))))
            map)
    nil))

(defun map-every-p (pred map)
  "Return non-nil if calling PRED on all elements of MAP returns non-nil."
  (catch 'map--break
    (map-do (lambda (key value)
              (or (funcall pred key value)
                  (throw 'map--break nil)))
            map)
    t))

(defun map--into-hash (map keyword-args)
  (let ((ht (apply #'make-hash-table keyword-args)))
    (map-do (lambda (key value)
              (puthash key value ht))
            map)
    ht))

(defun map-into (map type)
  "Convert MAP into a map of TYPE."
  (cond
   ((memq type '(list alist)) (map-pairs map))
   ((eq type 'plist)
    (let (plist)
      (map-do (lambda (k v) (setq plist `(,v ,k ,@plist))) map)
      (nreverse plist)))
   ((eq type 'hash-table)
    (map--into-hash map (list :size (map-length map) :test #'equal)))
   ((eq (car-safe type) 'hash-table)
    (map--into-hash map (cdr type)))
   (t (map--no-method 'map-into map type))))

(defun map--merge (merge type &rest maps)
  "Merge into a map of TYPE all the key/value pairs in MAPS."
  (let* ((tolist (memq type '(list alist plist)))
         (result (map-into (pop maps)
                           (cond ((eq type 'plist) '(hash-table :test eq))
                                 (tolist '(hash-table :test equal))
                                 (type)))))
    (dolist (map maps)
      (map-do (lambda (key value)
                (setq result (funcall merge result key value)))
              map))
    (if tolist (map-into result type) result)))

(defun map-merge (type &rest maps)
  "Merge into a map of TYPE all the key/value pairs in MAPS."
  (apply #'map--merge
         (lambda (result key value)
           (setf (map-elt result key) value)
           result)
         type maps))

(defun map-merge-with (type function &rest maps)
  "Merge into a map of TYPE all the key/value pairs in MAPS, combining
the values of a key found twice with FUNCTION."
  (let ((not-found (list nil)))
    (apply #'map--merge
           (lambda (result key value)
             (cl-callf (lambda (old)
                         (if (eql old not-found)
                             value
                           (funcall function old value)))
                 (map-elt result key not-found))
             result)
           type maps)))

(defun map-put! (map key value &optional testfn)
  "Associate KEY with VALUE in MAP, in place; signal `map-not-inplace' if
that is impossible."
  (cond
   ((listp map)
    (if (map--plist-p map)
        (map--plist-put map key value testfn)
      (let ((oldmap map))
        (setf (alist-get key map key nil (or testfn #'equal)) value)
        (unless (eq oldmap map)
          (signal 'map-not-inplace (list oldmap)))))
    value)
   ((hash-table-p map) (puthash key value map))
   ((arrayp map) (aset map key value))
   (t (map--no-method 'map-put! map key value))))

(defalias 'map--put #'map-put!)

(defun map-insert (map key value)
  "Return a new map like MAP except that it associates KEY with VALUE."
  (if (listp map)
      (if (map--plist-p map)
          (cons key (cons value map))
        (cons (cons key value) map))
    (let ((copy (map-copy map)))
      (map-put! copy key value)
      copy)))

(defmacro map-put (map key value &optional testfn)
  "Associate KEY with VALUE in MAP and return VALUE."
  (declare (obsolete "use `map-put!' or `(setf (map-elt ...) ...)' instead." "27.1"))
  (if testfn
      `(with-no-warnings
         (setf (map-elt ,map ,key nil ,testfn) ,value))
    `(setf (map-elt ,map ,key) ,value)))

(defun map--make-pcase-patterns (args)
  "Return a list of `(map ...)' pcase patterns built from ARGS."
  (cons 'map
        (mapcar (lambda (elt)
                  (if (eq (car-safe elt) 'map)
                      (map--make-pcase-patterns elt)
                    elt))
                args)))

(defmacro map-let (keys map &rest body)
  "Bind the variables in KEYS to the elements of MAP, then evaluate BODY."
  (declare (indent 2))
  `(pcase-let ((,(map--make-pcase-patterns keys) ,map))
     ,@body))

(provide 'map)
