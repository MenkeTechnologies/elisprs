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
pub struct Entry {
    pub name: &'static str,
    pub kind: Kind,
    pub sig: &'static str,
    pub doc: &'static str,
}
#[derive(Clone, Copy)]
pub enum Kind {
    SpecialForm,
    Function,
}

/// Special forms the compiler recognizes (lowered or pending). Offered for
/// completion/hover regardless of lowering status — they are valid elisp.
pub const SPECIAL_FORMS: &[Entry] = &[
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
pub const SUBRS: &[Entry] = &[
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
        name: "format-time-string",
        kind: Kind::Function,
        sig: "(format-time-string FORMAT &optional TIME ZONE)",
        doc: "Format TIME (default now) per FORMAT. ZONE nil=local, t=UTC, integer=offset secs.",
    },
    Entry {
        name: "float-time",
        kind: Kind::Function,
        sig: "(float-time &optional TIME)",
        doc: "Return TIME (default now) as seconds since the epoch, a float.",
    },
    Entry {
        name: "current-time",
        kind: Kind::Function,
        sig: "(current-time)",
        doc: "Return the current time as (HIGH LOW USEC PSEC).",
    },
    Entry {
        name: "current-time-string",
        kind: Kind::Function,
        sig: "(current-time-string &optional TIME ZONE)",
        doc: "Return TIME (default now) as a string like \"Sun Jun 29 12:00:00 2025\".",
    },
    Entry {
        name: "decode-time",
        kind: Kind::Function,
        sig: "(decode-time &optional TIME ZONE)",
        doc: "Decompose TIME into (SEC MIN HOUR DAY MON YEAR DOW DST UTCOFF).",
    },
    Entry {
        name: "encode-time",
        kind: Kind::Function,
        sig: "(encode-time TIME) or (encode-time SEC MIN HOUR DAY MON YEAR &optional ZONE)",
        doc: "Inverse of decode-time: turn decoded components into a (HIGH LOW) time value.",
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
        name: "boundp",
        kind: Kind::Function,
        sig: "(boundp SYMBOL)",
        doc: "Non-nil if SYMBOL has a value.",
    },
    Entry {
        name: "fboundp",
        kind: Kind::Function,
        sig: "(fboundp SYMBOL)",
        doc: "Non-nil if SYMBOL has a function definition.",
    },
    Entry {
        name: "fset",
        kind: Kind::Function,
        sig: "(fset SYMBOL DEFINITION)",
        doc: "Set SYMBOL's function cell to DEFINITION.",
    },
    Entry {
        name: "eval",
        kind: Kind::Function,
        sig: "(eval FORM &optional LEXICAL)",
        doc: "Evaluate FORM and return its value.",
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
    Entry {
        name: "string-match",
        kind: Kind::Function,
        sig: "(string-match REGEXP STRING &optional START)",
        doc: "Search STRING for REGEXP from START; set match data, return match index or nil.",
    },
    Entry {
        name: "string-match-p",
        kind: Kind::Function,
        sig: "(string-match-p REGEXP STRING &optional START)",
        doc: "Like string-match but preserve the existing match data.",
    },
    Entry {
        name: "match-beginning",
        kind: Kind::Function,
        sig: "(match-beginning N)",
        doc: "Char position where the Nth subexpression of the last match began.",
    },
    Entry {
        name: "match-end",
        kind: Kind::Function,
        sig: "(match-end N)",
        doc: "Char position where the Nth subexpression of the last match ended.",
    },
    Entry {
        name: "match-string",
        kind: Kind::Function,
        sig: "(match-string N &optional STRING)",
        doc: "Text matched by the Nth subexpression of the last match.",
    },
    Entry {
        name: "match-data",
        kind: Kind::Function,
        sig: "(match-data)",
        doc: "Last match's positions as a flat list (beg0 end0 beg1 end1 …).",
    },
    Entry {
        name: "set-match-data",
        kind: Kind::Function,
        sig: "(set-match-data LIST)",
        doc: "Restore match positions from a match-data list.",
    },
    Entry {
        name: "replace-regexp-in-string",
        kind: Kind::Function,
        sig: "(replace-regexp-in-string REGEXP REP STRING &optional FIXEDCASE LITERAL)",
        doc:
            "Replace every match of REGEXP in STRING with REP (\\& / \\N templates unless LITERAL).",
    },
    Entry {
        name: "regexp-quote",
        kind: Kind::Function,
        sig: "(regexp-quote STRING)",
        doc: "Return STRING with regexp metacharacters escaped to match literally.",
    },
    Entry {
        name: "save-match-data",
        kind: Kind::Function,
        sig: "(save-match-data &rest BODY)",
        doc: "Eval BODY, preserving the caller's regexp match data.",
    },
    Entry {
        name: "pcase",
        kind: Kind::Function,
        sig: "(pcase EXPR (PATTERN BODY...)...)",
        doc:
            "Structural dispatch: _, literals, 'x, binders, (pred FN), (guard E), (and …), (or …).",
    },
    Entry {
        name: "mod",
        kind: Kind::Function,
        sig: "(mod X Y)",
        doc: "X modulo Y, the remainder taking the sign of the divisor Y.",
    },
    Entry {
        name: "floor",
        kind: Kind::Function,
        sig: "(floor ARG &optional DIVISOR)",
        doc: "Largest integer less than or equal to ARG (or ARG/DIVISOR).",
    },
    Entry {
        name: "ceiling",
        kind: Kind::Function,
        sig: "(ceiling ARG &optional DIVISOR)",
        doc: "Smallest integer greater than or equal to ARG (or ARG/DIVISOR).",
    },
    Entry {
        name: "round",
        kind: Kind::Function,
        sig: "(round ARG &optional DIVISOR)",
        doc: "ARG (or ARG/DIVISOR) rounded to the nearest integer.",
    },
    Entry {
        name: "truncate",
        kind: Kind::Function,
        sig: "(truncate ARG &optional DIVISOR)",
        doc: "ARG (or ARG/DIVISOR) truncated toward zero to an integer.",
    },
    Entry {
        name: "float",
        kind: Kind::Function,
        sig: "(float ARG)",
        doc: "Convert ARG to a floating-point number.",
    },
    Entry {
        name: "logand",
        kind: Kind::Function,
        sig: "(logand &rest INTS)",
        doc: "Bitwise AND of the integer arguments.",
    },
    Entry {
        name: "logior",
        kind: Kind::Function,
        sig: "(logior &rest INTS)",
        doc: "Bitwise inclusive OR of the integer arguments.",
    },
    Entry {
        name: "logxor",
        kind: Kind::Function,
        sig: "(logxor &rest INTS)",
        doc: "Bitwise exclusive OR of the integer arguments.",
    },
    Entry {
        name: "lognot",
        kind: Kind::Function,
        sig: "(lognot NUMBER)",
        doc: "Bitwise NOT (one's complement) of NUMBER.",
    },
    Entry {
        name: "ash",
        kind: Kind::Function,
        sig: "(ash VALUE COUNT)",
        doc: "Arithmetic shift VALUE left COUNT bits (right if COUNT is negative).",
    },
    Entry {
        name: "lsh",
        kind: Kind::Function,
        sig: "(lsh VALUE COUNT)",
        doc: "Logical shift VALUE left COUNT bits (right if COUNT is negative).",
    },
    Entry {
        name: "expt",
        kind: Kind::Function,
        sig: "(expt BASE EXPONENT)",
        doc: "Return BASE raised to the power EXPONENT.",
    },
    Entry {
        name: "sqrt",
        kind: Kind::Function,
        sig: "(sqrt ARG)",
        doc: "Return the square root of ARG.",
    },
    Entry {
        name: "exp",
        kind: Kind::Function,
        sig: "(exp ARG)",
        doc: "Return e (the base of natural logarithms) raised to ARG.",
    },
    Entry {
        name: "log",
        kind: Kind::Function,
        sig: "(log ARG &optional BASE)",
        doc: "Natural logarithm of ARG, or the logarithm base BASE.",
    },
    Entry {
        name: "sin",
        kind: Kind::Function,
        sig: "(sin ARG)",
        doc: "Return the sine of ARG (in radians).",
    },
    Entry {
        name: "cos",
        kind: Kind::Function,
        sig: "(cos ARG)",
        doc: "Return the cosine of ARG (in radians).",
    },
    Entry {
        name: "tan",
        kind: Kind::Function,
        sig: "(tan ARG)",
        doc: "Return the tangent of ARG (in radians).",
    },
    Entry {
        name: "asin",
        kind: Kind::Function,
        sig: "(asin ARG)",
        doc: "Return the inverse sine of ARG, in radians.",
    },
    Entry {
        name: "acos",
        kind: Kind::Function,
        sig: "(acos ARG)",
        doc: "Return the inverse cosine of ARG, in radians.",
    },
    Entry {
        name: "atan",
        kind: Kind::Function,
        sig: "(atan Y &optional X)",
        doc: "Arc tangent of Y, or of Y/X using both signs to choose the quadrant.",
    },
    Entry {
        name: "ldexp",
        kind: Kind::Function,
        sig: "(ldexp SGNFCAND EXPONENT)",
        doc: "Return SGNFCAND * 2**EXPONENT, as a float.",
    },
    Entry {
        name: "copysign",
        kind: Kind::Function,
        sig: "(copysign X1 X2)",
        doc: "Return X1 with the sign of X2.",
    },
    Entry {
        name: "frexp",
        kind: Kind::Function,
        sig: "(frexp X)",
        doc: "Return (SIGNIFICAND . EXPONENT) with X = SIGNIFICAND * 2**EXPONENT and 0.5 <= |SIGNIFICAND| < 1.",
    },
    Entry {
        name: "isnan",
        kind: Kind::Function,
        sig: "(isnan X)",
        doc: "Non-nil if the float X is a NaN.",
    },
    Entry {
        name: "fround",
        kind: Kind::Function,
        sig: "(fround X)",
        doc: "Round X to the nearest integer, returned as a float.",
    },
    Entry {
        name: "ffloor",
        kind: Kind::Function,
        sig: "(ffloor X)",
        doc: "Largest integer value not greater than X, returned as a float.",
    },
    Entry {
        name: "fceiling",
        kind: Kind::Function,
        sig: "(fceiling X)",
        doc: "Smallest integer value not less than X, returned as a float.",
    },
    Entry {
        name: "ftruncate",
        kind: Kind::Function,
        sig: "(ftruncate X)",
        doc: "Truncate X toward zero to an integer value, returned as a float.",
    },
    Entry {
        name: "abs",
        kind: Kind::Function,
        sig: "(abs ARG)",
        doc: "Return the absolute value of ARG.",
    },
    Entry {
        name: "logcount",
        kind: Kind::Function,
        sig: "(logcount VALUE)",
        doc: "Number of one bits in VALUE (or zero bits if VALUE is negative).",
    },
    Entry {
        name: "logb",
        kind: Kind::Function,
        sig: "(logb ARG)",
        doc: "Return the binary exponent of ARG: the floor of log base 2 of |ARG|.",
    },
    Entry {
        name: "random",
        kind: Kind::Function,
        sig: "(random &optional LIMIT)",
        doc: "Pseudo-random integer; if LIMIT is a positive integer, in the range [0, LIMIT).",
    },
    Entry {
        name: "string-to-number",
        kind: Kind::Function,
        sig: "(string-to-number STRING &optional BASE)",
        doc: "Parse the number at the front of STRING (radix BASE, default 10).",
    },
    Entry {
        name: "type-of",
        kind: Kind::Function,
        sig: "(type-of OBJECT)",
        doc: "Return a symbol naming the primitive type of OBJECT.",
    },
    Entry {
        name: "recordp",
        kind: Kind::Function,
        sig: "(recordp OBJECT)",
        doc: "Non-nil if OBJECT is a record.",
    },
    Entry {
        name: "cl-struct-p",
        kind: Kind::Function,
        sig: "(cl-struct-p OBJECT)",
        doc: "Non-nil if OBJECT is a record (a cl-defstruct instance).",
    },
    Entry {
        name: "functionp",
        kind: Kind::Function,
        sig: "(functionp OBJECT)",
        doc: "Non-nil if OBJECT is callable as a function.",
    },
    Entry {
        name: "char-or-string-p",
        kind: Kind::Function,
        sig: "(char-or-string-p OBJECT)",
        doc: "Non-nil if OBJECT is a character (integer) or a string.",
    },
    Entry {
        name: "subrp",
        kind: Kind::Function,
        sig: "(subrp OBJECT)",
        doc: "Non-nil if OBJECT is a built-in (primitive) function.",
    },
    Entry {
        name: "macrop",
        kind: Kind::Function,
        sig: "(macrop OBJECT)",
        doc: "Non-nil if OBJECT is a macro.",
    },
    Entry {
        name: "special-form-p",
        kind: Kind::Function,
        sig: "(special-form-p OBJECT)",
        doc: "Non-nil if OBJECT is a special form.",
    },
    Entry {
        name: "char-uppercase-p",
        kind: Kind::Function,
        sig: "(char-uppercase-p CHAR)",
        doc: "Non-nil if CHAR is an uppercase character.",
    },
    Entry {
        name: "special-variable-p",
        kind: Kind::Function,
        sig: "(special-variable-p SYMBOL)",
        doc: "Non-nil if SYMBOL is a special (dynamically scoped) variable.",
    },
    Entry {
        name: "char-equal",
        kind: Kind::Function,
        sig: "(char-equal C1 C2)",
        doc: "Non-nil if characters C1 and C2 are equal (case-insensitive when case-fold-search).",
    },
    Entry {
        name: "makunbound",
        kind: Kind::Function,
        sig: "(makunbound SYMBOL)",
        doc: "Make SYMBOL's value cell void; return SYMBOL.",
    },
    Entry {
        name: "intern-soft",
        kind: Kind::Function,
        sig: "(intern-soft NAME)",
        doc: "Return the interned symbol named NAME, or nil if none exists.",
    },
    Entry {
        name: "symbol-function",
        kind: Kind::Function,
        sig: "(symbol-function SYMBOL)",
        doc: "Return SYMBOL's function definition, or nil if it has none.",
    },
    Entry {
        name: "indirect-function",
        kind: Kind::Function,
        sig: "(indirect-function OBJECT &optional NOERROR)",
        doc: "Chase symbol function cells from OBJECT to the underlying function.",
    },
    Entry {
        name: "defvaralias",
        kind: Kind::Function,
        sig: "(defvaralias NEW-ALIAS BASE-VARIABLE &optional DOCSTRING)",
        doc: "Make NEW-ALIAS a variable alias for BASE-VARIABLE: value operations on either affect both. Returns BASE-VARIABLE.",
    },
    Entry {
        name: "indirect-variable",
        kind: Kind::Function,
        sig: "(indirect-variable OBJECT)",
        doc: "Follow the `defvaralias' chain from OBJECT to the base variable symbol, or return OBJECT if it is not a symbol.",
    },
    Entry {
        name: "func-arity",
        kind: Kind::Function,
        sig: "(func-arity FUNCTION)",
        doc: "Return (MIN . MAX) giving FUNCTION's minimum and maximum argument counts (MAX may be `many').",
    },
    Entry {
        name: "subr-arity",
        kind: Kind::Function,
        sig: "(subr-arity SUBR)",
        doc: "Return (MIN . MAX), the minimum and maximum argument counts of built-in SUBR.",
    },
    Entry {
        name: "throw",
        kind: Kind::Function,
        sig: "(throw TAG VALUE)",
        doc: "Throw to the `catch' for TAG, returning VALUE from it.",
    },
    Entry {
        name: "error",
        kind: Kind::Function,
        sig: "(error FORMAT &rest ARGS)",
        doc: "Signal an error whose message is FORMAT formatted with ARGS.",
    },
    Entry {
        name: "user-error",
        kind: Kind::Function,
        sig: "(user-error FORMAT &rest ARGS)",
        doc: "Signal a user error (a mistake, not a bug) with a formatted message.",
    },
    Entry {
        name: "signal",
        kind: Kind::Function,
        sig: "(signal ERROR-SYMBOL DATA)",
        doc: "Signal the error named ERROR-SYMBOL with associated DATA.",
    },
    Entry {
        name: "read",
        kind: Kind::Function,
        sig: "(read STRING)",
        doc: "Read and return one Lisp object from STRING.",
    },
    Entry {
        name: "read-from-string",
        kind: Kind::Function,
        sig: "(read-from-string STRING &optional START END)",
        doc: "Read one object from STRING; return (OBJECT . NEXT-INDEX).",
    },
    Entry {
        name: "terpri",
        kind: Kind::Function,
        sig: "(terpri &optional STREAM)",
        doc: "Output a newline.",
    },
    Entry {
        name: "print",
        kind: Kind::Function,
        sig: "(print OBJECT &optional STREAM)",
        doc: "Output OBJECT in read syntax, surrounded by newlines; return OBJECT.",
    },
    Entry {
        name: "prin1-to-string",
        kind: Kind::Function,
        sig: "(prin1-to-string OBJECT)",
        doc: "Return a string of OBJECT printed in read syntax.",
    },
    Entry {
        name: "substring",
        kind: Kind::Function,
        sig: "(substring STRING &optional FROM TO)",
        doc: "Return the substring of STRING from index FROM to TO (negative indices count from the end).",
    },
    Entry {
        name: "split-string",
        kind: Kind::Function,
        sig: "(split-string STRING &optional SEPARATORS OMIT-NULLS TRIM)",
        doc: "Split STRING into a list of substrings around matches of SEPARATORS.",
    },
    Entry {
        name: "string-prefix-p",
        kind: Kind::Function,
        sig: "(string-prefix-p PREFIX STRING &optional IGNORE-CASE)",
        doc: "Non-nil if PREFIX is a prefix of STRING.",
    },
    Entry {
        name: "string-suffix-p",
        kind: Kind::Function,
        sig: "(string-suffix-p SUFFIX STRING &optional IGNORE-CASE)",
        doc: "Non-nil if SUFFIX is a suffix of STRING.",
    },
    Entry {
        name: "string-empty-p",
        kind: Kind::Function,
        sig: "(string-empty-p STRING)",
        doc: "Non-nil if STRING is the empty string.",
    },
    Entry {
        name: "string-join",
        kind: Kind::Function,
        sig: "(string-join STRINGS &optional SEPARATOR)",
        doc: "Concatenate the list STRINGS, inserting SEPARATOR between elements.",
    },
    Entry {
        name: "char-to-string",
        kind: Kind::Function,
        sig: "(char-to-string CHAR)",
        doc: "Return a one-character string containing CHAR.",
    },
    Entry {
        name: "string-to-char",
        kind: Kind::Function,
        sig: "(string-to-char STRING)",
        doc: "Return the first character of STRING as an integer (0 if empty).",
    },
    Entry {
        name: "make-string",
        kind: Kind::Function,
        sig: "(make-string COUNT CHARACTER &optional MULTIBYTE)",
        doc: "Return a string of COUNT copies of CHARACTER.",
    },
    Entry {
        name: "string",
        kind: Kind::Function,
        sig: "(string &rest CHARACTERS)",
        doc: "Return a string made from the character arguments.",
    },
    Entry {
        name: "string-search",
        kind: Kind::Function,
        sig: "(string-search NEEDLE HAYSTACK &optional START-POS)",
        doc: "Index of the first occurrence of NEEDLE in HAYSTACK (from START-POS), or nil.",
    },
    Entry {
        name: "downcase",
        kind: Kind::Function,
        sig: "(downcase OBJ)",
        doc: "Return OBJ (a string or character) converted to lower case.",
    },
    Entry {
        name: "upcase",
        kind: Kind::Function,
        sig: "(upcase OBJ)",
        doc: "Return OBJ (a string or character) converted to upper case.",
    },
    Entry {
        name: "compare-strings",
        kind: Kind::Function,
        sig: "(compare-strings STR1 START1 END1 STR2 START2 END2 &optional IGNORE-CASE)",
        doc: "Compare the specified substrings; t if equal, else a signed mismatch position.",
    },
    Entry {
        name: "string-distance",
        kind: Kind::Function,
        sig: "(string-distance STRING1 STRING2 &optional BYTECOMPARE)",
        doc: "Levenshtein edit distance between STRING1 and STRING2.",
    },
    Entry {
        name: "fillarray",
        kind: Kind::Function,
        sig: "(fillarray ARRAY ITEM)",
        doc: "Set every element of ARRAY to ITEM; return ARRAY.",
    },
    Entry {
        name: "vconcat",
        kind: Kind::Function,
        sig: "(vconcat &rest SEQUENCES)",
        doc: "Concatenate the SEQUENCES into a single new vector.",
    },
    Entry {
        name: "string-to-vector",
        kind: Kind::Function,
        sig: "(string-to-vector STRING)",
        doc: "Return a vector of the character codes in STRING.",
    },
    Entry {
        name: "string-to-list",
        kind: Kind::Function,
        sig: "(string-to-list STRING)",
        doc: "Return a list of the character codes in STRING.",
    },
    Entry {
        name: "make-hash-table",
        kind: Kind::Function,
        sig: "(make-hash-table &rest KEYWORD-ARGS)",
        doc: "Create and return a new empty hash table (accepts :test, :size, etc.).",
    },
    Entry {
        name: "gethash",
        kind: Kind::Function,
        sig: "(gethash KEY TABLE &optional DEFAULT)",
        doc: "Return the value for KEY in TABLE, or DEFAULT if absent.",
    },
    Entry {
        name: "puthash",
        kind: Kind::Function,
        sig: "(puthash KEY VALUE TABLE)",
        doc: "Associate KEY with VALUE in TABLE; return VALUE.",
    },
    Entry {
        name: "remhash",
        kind: Kind::Function,
        sig: "(remhash KEY TABLE)",
        doc: "Remove KEY and its value from TABLE.",
    },
    Entry {
        name: "clrhash",
        kind: Kind::Function,
        sig: "(clrhash TABLE)",
        doc: "Remove all entries from TABLE; return it.",
    },
    Entry {
        name: "hash-table-count",
        kind: Kind::Function,
        sig: "(hash-table-count TABLE)",
        doc: "Return the number of entries in TABLE.",
    },
    Entry {
        name: "hash-table-test",
        kind: Kind::Function,
        sig: "(hash-table-test TABLE)",
        doc: "Return the test function symbol used by TABLE.",
    },
    Entry {
        name: "hash-table-p",
        kind: Kind::Function,
        sig: "(hash-table-p OBJECT)",
        doc: "Non-nil if OBJECT is a hash table.",
    },
    Entry {
        name: "hash-table-keys",
        kind: Kind::Function,
        sig: "(hash-table-keys TABLE)",
        doc: "Return a list of all keys in TABLE.",
    },
    Entry {
        name: "hash-table-values",
        kind: Kind::Function,
        sig: "(hash-table-values TABLE)",
        doc: "Return a list of all values in TABLE.",
    },
    Entry {
        name: "copy-hash-table",
        kind: Kind::Function,
        sig: "(copy-hash-table TABLE)",
        doc: "Return a shallow copy of TABLE.",
    },
    Entry {
        name: "sha1",
        kind: Kind::Function,
        sig: "(sha1 OBJECT &optional START END BINARY)",
        doc: "Return the SHA-1 hash of OBJECT (a string or buffer) as a hex string.",
    },
    Entry {
        name: "md5",
        kind: Kind::Function,
        sig: "(md5 OBJECT &optional START END CODING-SYSTEM NOERROR)",
        doc: "Return the MD5 hash of OBJECT as a hex string.",
    },
    Entry {
        name: "secure-hash",
        kind: Kind::Function,
        sig: "(secure-hash ALGORITHM OBJECT &optional START END BINARY)",
        doc: "Return the ALGORITHM (md5/sha1/sha224/sha256/sha384/sha512) hash of OBJECT.",
    },
    Entry {
        name: "base64-encode-string",
        kind: Kind::Function,
        sig: "(base64-encode-string STRING &optional NO-LINE-BREAK)",
        doc: "Return the base64 encoding of STRING.",
    },
    Entry {
        name: "base64-decode-string",
        kind: Kind::Function,
        sig: "(base64-decode-string STRING &optional BASE64URL IGNORE-INVALID)",
        doc: "Return the contents of base64-encoded STRING decoded.",
    },
    Entry {
        name: "base64url-encode-string",
        kind: Kind::Function,
        sig: "(base64url-encode-string STRING &optional NO-PAD)",
        doc: "Return the base64url (URL-safe) encoding of STRING.",
    },
    Entry {
        name: "base64url-decode-string",
        kind: Kind::Function,
        sig: "(base64url-decode-string STRING &optional IGNORE-INVALID)",
        doc: "Decode a base64url (URL-safe) encoded STRING.",
    },
    Entry {
        name: "url-hexify-string",
        kind: Kind::Function,
        sig: "(url-hexify-string STRING &optional ALLOWED-CHARS)",
        doc: "Percent-encode (URL-escape) the characters of STRING.",
    },
    Entry {
        name: "url-unhex-string",
        kind: Kind::Function,
        sig: "(url-unhex-string STRING &optional ALLOW-NEWLINES)",
        doc: "Decode the %-escaped sequences in STRING.",
    },
    Entry {
        name: "sxhash",
        kind: Kind::Function,
        sig: "(sxhash OBJ)",
        doc: "Return an `equal'-based hash code for OBJ (alias of sxhash-equal).",
    },
    Entry {
        name: "sxhash-equal",
        kind: Kind::Function,
        sig: "(sxhash-equal OBJ)",
        doc: "Return a hash code for OBJ such that `equal' objects hash alike.",
    },
    Entry {
        name: "sxhash-eq",
        kind: Kind::Function,
        sig: "(sxhash-eq OBJ)",
        doc: "Return a hash code for OBJ such that `eq' objects hash alike.",
    },
    Entry {
        name: "sxhash-eql",
        kind: Kind::Function,
        sig: "(sxhash-eql OBJ)",
        doc: "Return a hash code for OBJ such that `eql' objects hash alike.",
    },
    Entry {
        name: "getenv",
        kind: Kind::Function,
        sig: "(getenv VARIABLE &optional FRAME)",
        doc: "Return the value of environment VARIABLE as a string, or nil.",
    },
    Entry {
        name: "setenv",
        kind: Kind::Function,
        sig: "(setenv VARIABLE &optional VALUE SUBSTITUTE-ENV-VARS)",
        doc: "Set environment VARIABLE to VALUE (unset it if VALUE is nil).",
    },
    Entry {
        name: "--current-directory--",
        kind: Kind::Function,
        sig: "(--current-directory--)",
        doc: "Internal: the process working directory as a string (backs `default-directory').",
    },
    Entry {
        name: "file-exists-p",
        kind: Kind::Function,
        sig: "(file-exists-p FILENAME)",
        doc: "Non-nil if FILENAME names an existing file.",
    },
    Entry {
        name: "file-directory-p",
        kind: Kind::Function,
        sig: "(file-directory-p FILENAME)",
        doc: "Non-nil if FILENAME names an existing directory.",
    },
    Entry {
        name: "file-regular-p",
        kind: Kind::Function,
        sig: "(file-regular-p FILENAME)",
        doc: "Non-nil if FILENAME names a regular file.",
    },
    Entry {
        name: "file-readable-p",
        kind: Kind::Function,
        sig: "(file-readable-p FILENAME)",
        doc: "Non-nil if FILENAME names a file you can read.",
    },
    Entry {
        name: "file-writable-p",
        kind: Kind::Function,
        sig: "(file-writable-p FILENAME)",
        doc: "Non-nil if FILENAME names a file you can write (or create).",
    },
    Entry {
        name: "file-symlink-p",
        kind: Kind::Function,
        sig: "(file-symlink-p FILENAME)",
        doc: "If FILENAME is a symbolic link, return its target; else nil.",
    },
    Entry {
        name: "file-executable-p",
        kind: Kind::Function,
        sig: "(file-executable-p FILENAME)",
        doc: "Non-nil if FILENAME names a file you can execute (search permission for a directory).",
    },
    Entry {
        name: "--invocation-file--",
        kind: Kind::Function,
        sig: "(--invocation-file--)",
        doc: "Internal: absolute path of the running `elisp' binary, backing `invocation-name', `invocation-directory', and `exec-directory'.",
    },
    Entry {
        name: "--directory-files--",
        kind: Kind::Function,
        sig: "(--directory-files-- DIRECTORY &optional MATCH-REGEXP NOSORT)",
        doc: "Internal primitive behind `directory-files': names in DIRECTORY, optionally MATCH-filtered and sorted.",
    },
    Entry {
        name: "write-region",
        kind: Kind::Function,
        sig: "(write-region START END FILENAME &optional APPEND VISIT LOCKNAME MUSTBENEW)",
        doc: "Write the buffer text between START and END to FILENAME.",
    },
    Entry {
        name: "delete-file",
        kind: Kind::Function,
        sig: "(delete-file FILENAME &optional TRASH)",
        doc: "Delete the file named FILENAME.",
    },
    Entry {
        name: "make-directory",
        kind: Kind::Function,
        sig: "(make-directory DIR &optional PARENTS)",
        doc: "Create the directory DIR (and parents when PARENTS is non-nil).",
    },
    Entry {
        name: "rename-file",
        kind: Kind::Function,
        sig: "(rename-file FILE NEWNAME &optional OK-IF-ALREADY-EXISTS)",
        doc: "Rename FILE to NEWNAME.",
    },
    Entry {
        name: "copy-file",
        kind: Kind::Function,
        sig: "(copy-file FILE NEWNAME &optional OK-IF-ALREADY-EXISTS KEEP-TIME PRESERVE-UID-GID PRESERVE-PERMISSIONS)",
        doc: "Copy FILE to NEWNAME.",
    },
    Entry {
        name: "insert-file-contents",
        kind: Kind::Function,
        sig: "(insert-file-contents FILENAME &optional VISIT BEG END REPLACE)",
        doc: "Insert the contents of FILENAME into the current buffer at point.",
    },
    Entry {
        name: "shell-command-to-string",
        kind: Kind::Function,
        sig: "(shell-command-to-string COMMAND)",
        doc: "Run shell COMMAND and return its standard output as a string.",
    },
    Entry {
        name: "call-process",
        kind: Kind::Function,
        sig: "(call-process PROGRAM &optional INFILE DESTINATION DISPLAY &rest ARGS)",
        doc: "Run PROGRAM synchronously with ARGS; return its exit status.",
    },
    Entry {
        name: "process-lines",
        kind: Kind::Function,
        sig: "(process-lines PROGRAM &rest ARGS)",
        doc: "Run PROGRAM with ARGS and return its output as a list of lines.",
    },
    Entry {
        name: "insert",
        kind: Kind::Function,
        sig: "(insert &rest ARGS)",
        doc: "Insert the ARGS (strings or characters) into the buffer at point.",
    },
    Entry {
        name: "buffer-string",
        kind: Kind::Function,
        sig: "(buffer-string)",
        doc: "Return the entire contents of the current buffer as a string.",
    },
    Entry {
        name: "buffer-size",
        kind: Kind::Function,
        sig: "(buffer-size &optional BUFFER)",
        doc: "Return the number of characters in the current buffer.",
    },
    Entry {
        name: "point",
        kind: Kind::Function,
        sig: "(point)",
        doc: "Return the value of point, as an integer (1-based).",
    },
    Entry {
        name: "point-min",
        kind: Kind::Function,
        sig: "(point-min)",
        doc: "Return the minimum accessible buffer position.",
    },
    Entry {
        name: "point-max",
        kind: Kind::Function,
        sig: "(point-max)",
        doc: "Return the maximum accessible buffer position.",
    },
    Entry {
        name: "goto-char",
        kind: Kind::Function,
        sig: "(goto-char POSITION)",
        doc: "Set point to POSITION in the current buffer.",
    },
    Entry {
        name: "erase-buffer",
        kind: Kind::Function,
        sig: "(erase-buffer)",
        doc: "Delete the entire contents of the current buffer.",
    },
    Entry {
        name: "char-after",
        kind: Kind::Function,
        sig: "(char-after &optional POS)",
        doc: "Return the character after POS (point by default), or nil.",
    },
    Entry {
        name: "buffer-substring",
        kind: Kind::Function,
        sig: "(buffer-substring START END)",
        doc: "Return the buffer text between positions START and END as a string.",
    },
    Entry {
        name: "buffer-substring-no-properties",
        kind: Kind::Function,
        sig: "(buffer-substring-no-properties START END)",
        doc: "Return the buffer text between START and END, with no text properties.",
    },
    Entry {
        name: "delete-region",
        kind: Kind::Function,
        sig: "(delete-region START END)",
        doc: "Delete the buffer text between positions START and END.",
    },
    Entry {
        name: "forward-char",
        kind: Kind::Function,
        sig: "(forward-char &optional N)",
        doc: "Move point N characters forward (default 1).",
    },
    Entry {
        name: "backward-char",
        kind: Kind::Function,
        sig: "(backward-char &optional N)",
        doc: "Move point N characters backward (default 1).",
    },
    Entry {
        name: "beginning-of-line",
        kind: Kind::Function,
        sig: "(beginning-of-line &optional N)",
        doc: "Move point to the beginning of the current line.",
    },
    Entry {
        name: "end-of-line",
        kind: Kind::Function,
        sig: "(end-of-line &optional N)",
        doc: "Move point to the end of the current line.",
    },
    Entry {
        name: "line-beginning-position",
        kind: Kind::Function,
        sig: "(line-beginning-position &optional N)",
        doc: "Return the position of the beginning of the current line.",
    },
    Entry {
        name: "line-end-position",
        kind: Kind::Function,
        sig: "(line-end-position &optional N)",
        doc: "Return the position of the end of the current line.",
    },
    Entry {
        name: "pos-bol",
        kind: Kind::Function,
        sig: "(pos-bol &optional N)",
        doc: "Return the position of the beginning of the current line (like line-beginning-position).",
    },
    Entry {
        name: "pos-eol",
        kind: Kind::Function,
        sig: "(pos-eol &optional N)",
        doc: "Return the position of the end of the current line (like line-end-position).",
    },
    Entry {
        name: "bolp",
        kind: Kind::Function,
        sig: "(bolp)",
        doc: "Non-nil if point is at the beginning of a line.",
    },
    Entry {
        name: "eolp",
        kind: Kind::Function,
        sig: "(eolp)",
        doc: "Non-nil if point is at the end of a line.",
    },
    Entry {
        name: "bobp",
        kind: Kind::Function,
        sig: "(bobp)",
        doc: "Non-nil if point is at the beginning of the buffer.",
    },
    Entry {
        name: "eobp",
        kind: Kind::Function,
        sig: "(eobp)",
        doc: "Non-nil if point is at the end of the buffer.",
    },
    Entry {
        name: "forward-line",
        kind: Kind::Function,
        sig: "(forward-line &optional N)",
        doc: "Move point N lines forward; return the count of lines left unmoved.",
    },
    Entry {
        name: "search-forward",
        kind: Kind::Function,
        sig: "(search-forward STRING &optional BOUND NOERROR COUNT)",
        doc: "Search forward for STRING, leaving point after it; return the new point.",
    },
    Entry {
        name: "re-search-forward",
        kind: Kind::Function,
        sig: "(re-search-forward REGEXP &optional BOUND NOERROR COUNT)",
        doc: "Search forward for a match of REGEXP, leaving point after it.",
    },
    Entry {
        name: "looking-at",
        kind: Kind::Function,
        sig: "(looking-at REGEXP &optional INHIBIT-MODIFY)",
        doc: "Non-nil if text after point matches REGEXP; sets the match data.",
    },
    Entry {
        name: "looking-at-p",
        kind: Kind::Function,
        sig: "(looking-at-p REGEXP)",
        doc: "Non-nil if text after point matches REGEXP, without changing the match data.",
    },
    Entry {
        name: "replace-match",
        kind: Kind::Function,
        sig: "(replace-match NEWTEXT &optional FIXEDCASE LITERAL STRING SUBEXP)",
        doc: "Replace the text matched by the last search with NEWTEXT.",
    },
    Entry {
        name: "char-before",
        kind: Kind::Function,
        sig: "(char-before &optional POS)",
        doc: "Return the character before POS (point by default), or nil.",
    },
    Entry {
        name: "delete-char",
        kind: Kind::Function,
        sig: "(delete-char N &optional KILLFLAG)",
        doc: "Delete N characters forward from point (backward if N is negative).",
    },
    Entry {
        name: "insert-char",
        kind: Kind::Function,
        sig: "(insert-char CHARACTER &optional COUNT INHERIT)",
        doc: "Insert COUNT copies of CHARACTER at point.",
    },
    Entry {
        name: "count-lines",
        kind: Kind::Function,
        sig: "(count-lines START END)",
        doc: "Return the number of lines between positions START and END.",
    },
    Entry {
        name: "line-number-at-pos",
        kind: Kind::Function,
        sig: "(line-number-at-pos &optional POS ABSOLUTE)",
        doc: "Return the line number of POS (point by default) in the current buffer.",
    },
    Entry {
        name: "current-column",
        kind: Kind::Function,
        sig: "(current-column)",
        doc: "Return the horizontal column position of point.",
    },
    Entry {
        name: "search-backward",
        kind: Kind::Function,
        sig: "(search-backward STRING &optional BOUND NOERROR COUNT)",
        doc: "Search backward for STRING, leaving point before it.",
    },
    Entry {
        name: "re-search-backward",
        kind: Kind::Function,
        sig: "(re-search-backward REGEXP &optional BOUND NOERROR COUNT)",
        doc: "Search backward for a match of REGEXP, leaving point before it.",
    },
    Entry {
        name: "skip-chars-forward",
        kind: Kind::Function,
        sig: "(skip-chars-forward STRING &optional LIM)",
        doc: "Move point forward over characters in the set STRING; return the distance moved.",
    },
    Entry {
        name: "skip-chars-backward",
        kind: Kind::Function,
        sig: "(skip-chars-backward STRING &optional LIM)",
        doc: "Move point backward over characters in the set STRING; return the distance moved.",
    },
    Entry {
        name: "forward-word",
        kind: Kind::Function,
        sig: "(forward-word &optional N)",
        doc: "Move point forward N words (default 1).",
    },
    Entry {
        name: "backward-word",
        kind: Kind::Function,
        sig: "(backward-word &optional N)",
        doc: "Move point backward N words (default 1).",
    },
    Entry {
        name: "--push-output-capture--",
        kind: Kind::Function,
        sig: "(--push-output-capture--)",
        doc: "Internal: start redirecting princ/prin1/print output into a capture buffer.",
    },
    Entry {
        name: "--pop-output-capture--",
        kind: Kind::Function,
        sig: "(--pop-output-capture--)",
        doc: "Internal: stop capturing output and return the captured string.",
    },
    // AOP pattern-intercept layer (elisprs extension, ported from zshrs). Glob
    // advice across many function names at once — distinct from nadvice.
    Entry {
        name: "intercept",
        kind: Kind::Function,
        sig: "(intercept KIND PATTERN FORM)",
        doc: "Register AOP advice. KIND is before/after/around; PATTERN is a glob (\"forward-*\", \"_*\", \"all\") matched against function names; FORM is the (quoted) advice form. Returns the intercept ID. Distinct from nadvice's advice-add: one registration fires on every matching name.",
    },
    Entry {
        name: "intercept-list",
        kind: Kind::Function,
        sig: "(intercept-list)",
        doc: "Return registered intercepts as a list of (ID KIND PATTERN FORM) entries, or nil.",
    },
    Entry {
        name: "intercept-remove",
        kind: Kind::Function,
        sig: "(intercept-remove ID)",
        doc: "Remove the intercept with integer ID. Returns t if removed, nil otherwise.",
    },
    Entry {
        name: "intercept-clear",
        kind: Kind::Function,
        sig: "(intercept-clear)",
        doc: "Remove all intercepts. Returns the count removed.",
    },
    Entry {
        name: "intercept-proceed",
        kind: Kind::Function,
        sig: "(intercept-proceed)",
        doc: "From an around advice, run the original function and return its value. Bound context in advice: intercept-name, intercept-args, intercept-cmd, and (after) intercept-ms/intercept-us.",
    },
];

pub fn lookup(name: &str) -> Option<&'static Entry> {
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

/// Guard against leaking as an orphan LSP process. Editors / GUI apps reap us via
/// kill-on-drop, which only fires on a *graceful* client exit; a hard client
/// death (SIGKILL/crash/force-quit) skips it, and a leaked pipe fd can keep our
/// stdin open so we never see EOF either. Watch for reparenting to pid 1 (our
/// client died) and exit — nothing this read-only server holds is worth leaking.
fn spawn_orphan_guard() {
    std::thread::spawn(|| {
        // Linux: also ask the kernel to SIGKILL us the instant the parent dies.
        // Best-effort; the getppid poll below is the portable guarantee (macOS
        // has no PDEATHSIG).
        #[cfg(target_os = "linux")]
        // SAFETY: prctl(PR_SET_PDEATHSIG, ...) only registers a signal disposition.
        unsafe {
            libc::prctl(
                libc::PR_SET_PDEATHSIG,
                libc::SIGKILL as libc::c_ulong,
                0,
                0,
                0,
            );
        }
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            // SAFETY: getppid takes no arguments and never fails.
            if unsafe { libc::getppid() } == 1 {
                std::process::exit(0);
            }
        }
    });
}

fn run() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    spawn_orphan_guard();
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

/// Every metadata entry the language knows: the special forms the compiler
/// recognizes followed by the installed subrs. Single source of truth for both
/// the LSP completion list (which keeps the full sig/doc detail) and the REPL
/// wordlist (`completion_words`, bare names only).
pub fn all_entries() -> impl Iterator<Item = &'static Entry> {
    SPECIAL_FORMS.iter().chain(SUBRS)
}

/// The merged, sorted, de-duplicated bare names of every special form + subr —
/// the wordlist the reedline REPL feeds its Tab completer. Shares `all_entries`
/// with the LSP `completion()` path so the two never drift.
pub fn completion_words() -> Vec<String> {
    let mut v: Vec<String> = all_entries().map(|e| e.name.to_string()).collect();
    v.sort();
    v.dedup();
    v
}

fn completion() -> CompletionResponse {
    let items = all_entries()
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
