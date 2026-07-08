//! Elisp ASCII logo + live-stats box banner. Single source of truth shared by:
//!   - REPL startup (`repl::run` in the `elisp` binary)
//!   - `elisp --help` output (the logo rows come from `LOGO_ROWS` here)
//!
//! Every count is pulled at call time — special-form / subr tables via `.len()`
//! and the live builtin count via `with_host` — so the banner never goes stale
//! after `cargo build` adds forms or subrs. Memory / core / os / arch / pid rows
//! come from `sysinfo` + `std`. ANSI colors are emitted only when `colored` is
//! set; the plain path is byte-clean for width tests and non-tty output.

/// The six ELISP block-glyph rows (no color, no leading space). The banner and
/// `elisp --help` both render these, so the logo literal lives in exactly one
/// place. Rows 0-1 render cyan, 2-3 magenta, 4-5 red (see `logo_colored`).
pub const LOGO_ROWS: [&str; 6] = [
    "███████╗██╗     ██╗███████╗██████╗ ██████╗ ███████╗",
    "██╔════╝██║     ██║██╔════╝██╔══██╗██╔══██╗██╔════╝",
    "█████╗  ██║     ██║███████╗██████╔╝██████╔╝███████╗",
    "██╔══╝  ██║     ██║╚════██║██╔═══╝ ██╔══██╗╚════██║",
    "███████╗███████╗██║███████║██║     ██║  ██║███████║",
    "╚══════╝╚══════╝╚═╝╚══════╝╚═╝     ╚═╝  ╚═╝╚══════╝",
];

/// Per-row logo colors: cyan, cyan, magenta, magenta, red, red.
const LOGO_COLORS: [&str; 6] = [
    "\x1b[36m", "\x1b[36m", "\x1b[35m", "\x1b[35m", "\x1b[31m", "\x1b[31m",
];

/// The six logo rows joined with a leading space and (optionally) per-row color.
/// Shared by `render_banner` and `elisp --help` so the glyphs are never
/// duplicated as a literal.
pub fn logo_colored(colored: bool) -> String {
    let mut out = String::with_capacity(512);
    for (i, row) in LOGO_ROWS.iter().enumerate() {
        if colored {
            out.push_str(&format!("{} {row}\x1b[0m\n", LOGO_COLORS[i]));
        } else {
            out.push_str(&format!(" {row}\n"));
        }
    }
    out
}

/// Count of visible columns in `s`, ignoring ANSI SGR escape sequences.
/// Multi-byte UTF-8 is counted as one column per char — sufficient for the
/// box-drawing glyphs and Latin labels in the banner; East-Asian-Wide chars
/// would need a wcwidth-style lookup that we deliberately skip.
pub fn visible_width(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut w = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0x1B && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                i += 1;
            }
            i += 1;
        } else {
            let step = std::str::from_utf8(&bytes[i..])
                .ok()
                .and_then(|s| s.chars().next())
                .map(|c| c.len_utf8())
                .unwrap_or(1);
            w += 1;
            i += step;
        }
    }
    w
}

/// Render the elisp ASCII logo + live-stats box + tagline into a string.
/// `colored=true` emits ANSI SGR escapes; `false` returns plain text.
pub fn render_banner(colored: bool) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let n_special = crate::lsp::SPECIAL_FORMS.len();
    let n_subrs = crate::lsp::SUBRS.len();
    let n_builtins = crate::with_host(|h| h.builtin_count());

    let (mem_total_gib, mem_avail_gib) = {
        use sysinfo::System;
        let mut sys = System::new();
        sys.refresh_memory();
        let total = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let avail = sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        (total, avail)
    };

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let pid = std::process::id();

    let (c, m, r, y, g, n) = if colored {
        (
            "\x1b[36m", "\x1b[35m", "\x1b[31m", "\x1b[33m", "\x1b[32m", "\x1b[0m",
        )
    } else {
        ("", "", "", "", "", "")
    };
    let _ = r;

    const INNER: usize = 64;
    let mut out = String::with_capacity(2048);

    let row = |out: &mut String, body: &str| {
        let pad = INNER.saturating_sub(visible_width(body));
        out.push_str(&format!("{c} │{n}{body}{:pad$}{c}│{n}\n", "", pad = pad));
    };

    out.push_str(&logo_colored(colored));
    out.push_str(&format!(
        "{c} ┌────────────────────────────────────────────────────────────────┐{n}\n"
    ));
    row(
        &mut out,
        &format!(
            " {y}SYSTEM{n}  status:{g} ONLINE {c}//{n} {y}os:{n} {os} {y}arch:{n} {arch} {y}pid:{n} {pid}"
        ),
    );
    row(
        &mut out,
        &format!(
            " {y}CORES{n}   {cores}    {y}MEM{n}  {mem_avail_gib:.1} {c}/{n} {mem_total_gib:.1} GiB available"
        ),
    );
    out.push_str(&format!(
        "{c} ├────────────────────────────────────────────────────────────────┤{n}\n"
    ));
    row(
        &mut out,
        &format!(
            " {y}special-forms{n} {n_special:<4} {y}subrs{n} {n_subrs:<4} {y}builtins{n} {n_builtins:<4}"
        ),
    );
    out.push_str(&format!(
        "{c} └────────────────────────────────────────────────────────────────┘{n}\n"
    ));
    out.push_str(&format!(
        "{m}  >> EMACS LISP ON FUSEVM // FULL SPECTRUM // v{version} <<{n}\n"
    ));
    out
}

/// Print the banner to stdout. Convenience wrapper around `render_banner`.
pub fn print_banner(colored: bool) {
    print!("{}", render_banner(colored));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_width_ignores_csi_sequences() {
        assert_eq!(visible_width("\x1b[31mabc\x1b[0m"), 3);
        assert_eq!(visible_width("\x1b[1;38;5;202mok"), 2);
    }

    #[test]
    fn visible_width_counts_each_char_once_for_multibyte() {
        // 3 box-drawing glyphs, each 3 bytes UTF-8, but one column each.
        assert_eq!(visible_width("─├┤"), 3);
        assert_eq!(visible_width("aé你"), 3);
    }

    #[test]
    fn visible_width_handles_empty_and_lone_escape() {
        assert_eq!(visible_width(""), 0);
        // Lone ESC with no `[` does not start a CSI; counts as 1 char.
        assert_eq!(visible_width("\x1bz"), 2);
    }

    #[test]
    fn render_banner_plain_has_no_ansi_escapes() {
        let s = render_banner(false);
        assert!(!s.contains('\x1b'), "plain banner must not contain ESC");
        assert!(s.contains("EMACS LISP ON FUSEVM"));
        assert!(s.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn render_banner_colored_contains_ansi_escapes() {
        let s = render_banner(true);
        assert!(s.contains("\x1b["));
        assert!(s.contains("\x1b[0m"));
    }

    #[test]
    fn logo_rows_are_all_the_same_visible_width() {
        // The block-glyph logo must be a clean rectangle; a stray glyph would
        // skew `--help` alignment. All six rows share one column count.
        let want = visible_width(LOGO_ROWS[0]);
        for r in LOGO_ROWS {
            assert_eq!(visible_width(r), want, "logo row width drift: {r}");
        }
    }

    #[test]
    fn render_banner_rows_all_match_inner_width_after_strip() {
        // Anchor expected width to the top border, then prove every interior
        // row matches it. Catches drift in `row()` padding even if the box
        // size is retuned later.
        let s = render_banner(false);
        let top = s
            .lines()
            .find(|l| l.starts_with(" ┌"))
            .expect("top border present");
        let want = visible_width(top);
        let mut box_rows = 0;
        for line in s.lines() {
            if line.starts_with(" │") && line.ends_with('│') {
                box_rows += 1;
                assert_eq!(
                    visible_width(line),
                    want,
                    "box row width drift on line: {line}"
                );
            }
        }
        assert!(box_rows >= 3, "expected several rendered box rows");
    }
}
