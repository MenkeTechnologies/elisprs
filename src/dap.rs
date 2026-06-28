//! `elisp --dap`: debug adapter (milestone-1 stub).
//!
//! Will speak the Debug Adapter Protocol over stdio, reusing the transport
//! patterns in `zemacs/zemacs-dap` (transport.rs / registry.rs). The evaluator
//! already has the hooks a debugger needs:
//! - `Interp::specstack` is the dynamic-binding stack → call-stack frames + locals
//! - `eval`/`apply` are the natural step/breakpoint boundaries
//! - the obarray is the variable-inspection surface
//!
//! Planned capabilities: setBreakpoints (by form), stepIn/Over/Out across
//! `eval_body`, stackTrace from the closure call chain, evaluate (REPL in frame).

pub fn run_stdio() -> i32 {
    eprintln!("elisp --dap: debug adapter is a milestone-1 stub (not yet implemented).");
    eprintln!("Planned: DAP over stdio (breakpoints/stepping/stack/inspect) hanging off");
    eprintln!("Interp::eval + the dynamic specstack, reusing zemacs-dap transport.");
    0
}
