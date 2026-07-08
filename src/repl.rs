//! Interactive REPL for `elisp` — utop-style line editor backed by `reedline`.
//!
//! Layout per turn:
//!
//! ```text
//! ─( HH:MM:SS )──< command N >──────────────────────────────{ elisp 0.1.2 }─
//! elisp❯ <buffer>
//!         car           cdr           cons          defun         let    …
//! ```
//!
//! * Top "modeline" is rendered as part of `Prompt::render_prompt_left` so it
//!   repaints with the buffer (no scroll-off, no flicker).
//! * Tab pops a `ColumnarMenu` of suggestions sourced from
//!   `elisprs::lsp::completion_words` — the same special-form + subr wordlist
//!   the `--lsp` server serves for completion.
//! * A `Validator` wired to `crate::parens_balanced` keeps a multi-line
//!   `(defun …)` open until its parens close, matching the old line REPL.
//! * History is `~/.elisprs/history` via `FileBackedHistory`.
//!
//! Reedline does not include a file-path completer; bare-path completion is
//! intentionally dropped — the word list covers the high-value surface
//! (special forms + subrs) and matches utop's UX (commands, not paths).

use std::borrow::Cow;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use nu_ansi_term::{Color as NuColor, Style};
use reedline::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    ColumnarMenu, Completer, EditMode, Emacs, FileBackedHistory, KeyCode, KeyModifiers,
    Keybindings, MenuBuilder, Prompt, PromptEditMode, PromptHistorySearch,
    PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu, Signal, Span, Suggestion,
    ValidationResult, Validator, Vi,
};

const ELISP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn elisprs_dir() -> std::path::PathBuf {
    let dir = std::env::var_os("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".elisprs"))
        .unwrap_or_else(|| std::path::PathBuf::from(".elisprs"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn history_path() -> std::path::PathBuf {
    elisprs_dir().join("history")
}

fn config_path() -> std::path::PathBuf {
    elisprs_dir().join("config.toml")
}

/// Contents of the auto-seeded `~/.elisprs/config.toml`. Every setting is
/// commented out so the seeded file documents the schema without changing
/// behavior — uncomment + edit a line to override the in-code default.
const DEFAULT_CONFIG_TOML: &str = r#"# elisprs REPL config — auto-generated on first launch.
# Lines starting with `#` are comments. Uncomment + edit a line to
# override the in-code default. Delete this file and elisprs will
# regenerate it on the next run.

[repl]
# Edit mode for the interactive REPL. Defaults to emacs.
#
#   "emacs" — Ctrl-A/Ctrl-E/Ctrl-K/etc., readline-style (default)
#   "vi"    — modal editing; Esc → normal mode, i/a → insert,
#             h/j/k/l navigation, dd/cc/yy/x, /-search, etc.
#
# Tab + Shift+Tab cycle the completion menu in either mode.
# Override per-session with `ELISPRS_REPL_MODE=vi elisp --repl`.
# mode = "emacs"
"#;

/// First-run seed: write `~/.elisprs/config.toml` if it does not exist. Safe to
/// call on every launch — no-op when the file is already there (and silent if
/// the home directory is read-only). Honors `ELISPRS_NO_CONFIG=1` for CI /
/// sandbox environments that should not touch the user's home dir.
fn ensure_default_config_seeded() {
    if std::env::var_os("ELISPRS_NO_CONFIG").is_some() {
        return;
    }
    let path = config_path();
    if path.exists() {
        return;
    }
    let _ = std::fs::write(&path, DEFAULT_CONFIG_TOML);
}

/// REPL edit-mode selector. `Emacs` is the default; `Vi` enables reedline's
/// two-mode insert/normal keybinding set with the standard `Esc` toggle.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ReplMode {
    Emacs,
    Vi,
}

/// Resolve the REPL edit mode in this precedence:
/// 1. `ELISPRS_REPL_MODE=emacs|vi` env var (overrides everything).
/// 2. `~/.elisprs/config.toml` `[repl] mode = "vi"`.
/// 3. Default `Emacs`.
fn resolve_repl_mode() -> ReplMode {
    if let Some(env) = std::env::var_os("ELISPRS_REPL_MODE") {
        let s = env.to_string_lossy().to_ascii_lowercase();
        if s == "vi" || s == "vim" {
            return ReplMode::Vi;
        }
        if s == "emacs" {
            return ReplMode::Emacs;
        }
    }
    let raw = match std::fs::read_to_string(config_path()) {
        Ok(s) => s,
        Err(_) => return ReplMode::Emacs,
    };
    let parsed: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return ReplMode::Emacs,
    };
    let mode = parsed
        .get("repl")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("emacs");
    match mode.to_ascii_lowercase().as_str() {
        "vi" | "vim" => ReplMode::Vi,
        _ => ReplMode::Emacs,
    }
}

/// Apply the completion-menu Tab / Shift+Tab bindings to a keybinding set — so
/// the bindings live on the emacs map AND the vi insert map.
fn install_menu_bindings(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );
}

/// Byte index `start` and the incomplete word before cursor (for prefix
/// matching). Elisp symbols run until a reader boundary — whitespace or one of
/// `( ) " ' \` , ; [ ]` — so `-`, `*`, `+`, `/`, `?`, `!`, `<`, `>`, `=`, `:`
/// (and every other non-boundary char) stay part of the symbol. Mirrors the
/// `is_sym_char` rule the `--lsp` server uses (src/lsp.rs).
fn completion_word_start(line: &str, pos: usize) -> (usize, &str) {
    let pos = pos.min(line.len());
    let before = line.get(..pos).unwrap_or("");
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| {
            c.is_whitespace() || matches!(*c, '(' | ')' | '"' | '\'' | '`' | ',' | ';' | '[' | ']')
        })
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    (start, line.get(start..pos).unwrap_or(""))
}

struct ElispCompleter {
    static_words: Vec<String>,
}

impl Completer for ElispCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let (start, prefix) = completion_word_start(line, pos);
        let span = Span::new(start, pos);
        let mut out: Vec<Suggestion> = self
            .static_words
            .iter()
            .filter(|w| w.starts_with(prefix))
            .map(|w| Suggestion {
                value: w.clone(),
                description: None,
                style: None,
                extra: None,
                span,
                append_whitespace: false,
                display_override: None,
                match_indices: None,
            })
            .collect();
        out.sort_by(|a, b| a.value.cmp(&b.value));
        out
    }
}

/// Keeps a multi-line `(defun …)` open until its parens balance — the same
/// rule the old line-based REPL used, reused here so the reedline editor
/// accepts a complete top-level form on the closing paren.
struct ElispValidator;

impl Validator for ElispValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        if crate::parens_balanced(line) {
            ValidationResult::Complete
        } else {
            ValidationResult::Incomplete
        }
    }
}

struct ElispPrompt {
    cmd_count: Arc<Mutex<u64>>,
}

fn now_hms() -> String {
    // Local time via `libc::localtime_r` — no chrono / time crate. Reads
    // `/etc/localtime` (or `TZ` env); works on macOS aarch64 + Linux. On
    // failure or invalid epoch, falls back to UTC modulo math so the status
    // bar always shows something.
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&secs, &mut tm).is_null() };
    if ok {
        format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
    } else {
        let s = (secs as u64) % 86_400;
        format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    }
}

fn term_cols() -> usize {
    use std::os::unix::io::AsRawFd;
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let fd = std::io::stdout().as_raw_fd();
    let cols = if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_col > 0 {
        ws.ws_col as usize
    } else {
        std::env::var("COLUMNS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(80)
    };
    cols.max(40)
}

fn render_status_bar(cmd_count: u64) -> String {
    let cols = term_cols();
    let dim = NuColor::DarkGray;
    let accent = NuColor::Cyan;
    let label = NuColor::LightYellow;

    let left = format!(" {} ", now_hms());
    let mid = format!(" command {} ", cmd_count);
    let right = format!(" elisp {} ", ELISP_VERSION);

    // `frame_chars` = display width of every literal frame char emitted below
    // (`─(`, `)──<`, `>`, `{`, `}─`). Off-by-N here pushes the right segment
    // onto a new line. `chars().count()` isn't `const fn`, so this is a `let`.
    let frame_chars = "─()──<>{}─".chars().count();
    let visible = left.chars().count() + mid.chars().count() + right.chars().count() + frame_chars;
    let dashes = cols.saturating_sub(visible);
    // Need at least 1 dash on each side for the frame look; if the terminal is
    // genuinely too narrow, drop the right segment entirely (one line, no wrap).
    if dashes < 2 {
        return format!(
            "{lp}{l}{rp}{ml}{m}{mr}",
            lp = Style::new().fg(dim).paint("─("),
            l = Style::new().fg(accent).paint(left),
            rp = Style::new().fg(dim).paint(")"),
            ml = Style::new().fg(dim).paint("──<"),
            m = Style::new().fg(label).bold().paint(mid),
            mr = Style::new().fg(dim).paint(">"),
        );
    }
    let left_dash = dashes / 2;
    let right_dash = dashes - left_dash;

    let bar_l = "─".repeat(left_dash);
    let bar_r = "─".repeat(right_dash);

    format!(
        "{lp}{l}{rp}{ml}{m}{mr}{bar}{rl}{r}{rr}",
        lp = Style::new().fg(dim).paint("─("),
        l = Style::new().fg(accent).paint(left),
        rp = Style::new().fg(dim).paint(")"),
        ml = Style::new().fg(dim).paint("──<"),
        m = Style::new().fg(label).bold().paint(mid),
        mr = Style::new().fg(dim).paint(">"),
        bar = Style::new().fg(dim).paint(format!("{}{}", bar_l, bar_r)),
        rl = Style::new().fg(dim).paint("{"),
        r = Style::new().fg(NuColor::Magenta).paint(right),
        rr = Style::new().fg(dim).paint("}─"),
    )
}

impl Prompt for ElispPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        let count = self.cmd_count.lock().map(|g| *g).unwrap_or(0);
        let bar = render_status_bar(count);
        let prompt = Style::new()
            .fg(NuColor::Cyan)
            .bold()
            .paint("elisp")
            .to_string();
        Cow::Owned(format!("{}\n{}", bar, prompt))
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str> {
        let s = Style::new()
            .fg(NuColor::LightCyan)
            .bold()
            .paint("❯ ")
            .to_string();
        Cow::Owned(s)
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        let s = Style::new()
            .fg(NuColor::DarkGray)
            .paint("····❯ ")
            .to_string();
        Cow::Owned(s)
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}

/// Start the reedline REPL. Prints the live banner, then reads / evaluates /
/// prints forms in the persistent thread-local host until `exit` / Ctrl-D.
pub fn run() -> ExitCode {
    ensure_default_config_seeded();

    // Same cyberpunk banner `elisp --help` shows, so a fresh REPL session looks
    // like the rest of the CLI. Followed by a single hint line.
    elisprs::banner::print_banner(true);
    println!();
    println!("\x1b[2m  type `exit` or Ctrl-D to leave the REPL — Tab for completion\x1b[0m");
    println!();

    let static_words = elisprs::lsp::completion_words();
    let cmd_count = Arc::new(Mutex::new(0u64));

    let completer = ElispCompleter { static_words };

    let menu = ColumnarMenu::default()
        .with_name("completion_menu")
        .with_columns(4)
        .with_column_padding(2);

    let edit_mode: Box<dyn EditMode> = match resolve_repl_mode() {
        ReplMode::Emacs => {
            let mut kb = default_emacs_keybindings();
            install_menu_bindings(&mut kb);
            Box::new(Emacs::new(kb))
        }
        ReplMode::Vi => {
            let mut insert_kb = default_vi_insert_keybindings();
            install_menu_bindings(&mut insert_kb);
            let normal_kb = default_vi_normal_keybindings();
            Box::new(Vi::new(insert_kb, normal_kb))
        }
    };

    let history = match FileBackedHistory::with_file(5_000, history_path()) {
        Ok(h) => Box::new(h) as Box<dyn reedline::History>,
        Err(e) => {
            eprintln!("repl: history unavailable: {}", e);
            match FileBackedHistory::new(5_000) {
                Ok(h) => Box::new(h) as Box<dyn reedline::History>,
                Err(_) => {
                    eprintln!("repl: cannot create in-memory history");
                    return ExitCode::FAILURE;
                }
            }
        }
    };

    let mut line_editor = Reedline::create()
        .with_completer(Box::new(completer))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_edit_mode(edit_mode)
        .with_validator(Box::new(ElispValidator))
        .with_history(history);

    let prompt = ElispPrompt {
        cmd_count: Arc::clone(&cmd_count),
    };

    loop {
        let sig = match line_editor.read_line(&prompt) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("repl: {}", e);
                break;
            }
        };

        match sig {
            Signal::Success(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let low = trimmed.to_lowercase();
                if low == "exit" || low == "quit" {
                    break;
                }

                if let Ok(mut g) = cmd_count.lock() {
                    *g += 1;
                }

                match elisprs::eval_str(trimmed) {
                    Ok(v) => println!("{}", elisprs::print(&v, true)),
                    Err(e) => eprintln!("error: {}", elisprs::format_error(&e)),
                }
            }
            Signal::CtrlC => continue,
            Signal::CtrlD => break,
            _ => break,
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_word_at_cursor_is_whole_symbol() {
        let s = "mapcar";
        let (st, pre) = completion_word_start(s, s.len());
        assert_eq!(st, 0);
        assert_eq!(pre, "mapcar");
    }

    #[test]
    fn completion_word_start_snaps_after_open_paren() {
        // Inside a form, the head symbol starts right after `(`.
        let s = "(setq";
        let (st, pre) = completion_word_start(s, s.len());
        assert_eq!(st, 1);
        assert_eq!(pre, "setq");
    }

    #[test]
    fn completion_word_keeps_elisp_symbol_punctuation() {
        // `-`, `*`, `?`, `<`, `=` are elisp symbol chars, not boundaries.
        for sym in [
            "string-match",
            "*standard-output*",
            "string<",
            "cl-remove-if-not",
        ] {
            let line = format!("(foo {sym}");
            let (_, pre) = completion_word_start(&line, line.len());
            assert_eq!(pre, sym, "symbol punctuation split: {sym}");
        }
    }

    #[test]
    fn completion_word_empty_after_space() {
        let s = "(car ";
        let (st, pre) = completion_word_start(s, s.len());
        assert_eq!(st, s.len());
        assert_eq!(pre, "");
    }

    #[test]
    fn static_words_include_core_forms_and_subrs() {
        let v = elisprs::lsp::completion_words();
        for w in [
            "car", "cdr", "cons", "defun", "let", "lambda", "funcall", "message",
        ] {
            assert!(v.iter().any(|s| s == w), "{w} missing from completion");
        }
    }

    #[test]
    fn completion_words_are_sorted_and_deduped() {
        let v = elisprs::lsp::completion_words();
        assert!(
            v.windows(2).all(|w| w[0] < w[1]),
            "words not strictly sorted"
        );
    }

    #[test]
    fn validator_tracks_paren_balance() {
        let v = ElispValidator;
        assert!(matches!(v.validate("(+ 1 2)"), ValidationResult::Complete));
        assert!(matches!(
            v.validate("(defun f ()"),
            ValidationResult::Incomplete
        ));
    }
}
