//! The `elisp` binary — a fusevm frontend driver.
//!
//!   elisp FILE.el            evaluate a file (lowered to fusevm, run on fusevm)
//!   elisp -e "EXPR"          evaluate an expression, print its value
//!   elisp                    REPL
//!   elisp --lsp / --dap      language server / debug adapter (stubs)
//!   elisp --aot FILE -o OUT  lower to a fusevm chunk / native object
//!   elisp --version

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

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
        println!("  path:    {}", elisprs::cache::default_cache_path().display());
        println!("  entries: {count}");
        println!("  bytes:   {bytes}");
        println!("  enabled: {}", elisprs::cache::cache_enabled());
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--cache-clear") {
        return match elisprs::cache::clear() {
            Ok(()) => {
                println!("elisprs: cleared {}", elisprs::cache::default_cache_path().display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("elisp --cache-clear: {e}");
                ExitCode::FAILURE
            }
        };
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

    repl()
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

fn repl() -> ExitCode {
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

fn print_help() {
    println!(
        "elisp (elisprs) — Emacs Lisp on fusevm\n\
         \n\
         USAGE:\n\
         \x20 elisp FILE.el            evaluate a file\n\
         \x20 elisp -e EXPR            evaluate an expression and print its value\n\
         \x20 elisp                    start a REPL\n\
         \x20 elisp --lsp              language server over stdio (stub)\n\
         \x20 elisp --dap              debug adapter over stdio (stub)\n\
         \x20 elisp --aot FILE -o OUT  lower to a fusevm chunk / native object\n\
         \x20 elisp --cache-stats     show the rkyv bytecode cache stats\n\
         \x20 elisp --cache-clear     delete the rkyv bytecode cache\n\
         \x20 elisp --version\n\
         \n\
         ENV:\n\
         \x20 ELISPRS_CACHE=0          disable the bytecode cache\n\
         \x20 ELISPRS_CACHE_DEBUG=1    log cache HIT/MISS to stderr"
    );
}
