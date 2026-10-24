//! fileio.c's file primitives over relative names, `find-file-name-handler`,
//! dired.c's `file-attributes`, the link and mode primitives, and files.el's
//! `file-truename`, `file-chase-links`, `delete-file` and `delete-directory`.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`), run over
//! the same symbolic-link fixture.

use elisprs::{eval_str, print, reset_host};
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::PathBuf;

/// Evaluate SRC in a fresh host; a signalled error comes back as `(ERR ERROR)`.
fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

/// A fresh fixture directory (its true name, links resolved):
///
/// ```text
/// real/sub/f  real/f.gz  link -> real  real/flink -> sub/f
/// real/sub/up -> ../real  loop1 <-> loop2  dangling -> /nonexistent/x
/// dbl -> real//sub  rel -> ../NAME/link
/// ```
fn fixture(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("elisprs-truename-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("real/sub")).unwrap();
    let base = base.canonicalize().unwrap();
    std::fs::write(base.join("real/sub/f"), "").unwrap();
    std::fs::write(base.join("real/f.gz"), "").unwrap();
    symlink("real", base.join("link")).unwrap();
    symlink("sub/f", base.join("real/flink")).unwrap();
    symlink("../real", base.join("real/sub/up")).unwrap();
    symlink("loop1", base.join("loop2")).unwrap();
    symlink("loop2", base.join("loop1")).unwrap();
    symlink("/nonexistent/x", base.join("dangling")).unwrap();
    symlink("real//sub", base.join("dbl")).unwrap();
    let dir = base.file_name().unwrap().to_string_lossy().into_owned();
    symlink(format!("../{dir}/link"), base.join("rel")).unwrap();
    base
}

/// Evaluate FORM with `default-directory` the fixture D, answering its
/// printed value with D replaced by "D".
fn in_dir(d: &PathBuf, form: &str) -> String {
    let d = d.to_string_lossy();
    eval(&format!("(let ((default-directory \"{d}/\")) {form})")).replace(&*d, "D")
}

/// A file predicate's relative name is expanded against `default-directory`
/// (each fileio.c primitive starts with `Fexpand_file_name`), and a quoted
/// "/:" name is the file it quotes.
#[test]
fn file_predicates_expand_against_default_directory() {
    let d = fixture("predicates");
    assert_eq!(
        in_dir(&d, "(list (file-exists-p \"link\") (file-directory-p \"link\") (file-regular-p \"real/sub/f\") (file-readable-p \"real/sub/f\") (file-writable-p \"real/newfile\") (file-symlink-p \"link\") (file-executable-p \"real\") (file-exists-p \"\") (file-directory-p \"\"))"),
        "(t t t t t \"real\" t t t)"
    );
    let q = d.to_string_lossy();
    assert_eq!(
        eval(&format!("(list (file-symlink-p \"/:{q}/link\") (file-exists-p \"/:{q}/link\") (file-exists-p \"/:\") (file-writable-p \"/:{q}/real/new\"))")),
        "(\"real\" t t t)"
    );
    assert_eq!(eval("(file-symlink-p 1)"), "(ERR (wrong-type-argument stringp 1))");
}

/// files.el's `file-truename`: links chased at every level, `.` and `..`
/// after the directory's truename, a dangling link answers its target, a
/// cycle is an error.
#[test]
fn file_truename_chases_every_level() {
    let d = fixture("truename");
    assert_eq!(
        in_dir(&d, "(list (file-truename \"link/sub/f\") (file-truename \"link/flink\") (file-truename \"link/sub/up/sub/..\") (file-truename \"link/.\") (file-truename \"dangling\") (file-truename \"nonexist/a/b\") (file-truename \"\") (file-truename \"link/f.gz\") (file-truename \"rel\") (file-truename \"dbl\") (file-truename \"link/sub/\") (file-truename \"/\"))"),
        "(\"D/real/sub/f\" \"D/real/sub/f\" \"D/real/real\" \"D/real\" \"/nonexistent/x\" \"D/nonexist/a/b\" \"D\" \"D/real/f.gz\" \"D/real\" \"D/real/sub\" \"D/real/sub/\" \"/\")"
    );
    assert_eq!(
        in_dir(&d, "(condition-case e (file-truename \"loop1\") (error (car e)))"),
        "error"
    );
    assert_eq!(eval("(file-truename 'x)"), "(ERR (wrong-type-argument arrayp x))");
    // A quoted name goes through `file-name-non-special', which quotes the result.
    let q = d.to_string_lossy();
    assert_eq!(
        eval(&format!("(file-truename \"/:{q}/link\")")).replace(&*q, "D"),
        "\"/:D/real\""
    );
    assert_eq!(eval("(file-truename \"/:\")"), "\"/:/\"");
}

/// files.el's `file-chase-links`: only the last component, LIMIT links at
/// most, and a name without a directory part reaches
/// `find-file-name-handler' with nil (as in Emacs).
#[test]
fn file_chase_links_follows_the_last_component() {
    let d = fixture("chase");
    assert_eq!(
        in_dir(&d, "(list (file-chase-links \"link/flink\") (file-chase-links \"rel\") (file-chase-links \"rel\" 1) (file-chase-links \"real/sub/up\") (file-chase-links \"real\"))"),
        "(\"link/sub/f\" \"D/real\" \"D/link\" \"D/real/real\" \"real\")"
    );
    assert_eq!(
        in_dir(&d, "(file-chase-links \"link\")"),
        "(ERR (wrong-type-argument stringp nil))"
    );
}

/// fileio.c `Ffind_file_name_handler`: the match furthest into the name
/// wins, a handler's `operations' property limits it, inhibited handlers
/// are skipped for `inhibit-file-name-operation', the match data survives.
#[test]
fn find_file_name_handler_follows_fileio_c() {
    assert_eq!(
        eval("(list (find-file-name-handler \"/tmp/a.gz\" 'load) (find-file-name-handler \"/tmp/a.gz\" 'file-truename) (find-file-name-handler \"/tmp/a.gpg\" 'file-truename) (find-file-name-handler \"/:/x\" 'file-truename) (find-file-name-handler \"/ssh:h:/x.gz\" 'load) (find-file-name-handler \"/ssh:h:/x\" 'file-truename))"),
        "(jka-compr-handler nil nil file-name-non-special jka-compr-handler tramp-autoload-file-name-handler)"
    );
    assert_eq!(
        eval("(list (let ((inhibit-file-name-handlers '(jka-compr-handler)) (inhibit-file-name-operation 'load)) (find-file-name-handler \"/ssh:h:/x.gz\" 'load)) (let ((file-name-handler-alist '(bad (\"x\" . \"str\") (\"z\" . bar)))) (find-file-name-handler \"/xz\" nil)) (let ((file-name-handler-alist '((\"A\" . foo))) (case-fold-search t)) (find-file-name-handler \"/a\" nil)) (progn (string-match \"b\" \"abc\") (find-file-name-handler \"/a.gz\" 'load) (match-beginning 0)))"),
        "(tramp-autoload-file-name-handler bar nil 1)"
    );
    assert_eq!(
        eval("(find-file-name-handler nil nil)"),
        "(ERR (wrong-type-argument stringp nil))"
    );
    assert_eq!(
        eval("(list (unhandled-file-name-directory \"/tmp/x\") (unhandled-file-name-directory \"/:/tmp\"))"),
        "(\"/tmp/x/\" \"/tmp/\")"
    );
}

/// dired.c `Ffile_attributes`: `lstat`, the type slot, ids as names on
/// request, `current-time-list' timestamps, the `ls -l' mode string; a
/// missing file or a non-string answers nil.
#[test]
fn file_attributes_reads_lstat() {
    let d = fixture("attributes");
    let st = std::fs::symlink_metadata(d.join("link")).unwrap();
    let t = |s: i64, ns: i64| format!("({} {} {} {})", s >> 16, s & 0xffff, ns / 1000, ns % 1000 * 1000);
    let expected = format!(
        "(\"real\" {} {} {} {} {} {} 4 \"lrwxr-xr-x\" t {} {})",
        st.nlink(),
        st.uid(),
        st.gid(),
        t(st.atime(), st.atime_nsec()),
        t(st.mtime(), st.mtime_nsec()),
        t(st.ctime(), st.ctime_nsec()),
        st.ino(),
        st.dev()
    );
    assert_eq!(in_dir(&d, "(file-attributes \"link\")"), expected);
    assert_eq!(
        in_dir(&d, "(let ((a (file-attributes \"real/sub/f\"))) (list (file-attribute-type a) (file-attribute-size a) (file-attribute-link-number a) (stringp (file-attribute-modes a)) (file-attribute-collect a 'type 'size)))"),
        "(nil 0 1 t (nil 0))"
    );
    assert_eq!(
        in_dir(&d, "(list (car (file-attributes \"real\")) (car (file-attributes \"dangling\")) (file-attributes \"nope\") (file-attributes \"real/sub/f/x\") (file-attributes 1) (equal (file-attributes \"real\") (file-attributes \"real\" 'integer)))"),
        "(t \"/nonexistent/x\" nil nil nil t)"
    );
    assert_eq!(
        in_dir(&d, "(list (stringp (nth 2 (file-attributes \"real\" 'string))) (equal (nth 2 (file-attributes \"real\" 'string)) (user-login-name (nth 2 (file-attributes \"real\")))))"),
        "(t t)"
    );
}

/// `file-modes`, `set-file-modes`, `file-newer-than-file-p` and
/// `file-accessible-directory-p`.
#[test]
fn file_modes_and_times() {
    let d = fixture("modes");
    std::fs::set_permissions(d.join("real/sub/f"), std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
    std::fs::File::options().write(true).open(d.join("real/sub/f")).unwrap().set_modified(old).unwrap();
    assert_eq!(
        in_dir(&d, "(list (file-modes \"real/sub/f\") (progn (set-file-modes \"real/sub/f\" #o600) (file-modes \"real/sub/f\")) (file-modes \"nope\") (file-modes \"real/sub/f/x\"))"),
        "(420 384 nil nil)"
    );
    assert_eq!(
        in_dir(&d, "(list (set-file-modes \"nope\" 420) (set-file-modes \"real/sub/f\" \"x\"))"),
        "(ERR (file-missing \"Doing chmod\" \"No such file or directory\" \"D/nope\"))"
    );
    assert_eq!(
        in_dir(&d, "(set-file-modes \"real/sub/f\" \"x\")"),
        "(ERR (wrong-type-argument fixnump \"x\"))"
    );
    assert_eq!(
        in_dir(&d, "(list (file-newer-than-file-p \"nope\" \"real\") (file-newer-than-file-p \"real\" \"nope\") (file-newer-than-file-p \"nope\" \"nope2\") (file-newer-than-file-p \"real\" \"real/sub/f\") (file-newer-than-file-p \"real/sub/f\" \"real\"))"),
        "(nil t nil t nil)"
    );
    assert_eq!(
        in_dir(&d, "(list (file-accessible-directory-p \"real/sub/f\") (file-accessible-directory-p \"nope\") (file-accessible-directory-p \"\") (file-accessible-directory-p \"link\") (file-accessible-directory-p \"link/\"))"),
        "(nil nil t t t)"
    );
}

/// fileio.c `Fmake_symbolic_link` and `Fadd_name_to_file`: the target is
/// stored as written, a directory name receives the file's last component,
/// an existing name needs OK-IF-ALREADY-EXISTS.
#[test]
fn make_symbolic_link_and_add_name_to_file() {
    let d = fixture("links");
    std::fs::create_dir(d.join("dir")).unwrap();
    assert_eq!(
        in_dir(&d, "(list (progn (make-symbolic-link \"~/x\" \"l1\") (file-symlink-p \"l1\")) (progn (make-symbolic-link \"/:/foo\" \"l2\") (file-symlink-p \"l2\")) (progn (make-symbolic-link \"a/b\" \"dir/\") (directory-files \"dir\")) (condition-case e (make-symbolic-link \"q\" \"l1\") (error e)) (progn (make-symbolic-link \"q\" \"l1\" t) (file-symlink-p \"l1\")) (condition-case e (make-symbolic-link \"q\" \"nodir/l\") (error e)))"),
        "(\"~/x\" \"/:/foo\" (\".\" \"..\" \"b\") (file-already-exists \"File already exists\" \"D/l1\") \"q\" (file-missing \"Making symbolic link\" \"No such file or directory\" \"q\" \"D/nodir/l\"))"
    );
    assert_eq!(
        in_dir(&d, "(list (progn (write-region \"x\" nil \"f\") (add-name-to-file \"f\" \"g\") (file-attribute-link-number (file-attributes \"f\"))) (condition-case e (add-name-to-file \"f\" \"g\") (error e)) (condition-case e (add-name-to-file \"nope\" \"h\") (error e)) (progn (add-name-to-file \"f\" \"dir/\") (directory-files \"dir\")))"),
        "(2 (file-already-exists \"File already exists\" \"D/g\") (file-missing \"Adding new name\" \"No such file or directory\" \"D/nope\" \"D/h\") (\".\" \"..\" \"b\" \"f\"))"
    );
}

/// files.el's `delete-file` (a directory is refused) and `delete-directory`
/// (RECURSIVE removes the tree without following links in it).
#[test]
fn delete_file_and_delete_directory() {
    let d = fixture("delete");
    assert_eq!(
        in_dir(&d, "(list (progn (make-directory \"nd/a/b\" t) (write-region \"x\" nil \"nd/a/f\") (make-symbolic-link \"../../real\" \"nd/a/l\") (delete-directory \"nd\" t) (file-exists-p \"nd\")) (file-exists-p \"real/sub/f\") (delete-directory \"nope\" t) (delete-file \"nope\") (progn (delete-file \"dangling\") (file-symlink-p \"dangling\")) (subrp (symbol-function 'delete-file)))"),
        "(nil t nil nil nil nil)"
    );
    assert_eq!(
        in_dir(&d, "(delete-file \"real\")"),
        "(ERR (file-error \"Removing old name: is a directory\" \"real\"))"
    );
    assert_eq!(
        in_dir(&d, "(delete-directory \"nope\")"),
        "(ERR (file-missing \"Removing directory\" \"No such file or directory\" \"D/nope\"))"
    );
    assert_eq!(
        in_dir(&d, "(progn (make-directory \"nd2\") (write-region \"x\" nil \"nd2/f\") (condition-case e (delete-directory \"nd2\") (error (list (car e) (cadr e)))))"),
        "(file-error \"Removing directory\")"
    );
}

/// editfns.c's `user-login-name` UID decoding, which `file-attributes`'
/// names are checked against: a natural, an integral float, or `(HI . LO)`
/// / `(HI LO)` as HI * 2^16 + LO; anything else is a plain `error`.
#[test]
fn user_login_name_decodes_uid() {
    let uid = unsafe { libc::geteuid() };
    assert_eq!(eval("(list (= (user-uid) (user-real-uid)) (equal (user-login-name 0) (user-login-name 0.0)) (user-login-name 4000000000))"), "(t t nil)");
    assert_eq!(
        eval(&format!("(list (equal (user-login-name {uid}) (user-real-login-name)) (equal (user-login-name '(0 . {uid})) (user-login-name '(0 {uid}))))")),
        "(t t)"
    );
    let bad = "(ERR (error \"Not an in-range integer, integral float, or cons of integers\"))";
    for arg in ["-1", "1.5", "\"x\"", "'(0 . 70000)", "'(0 . 1.0)", "(expt 2 40)"] {
        assert_eq!(eval(&format!("(user-login-name {arg})")), bad, "{arg}");
    }
    assert_eq!(eval("(user-real-login-name 1)"), "(ERR (wrong-number-of-arguments user-real-login-name 1))");
}
