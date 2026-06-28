//! The `elisp` binary.
//!
//! Usage:
//!   elisp FILE.el            evaluate a file
//!   elisp -e "EXPR"          evaluate an expression, print its value
//!   elisp                    start a REPL
//!   elisp --lsp              run the language server (stub)
//!   elisp --dap              run the debug adapter (stub)
//!   elisp --aot FILE -o OUT  AOT-compile to a native object (milestone 2)
//!   elisp --version

use elisprs::Interp;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("elisp (elisprs) {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--lsp") {
        return code(elisprs::lsp::run_stdio());
    }
    if args.iter().any(|a| a == "--dap") {
        return code(elisprs::dap::run_stdio());
    }
    if args.iter().any(|a| a == "--aot") {
        return run_aot(&args);
    }
    if let Some(pos) = args.iter().position(|a| a == "-e" || a == "--eval") {
        let Some(expr) = args.get(pos + 1) else {
            eprintln!("elisp: -e requires an expression");
            return ExitCode::FAILURE;
        };
        let mut it = Interp::new();
        return match it.eval_str(expr) {
            Ok(v) => {
                println!("{}", it.print(&v, true));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }

    // First non-flag argument is treated as a file to load.
    if let Some(file) = args.iter().find(|a| !a.starts_with('-')) {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("elisp: cannot read {file}: {e}");
                return ExitCode::FAILURE;
            }
        };
        let mut it = Interp::new();
        return match it.eval_str(&src) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }

    repl()
}

fn run_aot(args: &[String]) -> ExitCode {
    let file = args.iter().find(|a| !a.starts_with('-') && a.ends_with(".el"));
    let out = args
        .iter()
        .position(|a| a == "-o")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("out.o"));
    let Some(file) = file else {
        eprintln!("elisp --aot: expected a .el file");
        return ExitCode::FAILURE;
    };
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("elisp: cannot read {file}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match elisprs::aot::compile_file(&src, &out) {
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

fn repl() -> ExitCode {
    let mut it = Interp::new();
    let stdin = io::stdin();
    let mut buf = String::new();
    println!("elisp (elisprs) {} — milestone-1 REPL. Ctrl-D to exit.", env!("CARGO_PKG_VERSION"));
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
        // Only evaluate once parens are balanced, so multi-line input works.
        if !parens_balanced(&buf) {
            continue;
        }
        let src = std::mem::take(&mut buf);
        if src.trim().is_empty() {
            continue;
        }
        match it.eval_str(&src) {
            Ok(v) => println!("{}", it.print(&v, true)),
            Err(e) => eprintln!("error: {e}"),
        }
    }
}

/// Crude paren balance that ignores parens inside strings and `;` comments —
/// enough for the REPL's continuation prompt.
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

fn code(n: i32) -> ExitCode {
    ExitCode::from(n as u8)
}

fn print_help() {
    println!(
        "elisp (elisprs) — Emacs Lisp on the rust_lisp reader\n\
         \n\
         USAGE:\n\
         \x20 elisp FILE.el            evaluate a file\n\
         \x20 elisp -e EXPR            evaluate an expression and print its value\n\
         \x20 elisp                    start a REPL\n\
         \x20 elisp --lsp              language server over stdio (stub)\n\
         \x20 elisp --dap              debug adapter over stdio (stub)\n\
         \x20 elisp --aot FILE -o OUT  AOT-compile to a native object (milestone 2)\n\
         \x20 elisp --version"
    );
}
