//! `elisp --lsp` — a Language Server (stdio) for Emacs Lisp.
//!
//! Self-contained: it reuses elisprs's own reader rules for diagnostics and a
//! metadata table (the installed subrs + the special forms the compiler knows)
//! for completion / hover / signature help / document symbols. No output reaches
//! the terminal — the server speaks JSON-RPC on stdio only.
//!
//! Capabilities: full-sync text documents, publish-diagnostics on open/change,
//! completion, hover, document symbols, and signature help.

use std::collections::HashMap;

use lsp_server::{Connection, Message, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, HoverRequest, Request as _, SignatureHelpRequest,
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionResponse, Diagnostic,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidOpenTextDocumentParams, Documentation,
    Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, OneOf, Position,
    PublishDiagnosticsParams, Range, ServerCapabilities, SignatureHelp, SignatureHelpOptions,
    SignatureHelpParams, SignatureInformation, SymbolInformation, SymbolKind,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};

/// One entry in the language metadata table.
struct Entry {
    name: &'static str,
    kind: Kind,
    sig: &'static str,
    doc: &'static str,
}
#[derive(Clone, Copy)]
enum Kind {
    SpecialForm,
    Function,
}

/// Special forms the compiler recognizes (lowered or pending). Offered for
/// completion/hover regardless of lowering status — they are valid elisp.
const SPECIAL_FORMS: &[Entry] = &[
    Entry {
        name: "quote",
        kind: Kind::SpecialForm,
        sig: "(quote OBJECT)",
        doc: "Return OBJECT, unevaluated.",
    },
    Entry {
        name: "function",
        kind: Kind::SpecialForm,
        sig: "(function OBJECT)",
        doc: "Like quote, but for functions (`#'`).",
    },
    Entry {
        name: "lambda",
        kind: Kind::SpecialForm,
        sig: "(lambda ARGLIST BODY...)",
        doc: "An anonymous function.",
    },
    Entry {
        name: "progn",
        kind: Kind::SpecialForm,
        sig: "(progn BODY...)",
        doc: "Evaluate BODY in order; return the last value.",
    },
    Entry {
        name: "if",
        kind: Kind::SpecialForm,
        sig: "(if COND THEN ELSE...)",
        doc: "If COND is non-nil, eval THEN, else the ELSE forms.",
    },
    Entry {
        name: "when",
        kind: Kind::SpecialForm,
        sig: "(when COND BODY...)",
        doc: "If COND is non-nil, eval BODY.",
    },
    Entry {
        name: "unless",
        kind: Kind::SpecialForm,
        sig: "(unless COND BODY...)",
        doc: "If COND is nil, eval BODY.",
    },
    Entry {
        name: "and",
        kind: Kind::SpecialForm,
        sig: "(and CONDITIONS...)",
        doc: "Eval forms until one is nil; return the last value.",
    },
    Entry {
        name: "or",
        kind: Kind::SpecialForm,
        sig: "(or CONDITIONS...)",
        doc: "Eval forms until one is non-nil; return it.",
    },
    Entry {
        name: "setq",
        kind: Kind::SpecialForm,
        sig: "(setq SYM VAL SYM VAL...)",
        doc: "Set each SYM's value cell to VAL.",
    },
    Entry {
        name: "let",
        kind: Kind::SpecialForm,
        sig: "(let BINDINGS BODY...)",
        doc: "Bind variables in parallel, then eval BODY.",
    },
    Entry {
        name: "let*",
        kind: Kind::SpecialForm,
        sig: "(let* BINDINGS BODY...)",
        doc: "Bind variables sequentially, then eval BODY.",
    },
    Entry {
        name: "while",
        kind: Kind::SpecialForm,
        sig: "(while COND BODY...)",
        doc: "While COND is non-nil, eval BODY.",
    },
    Entry {
        name: "cond",
        kind: Kind::SpecialForm,
        sig: "(cond CLAUSES...)",
        doc: "Try each (TEST BODY...) clause; eval the first whose TEST is non-nil.",
    },
    Entry {
        name: "defun",
        kind: Kind::SpecialForm,
        sig: "(defun NAME ARGLIST BODY...)",
        doc: "Define NAME as a function.",
    },
    Entry {
        name: "defmacro",
        kind: Kind::SpecialForm,
        sig: "(defmacro NAME ARGLIST BODY...)",
        doc: "Define NAME as a macro.",
    },
    Entry {
        name: "defvar",
        kind: Kind::SpecialForm,
        sig: "(defvar NAME &optional INIT DOC)",
        doc: "Define a special (dynamic) variable.",
    },
    Entry {
        name: "defconst",
        kind: Kind::SpecialForm,
        sig: "(defconst NAME INIT &optional DOC)",
        doc: "Define a constant special variable.",
    },
];

/// The installed primitive subrs (mirrors `builtins::install`).
const SUBRS: &[Entry] = &[
    Entry {
        name: "+",
        kind: Kind::Function,
        sig: "(+ &rest NUMBERS)",
        doc: "Sum of the arguments.",
    },
    Entry {
        name: "-",
        kind: Kind::Function,
        sig: "(- &rest NUMBERS)",
        doc: "Negation, or subtraction from the first.",
    },
    Entry {
        name: "*",
        kind: Kind::Function,
        sig: "(* &rest NUMBERS)",
        doc: "Product of the arguments.",
    },
    Entry {
        name: "/",
        kind: Kind::Function,
        sig: "(/ DIVIDEND &rest DIVISORS)",
        doc: "Quotient of the arguments.",
    },
    Entry {
        name: "%",
        kind: Kind::Function,
        sig: "(% X Y)",
        doc: "Integer remainder of X divided by Y.",
    },
    Entry {
        name: "1+",
        kind: Kind::Function,
        sig: "(1+ NUMBER)",
        doc: "NUMBER plus one.",
    },
    Entry {
        name: "1-",
        kind: Kind::Function,
        sig: "(1- NUMBER)",
        doc: "NUMBER minus one.",
    },
    Entry {
        name: "=",
        kind: Kind::Function,
        sig: "(= &rest NUMBERS)",
        doc: "Non-nil if all numbers are equal.",
    },
    Entry {
        name: "<",
        kind: Kind::Function,
        sig: "(< &rest NUMBERS)",
        doc: "Non-nil if numbers strictly increase.",
    },
    Entry {
        name: ">",
        kind: Kind::Function,
        sig: "(> &rest NUMBERS)",
        doc: "Non-nil if numbers strictly decrease.",
    },
    Entry {
        name: "<=",
        kind: Kind::Function,
        sig: "(<= &rest NUMBERS)",
        doc: "Non-nil if numbers are non-decreasing.",
    },
    Entry {
        name: ">=",
        kind: Kind::Function,
        sig: "(>= &rest NUMBERS)",
        doc: "Non-nil if numbers are non-increasing.",
    },
    Entry {
        name: "eq",
        kind: Kind::Function,
        sig: "(eq A B)",
        doc: "Non-nil if A and B are the same object.",
    },
    Entry {
        name: "eql",
        kind: Kind::Function,
        sig: "(eql A B)",
        doc: "Like eq, but compares numbers by value.",
    },
    Entry {
        name: "equal",
        kind: Kind::Function,
        sig: "(equal A B)",
        doc: "Non-nil if A and B are structurally equal.",
    },
    Entry {
        name: "null",
        kind: Kind::Function,
        sig: "(null OBJECT)",
        doc: "Non-nil if OBJECT is nil.",
    },
    Entry {
        name: "not",
        kind: Kind::Function,
        sig: "(not OBJECT)",
        doc: "Non-nil if OBJECT is nil.",
    },
    Entry {
        name: "consp",
        kind: Kind::Function,
        sig: "(consp OBJECT)",
        doc: "Non-nil if OBJECT is a cons cell.",
    },
    Entry {
        name: "listp",
        kind: Kind::Function,
        sig: "(listp OBJECT)",
        doc: "Non-nil if OBJECT is a list (cons or nil).",
    },
    Entry {
        name: "atom",
        kind: Kind::Function,
        sig: "(atom OBJECT)",
        doc: "Non-nil if OBJECT is not a cons cell.",
    },
    Entry {
        name: "symbolp",
        kind: Kind::Function,
        sig: "(symbolp OBJECT)",
        doc: "Non-nil if OBJECT is a symbol.",
    },
    Entry {
        name: "stringp",
        kind: Kind::Function,
        sig: "(stringp OBJECT)",
        doc: "Non-nil if OBJECT is a string.",
    },
    Entry {
        name: "numberp",
        kind: Kind::Function,
        sig: "(numberp OBJECT)",
        doc: "Non-nil if OBJECT is a number.",
    },
    Entry {
        name: "integerp",
        kind: Kind::Function,
        sig: "(integerp OBJECT)",
        doc: "Non-nil if OBJECT is an integer.",
    },
    Entry {
        name: "floatp",
        kind: Kind::Function,
        sig: "(floatp OBJECT)",
        doc: "Non-nil if OBJECT is a float.",
    },
    Entry {
        name: "vectorp",
        kind: Kind::Function,
        sig: "(vectorp OBJECT)",
        doc: "Non-nil if OBJECT is a vector.",
    },
    Entry {
        name: "zerop",
        kind: Kind::Function,
        sig: "(zerop NUMBER)",
        doc: "Non-nil if NUMBER is zero.",
    },
    Entry {
        name: "cons",
        kind: Kind::Function,
        sig: "(cons CAR CDR)",
        doc: "Create a new cons cell.",
    },
    Entry {
        name: "car",
        kind: Kind::Function,
        sig: "(car LIST)",
        doc: "Return the first element of LIST.",
    },
    Entry {
        name: "cdr",
        kind: Kind::Function,
        sig: "(cdr LIST)",
        doc: "Return the rest of LIST after the first element.",
    },
    Entry {
        name: "setcar",
        kind: Kind::Function,
        sig: "(setcar CELL VALUE)",
        doc: "Set the car of CELL to VALUE.",
    },
    Entry {
        name: "setcdr",
        kind: Kind::Function,
        sig: "(setcdr CELL VALUE)",
        doc: "Set the cdr of CELL to VALUE.",
    },
    Entry {
        name: "list",
        kind: Kind::Function,
        sig: "(list &rest OBJECTS)",
        doc: "Return a newly created list of OBJECTS.",
    },
    Entry {
        name: "append",
        kind: Kind::Function,
        sig: "(append &rest SEQUENCES)",
        doc: "Concatenate lists into one.",
    },
    Entry {
        name: "reverse",
        kind: Kind::Function,
        sig: "(reverse SEQUENCE)",
        doc: "Return a reversed copy of SEQUENCE.",
    },
    Entry {
        name: "length",
        kind: Kind::Function,
        sig: "(length SEQUENCE)",
        doc: "Return the length of SEQUENCE.",
    },
    Entry {
        name: "nth",
        kind: Kind::Function,
        sig: "(nth N LIST)",
        doc: "Return the Nth element of LIST.",
    },
    Entry {
        name: "vector",
        kind: Kind::Function,
        sig: "(vector &rest OBJECTS)",
        doc: "Return a vector of OBJECTS.",
    },
    Entry {
        name: "make-vector",
        kind: Kind::Function,
        sig: "(make-vector LENGTH INIT)",
        doc: "Return a vector of LENGTH elements, all INIT.",
    },
    Entry {
        name: "aref",
        kind: Kind::Function,
        sig: "(aref ARRAY IDX)",
        doc: "Return the IDX'th element of ARRAY.",
    },
    Entry {
        name: "aset",
        kind: Kind::Function,
        sig: "(aset ARRAY IDX VALUE)",
        doc: "Set the IDX'th element of ARRAY to VALUE.",
    },
    Entry {
        name: "symbol-name",
        kind: Kind::Function,
        sig: "(symbol-name SYMBOL)",
        doc: "Return SYMBOL's name as a string.",
    },
    Entry {
        name: "intern",
        kind: Kind::Function,
        sig: "(intern NAME)",
        doc: "Return the interned symbol named NAME.",
    },
    Entry {
        name: "make-symbol",
        kind: Kind::Function,
        sig: "(make-symbol NAME)",
        doc: "Return a fresh symbol named NAME.",
    },
    Entry {
        name: "set",
        kind: Kind::Function,
        sig: "(set SYMBOL VALUE)",
        doc: "Set SYMBOL's value cell to VALUE.",
    },
    Entry {
        name: "symbol-value",
        kind: Kind::Function,
        sig: "(symbol-value SYMBOL)",
        doc: "Return SYMBOL's value.",
    },
    Entry {
        name: "funcall",
        kind: Kind::Function,
        sig: "(funcall FUNCTION &rest ARGS)",
        doc: "Call FUNCTION with ARGS.",
    },
    Entry {
        name: "apply",
        kind: Kind::Function,
        sig: "(apply FUNCTION &rest ARGS LIST)",
        doc: "Call FUNCTION with ARGS and the elements of LIST.",
    },
    Entry {
        name: "identity",
        kind: Kind::Function,
        sig: "(identity ARG)",
        doc: "Return ARG unchanged.",
    },
    Entry {
        name: "concat",
        kind: Kind::Function,
        sig: "(concat &rest STRINGS)",
        doc: "Concatenate STRINGS into one string.",
    },
    Entry {
        name: "format",
        kind: Kind::Function,
        sig: "(format STRING &rest ARGS)",
        doc: "Format ARGS per the %-directives in STRING.",
    },
    Entry {
        name: "message",
        kind: Kind::Function,
        sig: "(message FORMAT &rest ARGS)",
        doc: "Print a formatted message (to stderr).",
    },
    Entry {
        name: "princ",
        kind: Kind::Function,
        sig: "(princ OBJECT)",
        doc: "Output OBJECT (no quoting).",
    },
    Entry {
        name: "prin1",
        kind: Kind::Function,
        sig: "(prin1 OBJECT)",
        doc: "Output OBJECT in read syntax.",
    },
    Entry {
        name: "number-to-string",
        kind: Kind::Function,
        sig: "(number-to-string NUMBER)",
        doc: "Return NUMBER rendered as a string.",
    },
];

fn lookup(name: &str) -> Option<&'static Entry> {
    SPECIAL_FORMS.iter().chain(SUBRS).find(|e| e.name == name)
}

pub fn run_stdio() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("elisp --lsp: {e}");
            1
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["(".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), " ".to_string()]),
            retrigger_characters: None,
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        ..Default::default()
    };
    let _ = connection.initialize(serde_json::to_value(caps)?)?;
    main_loop(&connection)?;
    io_threads.join()?;
    Ok(())
}

/// lsp-types `Uri` has interior mutability, so we key documents by its string
/// form (clippy's "mutable key type") and keep the `Uri` only where the
/// protocol needs it.
fn uri_key(uri: &Uri) -> String {
    uri.as_str().to_string()
}

fn main_loop(c: &Connection) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let mut docs: HashMap<String, String> = HashMap::new();
    for msg in &c.receiver {
        match msg {
            Message::Request(req) => {
                if c.handle_shutdown(&req)? {
                    return Ok(());
                }
                let resp = handle_request(&req, &docs);
                c.sender.send(Message::Response(resp))?;
            }
            Message::Notification(not) => match not.method.as_str() {
                DidOpenTextDocument::METHOD => {
                    let p: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                    let uri = p.text_document.uri.clone();
                    docs.insert(uri_key(&uri), p.text_document.text.clone());
                    publish(c, &uri, &p.text_document.text)?;
                }
                DidChangeTextDocument::METHOD => {
                    let p: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                    if let Some(change) = p.content_changes.into_iter().last() {
                        let uri = p.text_document.uri.clone();
                        docs.insert(uri_key(&uri), change.text.clone());
                        publish(c, &uri, &change.text)?;
                    }
                }
                DidCloseTextDocument::METHOD => {
                    let p: lsp_types::DidCloseTextDocumentParams =
                        serde_json::from_value(not.params)?;
                    docs.remove(&uri_key(&p.text_document.uri));
                }
                _ => {}
            },
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn ok_response(id: RequestId, value: serde_json::Value) -> Response {
    Response {
        id,
        result: Some(value),
        error: None,
    }
}

fn handle_request(req: &Request, docs: &HashMap<String, String>) -> Response {
    let id = req.id.clone();
    let result = match req.method.as_str() {
        Completion::METHOD => serde_json::to_value(completion()).ok(),
        HoverRequest::METHOD => {
            let p: Result<HoverParams, _> = serde_json::from_value(req.params.clone());
            p.ok().and_then(|p| {
                let uri = &p.text_document_position_params.text_document.uri;
                let pos = p.text_document_position_params.position;
                docs.get(&uri_key(uri))
                    .and_then(|t| hover(t, pos))
                    .and_then(|h| serde_json::to_value(h).ok())
            })
        }
        DocumentSymbolRequest::METHOD => {
            let p: Result<lsp_types::DocumentSymbolParams, _> =
                serde_json::from_value(req.params.clone());
            p.ok().and_then(|p| {
                docs.get(&uri_key(&p.text_document.uri)).map(|t| {
                    serde_json::to_value(document_symbols(&p.text_document.uri, t)).unwrap()
                })
            })
        }
        SignatureHelpRequest::METHOD => {
            let p: Result<SignatureHelpParams, _> = serde_json::from_value(req.params.clone());
            p.ok().and_then(|p| {
                let uri = &p.text_document_position_params.text_document.uri;
                let pos = p.text_document_position_params.position;
                docs.get(&uri_key(uri))
                    .and_then(|t| signature_help(t, pos))
                    .and_then(|h| serde_json::to_value(h).ok())
            })
        }
        _ => None,
    };
    ok_response(id, result.unwrap_or(serde_json::Value::Null))
}

fn publish(
    c: &Connection,
    uri: &Uri,
    text: &str,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let diags = diagnostics(text);
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: diags,
        version: None,
    };
    c.sender
        .send(Message::Notification(lsp_server::Notification {
            method: PublishDiagnostics::METHOD.to_string(),
            params: serde_json::to_value(params)?,
        }))?;
    Ok(())
}

// ── line index ──────────────────────────────────────────────────────────────

/// Maps byte/char offsets to LSP `Position` (line + char). Operates on chars.
struct LineIndex {
    line_starts: Vec<usize>, // char offset of each line start
}
impl LineIndex {
    fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, c) in text.chars().enumerate() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex { line_starts }
    }
    fn position(&self, offset: usize) -> Position {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(l) => l,
            Err(l) => l - 1,
        };
        Position {
            line: line as u32,
            character: (offset - self.line_starts[line]) as u32,
        }
    }
    fn range(&self, start: usize, end: usize) -> Range {
        Range {
            start: self.position(start),
            end: self.position(end),
        }
    }
}

// ── diagnostics ──────────────────────────────────────────────────────────────

/// Position-aware scan mirroring the reader's error rules: unmatched parens,
/// unterminated strings, and the unsupported backquote/unquote syntax.
fn diagnostics(text: &str) -> Vec<Diagnostic> {
    let chars: Vec<char> = text.chars().collect();
    let idx = LineIndex::new(text);
    let mut out = Vec::new();
    let mut stack: Vec<usize> = Vec::new(); // offsets of open parens
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ';' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '"' => {
                let start = i;
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    match chars[i] {
                        '\\' => i += 2,
                        '"' => {
                            closed = true;
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
                if !closed {
                    out.push(diag(idx.range(start, chars.len()), "unterminated string"));
                }
                continue;
            }
            '?' => {
                // char literal: skip the (possibly escaped) next char
                i += if chars.get(i + 1) == Some(&'\\') {
                    3
                } else {
                    2
                };
                continue;
            }
            '(' => stack.push(i),
            ')' => {
                if stack.pop().is_none() {
                    out.push(diag(idx.range(i, i + 1), "unexpected `)`"));
                }
            }
            '`' | ',' => {
                out.push(diag(
                    idx.range(i, i + 1),
                    "backquote/unquote not supported yet",
                ));
            }
            _ => {}
        }
        i += 1;
    }
    for open in stack {
        out.push(diag(
            idx.range(open, open + 1),
            "unclosed `(` — missing `)`",
        ));
    }
    out
}

fn diag(range: Range, msg: &str) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("elisprs".to_string()),
        message: msg.to_string(),
        ..Default::default()
    }
}

// ── token helpers ────────────────────────────────────────────────────────────

fn is_sym_char(c: char) -> bool {
    !(c.is_whitespace() || matches!(c, '(' | ')' | '"' | '\'' | '`' | ',' | ';'))
}

/// The symbol token covering char-offset `off` (or just before it).
fn token_at(chars: &[char], off: usize) -> Option<String> {
    if chars.is_empty() {
        return None;
    }
    let mut start = off.min(chars.len());
    while start > 0 && is_sym_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = off.min(chars.len());
    while end < chars.len() && is_sym_char(chars[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    let tok: String = chars[start..end].iter().collect();
    Some(tok)
}

fn offset_of(text: &str, pos: Position) -> usize {
    let mut line = 0u32;
    for (i, c) in text.chars().enumerate() {
        if line == pos.line {
            return i + pos.character as usize;
        }
        if c == '\n' {
            line += 1;
        }
    }
    text.chars().count()
}

// ── feature handlers ─────────────────────────────────────────────────────────

fn completion() -> CompletionResponse {
    let items = SPECIAL_FORMS
        .iter()
        .chain(SUBRS)
        .map(|e| CompletionItem {
            label: e.name.to_string(),
            kind: Some(match e.kind {
                Kind::SpecialForm => CompletionItemKind::KEYWORD,
                Kind::Function => CompletionItemKind::FUNCTION,
            }),
            detail: Some(e.sig.to_string()),
            documentation: Some(Documentation::String(e.doc.to_string())),
            ..Default::default()
        })
        .collect();
    CompletionResponse::Array(items)
}

fn hover(text: &str, pos: Position) -> Option<Hover> {
    let chars: Vec<char> = text.chars().collect();
    let off = offset_of(text, pos);
    let tok = token_at(&chars, off)?;
    let e = lookup(&tok)?;
    let md = format!("```elisp\n{}\n```\n\n{}", e.sig, e.doc);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

fn signature_help(text: &str, pos: Position) -> Option<SignatureHelp> {
    let chars: Vec<char> = text.chars().collect();
    let off = offset_of(text, pos).min(chars.len());
    // Walk back to the innermost unmatched '(' and read the head symbol.
    let mut depth = 0i32;
    let mut i = off;
    while i > 0 {
        i -= 1;
        match chars[i] {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    let head = token_at(&chars, i + 1)?;
                    let e = lookup(&head)?;
                    return Some(SignatureHelp {
                        signatures: vec![SignatureInformation {
                            label: e.sig.to_string(),
                            documentation: Some(Documentation::String(e.doc.to_string())),
                            parameters: None,
                            active_parameter: None,
                        }],
                        active_signature: Some(0),
                        active_parameter: None,
                    });
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Document symbols: `(defun|defmacro|defvar|defconst NAME ...)` definitions.
#[allow(deprecated)]
fn document_symbols(uri: &Uri, text: &str) -> Vec<SymbolInformation> {
    let chars: Vec<char> = text.chars().collect();
    let idx = LineIndex::new(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '(' {
            let head_start = i + 1;
            if let Some(head) = token_at(&chars, head_start) {
                let (kind, is_def) = match head.as_str() {
                    "defun" | "defmacro" => (SymbolKind::FUNCTION, true),
                    "defvar" | "defconst" => (SymbolKind::VARIABLE, true),
                    _ => (SymbolKind::NULL, false),
                };
                if is_def {
                    // skip past head + whitespace to the NAME token
                    let mut j = head_start + head.chars().count();
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    if let Some(name) = token_at(&chars, j) {
                        let end = j + name.chars().count();
                        #[allow(deprecated)]
                        out.push(SymbolInformation {
                            name,
                            kind,
                            tags: None,
                            deprecated: None,
                            location: lsp_types::Location {
                                uri: uri.clone(),
                                range: idx.range(j, end),
                            },
                            container_name: None,
                        });
                    }
                }
            }
        }
        i += 1;
    }
    out
}
