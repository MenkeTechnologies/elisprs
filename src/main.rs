//! The `elisp` binary — a fusevm frontend driver.
//!
//!   elisp FILE.el            evaluate a file (lowered to fusevm, run on fusevm)
//!   elisp -e "EXPR"          evaluate an expression, print its value
//!   elisp                    REPL
//!   elisp --lsp / --dap      language server (stub) / line-level debug adapter
//!   elisp --aot FILE -o OUT  lower to a fusevm chunk / native object
//!   elisp --version

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

mod repl;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "elisp (elisprs) {} — fusevm frontend",
            env!("CARGO_PKG_VERSION")
        );
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--lsp") {
        return ExitCode::from(elisprs::lsp::run_stdio() as u8);
    }
    if args.iter().any(|a| a == "--dap") {
        return ExitCode::from(elisprs::dap::run_stdio() as u8);
    }
    if args.iter().any(|a| a == "--aot-exe") {
        return run_aot(&args, true);
    }
    if args.iter().any(|a| a == "--aot") {
        return run_aot(&args, false);
    }
    if args.iter().any(|a| a == "--cache-stats") {
        let (count, bytes) = elisprs::cache::stats();
        println!("elisprs bytecode cache");
        println!(
            "  path:    {}",
            elisprs::cache::default_cache_path().display()
        );
        println!("  entries: {count}");
        println!("  bytes:   {bytes}");
        println!("  enabled: {}", elisprs::cache::cache_enabled());
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--cache-clear") {
        return match elisprs::cache::clear() {
            Ok(()) => {
                println!(
                    "elisprs: cleared {}",
                    elisprs::cache::default_cache_path().display()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("elisp --cache-clear: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if args.iter().any(|a| a == "--repl") {
        return repl::run();
    }
    if let Some(pos) = args.iter().position(|a| a == "-e" || a == "--eval") {
        let Some(expr) = args.get(pos + 1) else {
            eprintln!("elisp: -e requires an expression");
            return ExitCode::FAILURE;
        };
        return match elisprs::eval_str(expr) {
            Ok(v) => {
                println!("{}", elisprs::print(&v, true));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {}", elisprs::format_error(&e));
                ExitCode::FAILURE
            }
        };
    }

    if let Some(file) = args.iter().find(|a| !a.starts_with('-')) {
        return match elisprs::eval_file(file) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {}", elisprs::format_error(&e));
                ExitCode::FAILURE
            }
        };
    }

    // No flag, no file: launch the reedline REPL on an interactive terminal;
    // fall back to the plain line-buffered reader when stdin is piped (so
    // `echo '(+ 1 2)' | elisp` still evaluates without a TTY line editor).
    if io::stdin().is_terminal() {
        repl::run()
    } else {
        repl_stdin()
    }
}

fn run_aot(args: &[String], exe: bool) -> ExitCode {
    let Some(file) = args.iter().find(|a| a.ends_with(".el")) else {
        eprintln!("elisp --aot: expected a .el file");
        return ExitCode::FAILURE;
    };
    let default = if exe { "a.out" } else { "out.o" };
    let out = args
        .iter()
        .position(|a| a == "-o")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default));
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("elisp: cannot read {file}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = if exe {
        elisprs::aot::compile_executable(&src, &out)
    } else {
        elisprs::aot::compile_file(&src, &out)
    };
    match result {
        Ok(()) => {
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("aot: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Plain line-buffered REPL used when stdin is not a terminal (piped input).
/// Accumulates lines until parens balance, then evaluates. The interactive TTY
/// path uses the reedline editor in `repl::run`.
fn repl_stdin() -> ExitCode {
    let stdin = io::stdin();
    let mut buf = String::new();
    println!(
        "elisp (elisprs) {} — fusevm frontend REPL. Ctrl-D to exit.",
        env!("CARGO_PKG_VERSION")
    );
    loop {
        print!("{} ", if buf.is_empty() { "elisp>" } else { "  ...>" });
        let _ = io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                return ExitCode::SUCCESS;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("read error: {e}");
                return ExitCode::FAILURE;
            }
        }
        buf.push_str(&line);
        if !parens_balanced(&buf) {
            continue;
        }
        let src = std::mem::take(&mut buf);
        if src.trim().is_empty() {
            continue;
        }
        match elisprs::eval_str(&src) {
            Ok(v) => println!("{}", elisprs::print(&v, true)),
            Err(e) => eprintln!("error: {}", elisprs::format_error(&e)),
        }
    }
}

fn parens_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escaped = false;
    for line in s.lines() {
        let mut in_comment = false;
        for c in line.chars() {
            if in_comment {
                break;
            }
            if in_str {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_str = false;
                }
                continue;
            }
            match c {
                '"' => in_str = true,
                ';' => in_comment = true,
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
        }
    }
    depth <= 0
}

/// Print the `--help` / `-h` screen in the MenkeTechnologies house style (see
/// `tp -h`): ANSI-Shadow banner, a status box padded at runtime so its right
/// border never drifts as the version grows, yellow `USAGE:`, cyan section
/// rules, green `//` comment separators, and a SYSTEM footer.
fn print_help() {
    const BOX_W: usize = 54;
    let ver = env!("CARGO_PKG_VERSION");
    let status = format!(" STATUS: ONLINE  // SIGNAL: ████████░░ // v{ver}");
    let space = " ".repeat(BOX_W.saturating_sub(status.chars().count()));
    let rule = "─".repeat(BOX_W);
    // Logo glyphs live in exactly one place — `banner::LOGO_ROWS` — so the
    // `--help` header and the REPL banner never drift.
    print!("\n{}", elisprs::banner::logo_colored(true));
    print!(
        concat!(
            " \x1b[36m┌{rule}┐\x1b[0m\n",
                " \x1b[36m│\x1b[0m{status}{space}\x1b[36m│\x1b[0m\n",
                " \x1b[36m└{rule}┘\x1b[0m\n",
                "\x1b[35m  >> EMACS LISP ON FUSEVM // FULL SPECTRUM <<\x1b[0m\n",
                "\n",
                "  Emacs Lisp interpreter on the fusevm bytecode VM\n",
                "\n",
                "\x1b[33m  USAGE:\x1b[0m elisp [OPTIONS] [FILE]\n",
                "\n",
                "\x1b[36m  ── MODES ──────────────────────────────────────────────\x1b[0m\n",
                "  elisp FILE.el            \x1b[32m//\x1b[0m evaluate a file\n",
                "  elisp -e EXPR            \x1b[32m//\x1b[0m evaluate an expression and print its value\n",
                "  elisp                    \x1b[32m//\x1b[0m start a REPL\n",
                "  elisp --repl             \x1b[32m//\x1b[0m start the reedline REPL (Tab-completion + stats banner)\n",
                "  elisp --lsp              \x1b[32m//\x1b[0m language server over stdio (stub)\n",
                "  elisp --dap              \x1b[32m//\x1b[0m line-level debug adapter over stdio (breakpoints, stepping, variables)\n",
                "  elisp --aot FILE -o OUT  \x1b[32m//\x1b[0m lower to a fusevm chunk / native object\n",
                "  elisp --cache-stats      \x1b[32m//\x1b[0m show the rkyv bytecode cache stats\n",
                "  elisp --cache-clear      \x1b[32m//\x1b[0m delete the rkyv bytecode cache\n",
                "  elisp --version          \x1b[32m//\x1b[0m print version\n",
                "  elisp --help             \x1b[32m//\x1b[0m print this help\n",
                "\n",
                "\x1b[36m  ── ENV ────────────────────────────────────────────────\x1b[0m\n",
                "  ELISPRS_CACHE=0          \x1b[32m//\x1b[0m disable the bytecode cache\n",
                "  ELISPRS_CACHE_DEBUG=1    \x1b[32m//\x1b[0m log cache HIT/MISS to stderr\n",
                "\n",
                "\x1b[36m  ── SYSTEM ─────────────────────────────────────────────\x1b[0m\n",
                "  \x1b[35mv{ver} \x1b[0m// \x1b[33m(c) MenkeTechnologies\x1b[0m\n",
                "  \x1b[35mThe parens are balanced. The runtime is vast.\x1b[0m\n",
                "  \x1b[33m>>> JACK IN. EVAL THE FORM. RUN ELISP EVERYWHERE. <<<\x1b[0m\n",
                " \x1b[36m░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░\x1b[0m\n",
            ),
            rule = rule,
            status = status,
            space = space,
            ver = ver,
    );
}
