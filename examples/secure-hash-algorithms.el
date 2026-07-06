;;; secure-hash-algorithms.el --- secure-hash SHA-2 family vs Emacs 30.2  -*- lexical-binding: nil; -*-

;; Differential-tested against real `emacs -Q --batch' 30.2.  Pins the
;; `secure-hash-algorithms' accessor (faithful to C `Fsecure_hash_algorithms',
;; a fixed 6-symbol list) and the three SHA-2 digests it advertised but that
;; `secure-hash' could not previously compute: sha224, sha384, sha512.  Digest
;; vectors are the NIST/FIPS-180 reference values for "abc" and the empty
;; string, byte-for-byte identical to the oracle.  files.el evaluates
;; `(secure-hash-algorithms)' at load time to build the
;; `auto-save-file-name-transforms' defcustom :type choice list; a void
;; function there aborted the whole file (and every library that requires it).
(message "== secure-hash SHA-2 family ==")

(ert-deftest secure-hash-algorithm-list ()
  "The accessor returns the fixed 6-symbol algorithm list, in C order."
  (should (equal (secure-hash-algorithms)
                 '(md5 sha1 sha224 sha256 sha384 sha512)))
  ;; Every advertised algorithm must be one `secure-hash' actually accepts:
  ;; the list may not over-promise.
  (dolist (algo (secure-hash-algorithms))
    (should (stringp (secure-hash algo "probe")))))

(ert-deftest secure-hash-sha224-values ()
  "sha224 matches FIPS-180 reference digests (28-byte output)."
  (should (equal (secure-hash 'sha224 "abc")
                 "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7"))
  (should (equal (secure-hash 'sha224 "")
                 "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f"))
  (should (= (length (secure-hash 'sha224 "")) 56)))

(ert-deftest secure-hash-sha384-values ()
  "sha384 matches FIPS-180 reference digests (48-byte output)."
  (should (equal (secure-hash 'sha384 "abc")
                 (concat "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded163"
                         "1a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7")))
  (should (equal (secure-hash 'sha384 "")
                 (concat "38b060a751ac96384cd9327eb1b1e36a21fdb71114be0743"
                         "4c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b")))
  (should (= (length (secure-hash 'sha384 "")) 96)))

(ert-deftest secure-hash-sha512-values ()
  "sha512 matches FIPS-180 reference digests (64-byte output)."
  (should (equal (secure-hash 'sha512 "abc")
                 (concat "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea2"
                         "0a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd"
                         "454d4423643ce80e2a9ac94fa54ca49f")))
  (should (equal (secure-hash 'sha512 "")
                 (concat "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc"
                         "83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f"
                         "63b931bd47417a81a538327af927da3e")))
  (should (= (length (secure-hash 'sha512 "")) 128)))

(ert-deftest secure-hash-start-end-region ()
  "START/END select a char sub-range before hashing, like the oracle."
  ;; (secure-hash 'sha256 \"aabcd\" 1 4) hashes \"abc\".
  (should (equal (secure-hash 'sha256 "aabcd" 1 4)
                 (secure-hash 'sha256 "abc")))
  ;; sha512 honors the same START/END path.
  (should (equal (secure-hash 'sha512 "zzabc" 2)
                 (secure-hash 'sha512 "abc"))))

(ert-deftest secure-hash-binary-form ()
  "Non-nil BINARY returns raw digest bytes, whose length is the byte count."
  (should (= (length (secure-hash 'sha224 "abc" nil nil t)) 28))
  (should (= (length (secure-hash 'sha384 "abc" nil nil t)) 48))
  (should (= (length (secure-hash 'sha512 "abc" nil nil t)) 64)))

(ert-run-tests-batch-and-exit)
