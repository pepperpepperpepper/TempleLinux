#[path = "02_fmt.rs"]
mod fmt;

#[path = "03_preprocess.rs"]
mod preprocess;

#[path = "04_cli.rs"]
mod cli;

#[path = "vm/mod.rs"]
mod vm;

#[cfg(test)]
#[path = "05_tests.rs"]
mod hc_tests;

pub(super) fn run() -> std::io::Result<()> {
    cli::run()
}

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

#[derive(Clone, Debug)]
struct Span {
    file: Arc<str>,
    line: usize,
    col: usize,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    span: Span,
}

#[derive(Clone, Debug)]
enum TokenKind {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Char(u64),
    DolDocCmd(String),
    Sym(Sym),
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sym {
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Dot,
    Assign,
    PlusAssign,
    Plus,
    PlusPlus,
    MinusAssign,
    Minus,
    MinusMinus,
    Arrow,
    StarAssign,
    Star,
    SlashAssign,
    Slash,
    PercentAssign,
    Percent,
    Bang,
    EqEq,
    NotEq,
    Lt,
    Le,
    ShlAssign,
    Shl,
    Gt,
    Ge,
    ShrAssign,
    Shr,
    AmpersandAssign,
    Ampersand,
    AndAnd,
    PipeAssign,
    Pipe,
    OrOr,
    CaretAssign,
    Caret,
    Tilde,
}

#[derive(Debug)]
struct ParseError {
    span: Span,
    msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.span.file, self.span.line, self.span.col, self.msg
        )
    }
}

impl std::error::Error for ParseError {}

fn is_type_name(s: &str) -> bool {
    matches!(
        s,
        "U0" | "U8" | "U16" | "U32" | "U64" | "I8" | "I16" | "I32" | "I64" | "F32" | "F64" | "Bool"
    )
}

fn is_user_type_name(s: &str) -> bool {
    s.starts_with('C')
}

struct Lexer<'a> {
    file: Arc<str>,
    input: &'a [u8],
    idx: usize,
    line: usize,
    col: usize,
    macros: Arc<HashMap<String, String>>,
    macro_queue: VecDeque<Token>,
    temple_file_path: Option<String>,
    temple_file_path_ready: bool,
}

impl<'a> Lexer<'a> {
    fn new(
        file: Arc<str>,
        src: &'a [u8],
        start_line: usize,
        macros: Arc<HashMap<String, String>>,
    ) -> Self {
        Self {
            file,
            input: src,
            idx: 0,
            line: start_line,
            col: 1,
            macros,
            macro_queue: VecDeque::new(),
            temple_file_path: None,
            temple_file_path_ready: false,
        }
    }

    fn compute_temple_file_path(label: &str) -> Option<String> {
        if label.starts_with('<') {
            return None;
        }

        let path = Path::new(label);
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        let mut roots: Vec<PathBuf> = Vec::new();
        if let Ok(v) = std::env::var("TEMPLE_ROOT") {
            let v = v.trim();
            if !v.is_empty() {
                roots.push(PathBuf::from(v));
            }
        }
        if let Ok(v) = std::env::var("TEMPLEOS_ROOT") {
            let v = v.trim();
            if !v.is_empty() {
                roots.push(PathBuf::from(v));
            }
        } else if let Some(root) = preprocess::discover_templeos_root() {
            roots.push(root);
        }

        for root in roots {
            let root = std::fs::canonicalize(&root).unwrap_or(root);
            if let Ok(rel) = path.strip_prefix(&root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                let rel = rel.trim_start_matches('/');
                return Some(if rel.is_empty() {
                    "/".to_string()
                } else {
                    format!("/{rel}")
                });
            }
        }

        None
    }

    fn temple_file_path(&mut self) -> Option<&str> {
        if !self.temple_file_path_ready {
            self.temple_file_path = Self::compute_temple_file_path(self.file.as_ref());
            self.temple_file_path_ready = true;
        }
        self.temple_file_path.as_deref()
    }

    fn temple_file_dir(&mut self) -> Option<String> {
        let Some(file) = self.temple_file_path() else {
            return None;
        };
        let file = file.trim_end_matches('/');
        let Some(idx) = file.rfind('/') else {
            return Some("/".to_string());
        };
        if idx == 0 {
            return Some("/".to_string());
        }
        Some(file[..idx].to_string())
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.idx).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.input.get(self.idx + 1).copied()
    }

    fn peek3(&self) -> Option<u8> {
        self.input.get(self.idx + 2).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.idx += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    fn span(&self) -> Span {
        Span {
            file: self.file.clone(),
            line: self.line,
            col: self.col,
        }
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), ParseError> {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.bump();
            }

            let Some(b'/') = self.peek() else {
                return Ok(());
            };

            match self.peek2() {
                Some(b'/') => {
                    self.bump();
                    self.bump();
                    while let Some(c) = self.peek() {
                        self.bump();
                        if c == b'\n' {
                            break;
                        }
                    }
                }
                Some(b'*') => {
                    let start = self.span();
                    self.bump();
                    self.bump();
                    loop {
                        match (self.peek(), self.peek2()) {
                            (Some(b'*'), Some(b'/')) => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            (Some(_), _) => {
                                self.bump();
                            }
                            (None, _) => {
                                return Err(ParseError {
                                    span: start,
                                    msg: "unterminated block comment".to_string(),
                                });
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn lex_number(&mut self) -> Result<TokenKind, ParseError> {
        let start_span = self.span();
        let start_idx = self.idx;

        let mut has_dot = false;
        let mut has_exp = false;

        if self.peek() == Some(b'0') {
            match self.peek2() {
                Some(b'x') | Some(b'X') => {
                    self.bump();
                    self.bump();
                    let mut v: u64 = 0;
                    let mut saw = false;
                    while let Some(c) = self.peek() {
                        let d = match c {
                            b'0'..=b'9' => Some((c - b'0') as u64),
                            b'a'..=b'f' => Some((c - b'a') as u64 + 10),
                            b'A'..=b'F' => Some((c - b'A') as u64 + 10),
                            _ => None,
                        };
                        let Some(d) = d else { break };
                        saw = true;
                        self.bump();
                        v = v.saturating_mul(16).saturating_add(d);
                    }
                    if !saw {
                        return Err(ParseError {
                            span: start_span,
                            msg: "expected hex digits after 0x".to_string(),
                        });
                    }
                    return Ok(TokenKind::Int(v as i64));
                }
                Some(b'b') | Some(b'B') => {
                    self.bump();
                    self.bump();
                    let mut v: u64 = 0;
                    let mut saw = false;
                    while let Some(c) = self.peek() {
                        let d = match c {
                            b'0' => Some(0u64),
                            b'1' => Some(1u64),
                            _ => None,
                        };
                        let Some(d) = d else { break };
                        saw = true;
                        self.bump();
                        v = v.saturating_mul(2).saturating_add(d);
                    }
                    if !saw {
                        return Err(ParseError {
                            span: start_span,
                            msg: "expected binary digits after 0b".to_string(),
                        });
                    }
                    return Ok(TokenKind::Int(v as i64));
                }
                _ => {}
            }
        }

        if self.peek() == Some(b'.') {
            has_dot = true;
            self.bump();
        }

        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump();
        }

        if self.peek() == Some(b'.') {
            has_dot = true;
            self.bump();
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }

        if matches!(self.peek(), Some(b'e' | b'E')) {
            has_exp = true;
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            let mut saw = false;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                saw = true;
                self.bump();
            }
            if !saw {
                return Err(ParseError {
                    span: start_span,
                    msg: "expected digits after exponent".to_string(),
                });
            }
        }

        let slice = &self.input[start_idx..self.idx];
        let s = std::str::from_utf8(slice).unwrap_or("");
        if has_dot || has_exp {
            let v: f64 = s.parse().map_err(|_| ParseError {
                span: start_span,
                msg: format!("invalid float literal: {s}"),
            })?;
            Ok(TokenKind::Float(v))
        } else {
            let v: i64 = s.parse().map_err(|_| ParseError {
                span: start_span,
                msg: format!("invalid int literal: {s}"),
            })?;
            Ok(TokenKind::Int(v))
        }
    }

    fn lex_ident(&mut self) -> String {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            // TempleOS treats bytes 128-255 as letters for identifiers (see `::/Demo/ExtChars.HC`).
            if (c as char).is_ascii_alphanumeric() || c == b'_' || c >= 128 {
                out.push(self.bump().unwrap());
            } else {
                break;
            }
        }
        // Identifiers in the vendored TempleOS tree can contain raw CP437 bytes; decode so the
        // parser can treat them as normal strings (byte-accurate via the CP437 mapping).
        temple_rt::assets::decode_cp437_bytes(&out)
    }

    fn empty_macros() -> Arc<HashMap<String, String>> {
        static EMPTY: OnceLock<Arc<HashMap<String, String>>> = OnceLock::new();
        EMPTY.get_or_init(|| Arc::new(HashMap::new())).clone()
    }

    fn expand_macro_kinds(
        name: &str,
        macros: &HashMap<String, String>,
        span: &Span,
        stack: &mut Vec<String>,
    ) -> Result<Vec<TokenKind>, ParseError> {
        if stack.len() > 64 {
            return Err(ParseError {
                span: span.clone(),
                msg: format!("macro expansion too deep while expanding {name}"),
            });
        }
        if stack.iter().any(|s| s == name) {
            return Ok(vec![TokenKind::Ident(name.to_string())]);
        }
        let Some(body) = macros.get(name) else {
            return Ok(vec![TokenKind::Ident(name.to_string())]);
        };

        stack.push(name.to_string());

        let mut out = Vec::new();
        let mut lex = Lexer::new(
            span.file.clone(),
            body.as_bytes(),
            span.line,
            Self::empty_macros(),
        );

        loop {
            let t = lex.next_token()?;
            match t.kind {
                TokenKind::Eof => break,
                TokenKind::Ident(id) => {
                    if macros.contains_key(&id) {
                        out.extend(Self::expand_macro_kinds(&id, macros, span, stack)?);
                    } else {
                        out.push(TokenKind::Ident(id));
                    }
                }
                other => out.push(other),
            }
        }

        stack.pop();
        Ok(out)
    }

    fn lex_string(&mut self) -> Result<String, ParseError> {
        let start = self.span();
        let Some(b'"') = self.bump() else {
            unreachable!("lex_string called without starting quote");
        };
        let mut out = String::new();
        // TempleOS strings often embed DolDoc markup like:
        //   $LK,"MsgLoop",A="MN:MSG_CMD"$
        // and color sequences like:
        //   $$GREEN$$
        // These can contain unescaped `"` characters. Track whether we are inside a
        // $...$ or $$...$$ region so we don't terminate the string on embedded quotes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DolDelim {
            None,
            Single,
            Double,
        }
        let mut dol = DolDelim::None;
        loop {
            match self.bump() {
                Some(b'"') => {
                    if dol == DolDelim::None {
                        return Ok(out);
                    }
                    out.push('"');
                }
                Some(b'\\') => {
                    let esc = self.bump().ok_or_else(|| ParseError {
                        span: start.clone(),
                        msg: "unterminated string escape".to_string(),
                    })?;
                    match esc {
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'0' => out.push('\0'),
                        b'\\' => out.push('\\'),
                        b'"' => out.push('"'),
                        _ => {
                            return Err(ParseError {
                                span: start,
                                msg: format!("unknown string escape: \\{}", esc as char),
                            });
                        }
                    }
                }
                Some(b'$') => {
                    out.push('$');
                    match dol {
                        DolDelim::None => {
                            if self.peek() == Some(b'$') {
                                self.bump();
                                out.push('$');
                                dol = DolDelim::Double;
                            } else {
                                dol = DolDelim::Single;
                            }
                        }
                        DolDelim::Single => {
                            dol = DolDelim::None;
                        }
                        DolDelim::Double => {
                            if self.peek() == Some(b'$') {
                                self.bump();
                                out.push('$');
                                dol = DolDelim::None;
                            }
                        }
                    }
                }
                Some(c) => out.push(temple_rt::assets::decode_cp437_byte(c)),
                None => {
                    return Err(ParseError {
                        span: start,
                        msg: "unterminated string literal".to_string(),
                    });
                }
            }
        }
    }

    fn lex_char(&mut self) -> Result<u64, ParseError> {
        let start = self.span();
        let Some(b'\'') = self.bump() else {
            unreachable!("lex_char called without starting quote");
        };

        let mut bytes = Vec::new();
        loop {
            match self.bump() {
                Some(b'\'') => break,
                Some(b'\\') => {
                    let esc = self.bump().ok_or_else(|| ParseError {
                        span: start.clone(),
                        msg: "unterminated char escape".to_string(),
                    })?;
                    let b = match esc {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'0' => b'\0',
                        b'\\' => b'\\',
                        b'\'' => b'\'',
                        b'"' => b'"',
                        _ => {
                            return Err(ParseError {
                                span: start,
                                msg: format!("unknown char escape: \\{}", esc as char),
                            });
                        }
                    };
                    bytes.push(b);
                }
                None => {
                    return Err(ParseError {
                        span: start,
                        msg: "unterminated char literal".to_string(),
                    });
                }
                Some(c) => bytes.push(c),
            }
        }

        if bytes.is_empty() {
            return Err(ParseError {
                span: start,
                msg: "empty char literal".to_string(),
            });
        }

        let mut v: u64 = 0;
        for (i, b) in bytes.into_iter().take(8).enumerate() {
            v |= (b as u64) << (i * 8);
        }
        Ok(v)
    }

    fn lex_doldoc_cmd(&mut self) -> Result<String, ParseError> {
        let start = self.span();
        let Some(b'$') = self.peek() else {
            return Err(ParseError {
                span: start,
                msg: "lex_doldoc_cmd: expected '$'".to_string(),
            });
        };
        self.bump(); // opening '$'

        let start_idx = self.idx;
        while let Some(c) = self.peek() {
            if c == b'$' {
                let end_idx = self.idx;
                self.bump(); // closing '$'
                let bytes = &self.input[start_idx..end_idx];
                return Ok(std::str::from_utf8(bytes)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| temple_rt::assets::decode_cp437_bytes(bytes)));
            }
            self.bump();
        }

        Err(ParseError {
            span: start,
            msg: "unterminated DolDoc cmd (missing closing '$')".to_string(),
        })
    }

    fn next_token(&mut self) -> Result<Token, ParseError> {
        if let Some(t) = self.macro_queue.pop_front() {
            return Ok(t);
        }

        self.skip_ws_and_comments()?;
        let span = self.span();
        let Some(c) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span,
            });
        };

        if (c as char).is_ascii_digit() || (c == b'.' && matches!(self.peek2(), Some(b'0'..=b'9')))
        {
            let kind = self.lex_number()?;
            return Ok(Token { kind, span });
        }

        if (c as char).is_ascii_alphabetic() || c == b'_' || c >= 128 {
            let s = self.lex_ident();
            if s == "__DIR__" {
                let dir = self.temple_file_dir().unwrap_or_else(|| ".".to_string());
                return Ok(Token {
                    kind: TokenKind::Str(dir),
                    span,
                });
            }
            if s == "__FILE__" {
                let file = self
                    .temple_file_path()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| self.file.as_ref().to_string());
                return Ok(Token {
                    kind: TokenKind::Str(file),
                    span,
                });
            }
            if self.macros.contains_key(&s) {
                let mut stack = Vec::new();
                let expanded = Self::expand_macro_kinds(&s, &self.macros, &span, &mut stack)?;
                if expanded.is_empty() {
                    return Ok(Token {
                        kind: TokenKind::Int(0),
                        span,
                    });
                }
                for kind in expanded {
                    self.macro_queue.push_back(Token {
                        kind,
                        span: span.clone(),
                    });
                }
                return Ok(self
                    .macro_queue
                    .pop_front()
                    .expect("macro_queue non-empty after expansion"));
            }
            return Ok(Token {
                kind: TokenKind::Ident(s),
                span,
            });
        }

        if c == b'"' {
            let s = self.lex_string()?;
            return Ok(Token {
                kind: TokenKind::Str(s),
                span,
            });
        }

        if c == b'\'' {
            let v = self.lex_char()?;
            return Ok(Token {
                kind: TokenKind::Char(v),
                span,
            });
        }

        if c == b'$' {
            let cmd = self.lex_doldoc_cmd()?;
            return Ok(Token {
                kind: TokenKind::DolDocCmd(cmd),
                span,
            });
        }

        let sym = match (c, self.peek2()) {
            (b'=', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::EqEq
            }
            (b'<', Some(b'<')) => {
                if self.peek3() == Some(b'=') {
                    self.bump();
                    self.bump();
                    self.bump();
                    Sym::ShlAssign
                } else {
                    self.bump();
                    self.bump();
                    Sym::Shl
                }
            }
            (b'-', Some(b'>')) => {
                self.bump();
                self.bump();
                Sym::Arrow
            }
            (b'!', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::NotEq
            }
            (b'<', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::Le
            }
            (b'>', Some(b'>')) => {
                if self.peek3() == Some(b'=') {
                    self.bump();
                    self.bump();
                    self.bump();
                    Sym::ShrAssign
                } else {
                    self.bump();
                    self.bump();
                    Sym::Shr
                }
            }
            (b'>', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::Ge
            }
            (b'&', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::AmpersandAssign
            }
            (b'&', Some(b'&')) => {
                self.bump();
                self.bump();
                Sym::AndAnd
            }
            (b'&', _) => {
                self.bump();
                Sym::Ampersand
            }
            (b'|', Some(b'|')) => {
                self.bump();
                self.bump();
                Sym::OrOr
            }
            (b'|', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::PipeAssign
            }
            (b'|', _) => {
                self.bump();
                Sym::Pipe
            }
            (b'^', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::CaretAssign
            }
            (b'^', _) => {
                self.bump();
                Sym::Caret
            }
            (b'~', _) => {
                self.bump();
                Sym::Tilde
            }
            (b'(', _) => {
                self.bump();
                Sym::LParen
            }
            (b')', _) => {
                self.bump();
                Sym::RParen
            }
            (b'{', _) => {
                self.bump();
                Sym::LBrace
            }
            (b'}', _) => {
                self.bump();
                Sym::RBrace
            }
            (b'[', _) => {
                self.bump();
                Sym::LBracket
            }
            (b']', _) => {
                self.bump();
                Sym::RBracket
            }
            (b',', _) => {
                self.bump();
                Sym::Comma
            }
            (b':', _) => {
                self.bump();
                Sym::Colon
            }
            (b';', _) => {
                self.bump();
                Sym::Semicolon
            }
            (b'.', _) => {
                self.bump();
                Sym::Dot
            }
            (b'=', _) => {
                self.bump();
                Sym::Assign
            }
            (b'+', Some(b'+')) => {
                self.bump();
                self.bump();
                Sym::PlusPlus
            }
            (b'+', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::PlusAssign
            }
            (b'+', _) => {
                self.bump();
                Sym::Plus
            }
            (b'-', Some(b'-')) => {
                self.bump();
                self.bump();
                Sym::MinusMinus
            }
            (b'-', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::MinusAssign
            }
            (b'-', _) => {
                self.bump();
                Sym::Minus
            }
            (b'*', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::StarAssign
            }
            (b'*', _) => {
                self.bump();
                Sym::Star
            }
            (b'/', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::SlashAssign
            }
            (b'/', _) => {
                self.bump();
                Sym::Slash
            }
            (b'%', Some(b'=')) => {
                self.bump();
                self.bump();
                Sym::PercentAssign
            }
            (b'%', _) => {
                self.bump();
                Sym::Percent
            }
            (b'!', _) => {
                self.bump();
                Sym::Bang
            }
            (b'<', _) => {
                self.bump();
                Sym::Lt
            }
            (b'>', _) => {
                self.bump();
                Sym::Gt
            }
            _ => {
                self.bump();
                return Err(ParseError {
                    span,
                    msg: format!("unexpected character: {}", c as char),
                });
            }
        };

        Ok(Token {
            kind: TokenKind::Sym(sym),
            span,
        })
    }
}

#[derive(Clone, Debug)]
enum Expr {
    DefaultArg,
    Int(i64),
    Float(f64),
    Str(String),
    Char(u64),
    InitList(Vec<Expr>),
    Var(String),
    AddrOf(Box<Expr>),
    Deref(Box<Expr>),
    Cast {
        expr: Box<Expr>,
        ty: String,
        pointer_depth: usize,
    },
    Member {
        base: Box<Expr>,
        field: String,
    },
    PtrMember {
        base: Box<Expr>,
        field: String,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Assign {
        op: AssignOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    PreInc(String),
    PreDec(String),
    PostInc(String),
    PostDec(String),
    PostIncExpr(Box<Expr>),
    PostDecExpr(Box<Expr>),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    SizeOf(Box<Expr>),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    CompareChain {
        first: Box<Expr>,
        rest: Vec<(BinOp, Expr)>,
    },
    DolDocBinPtr {
        file: Arc<str>,
        bin_num: u32,
    },
    DolDocBinSize {
        file: Arc<str>,
        bin_num: u32,
    },
}

#[derive(Clone, Copy, Debug)]
enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug)]
enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitXor,
    BitOr,
    Shl,
    Shr,
}

#[derive(Clone, Copy, Debug)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    BitAnd,
    BitXor,
    BitOr,
    And,
    Or,
    Comma,
}

#[derive(Clone, Debug)]
enum Stmt {
    Empty,
    Print {
        parts: Vec<Expr>,
    },
    Label(String),
    Goto(String),
    VarDecl {
        decl: Decl,
    },
    VarDecls {
        decls: Vec<Decl>,
    },
    Assign {
        lhs: Expr,
        expr: Expr,
    },
    ExprStmt(Expr),
    TryCatch {
        try_block: Vec<Stmt>,
        catch_block: Vec<Stmt>,
    },
    Throw,
    Break,
    Continue,
    If {
        cond: Expr,
        then_block: Vec<Stmt>,
        else_block: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    DoWhile {
        body: Vec<Stmt>,
        cond: Expr,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        post: Option<Expr>,
        body: Vec<Stmt>,
    },
    Switch {
        expr: Expr,
        arms: Vec<SwitchArm>,
    },
    Return(Option<Expr>),
}

#[derive(Clone, Debug)]
struct Decl {
    ty: String,
    name: String,
    pointer: bool,
    array_lens: Vec<Expr>,
    init: Option<Expr>,
}

#[derive(Clone, Debug)]
struct FieldDef {
    ty: String,
    name: String,
    pointer: bool,
    array_lens: Vec<Expr>,
    init: Option<Expr>,
}

#[derive(Clone, Debug)]
struct ClassDef {
    name: String,
    base_ty: Option<String>,
    fields: Vec<FieldDef>,
    is_extern: bool,
}

#[derive(Clone, Debug)]
enum SwitchArm {
    Case {
        value: i64,
        body: Vec<Stmt>,
    },
    Group {
        prefix: Vec<Stmt>,
        arms: Vec<SwitchArm>,
        suffix: Vec<Stmt>,
    },
}

fn switch_arm_contains_value(arm: &SwitchArm, value: i64) -> bool {
    match arm {
        SwitchArm::Case { value: v, .. } => *v == value,
        SwitchArm::Group { arms, .. } => arms.iter().any(|a| switch_arm_contains_value(a, value)),
    }
}

#[derive(Clone, Debug)]
struct Function {
    name: String,
    params: Vec<String>,
    body: Vec<Stmt>,
}

#[derive(Debug)]
struct Program {
    classes: HashMap<String, ClassDef>,
    functions: HashMap<String, Function>,
    top_level: Vec<Stmt>,
    bins_by_file: HashMap<Arc<str>, std::collections::BTreeMap<u32, Vec<u8>>>,
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    user_types: HashSet<String>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            user_types: HashSet::new(),
        }
    }

    fn is_type_ident(&self, name: &str) -> bool {
        is_type_name(name) || is_user_type_name(name) || self.user_types.contains(name)
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or(self.tokens.last().expect("tokens non-empty"))
    }

    fn bump(&mut self) -> Token {
        let t = self.peek().clone();
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn eat_sym(&mut self, sym: Sym) -> bool {
        matches!(self.peek().kind, TokenKind::Sym(s) if s == sym)
            .then(|| self.bump())
            .is_some()
    }

    fn expect_sym(&mut self, sym: Sym) -> Result<(), ParseError> {
        let t = self.bump();
        match t.kind {
            TokenKind::Sym(s) if s == sym => Ok(()),
            _ => Err(ParseError {
                span: t.span,
                msg: format!("expected {sym:?}"),
            }),
        }
    }

    fn expect_ident(&mut self) -> Result<(Span, String), ParseError> {
        let t = self.bump();
        match t.kind {
            TokenKind::Ident(s) => Ok((t.span, s)),
            _ => Err(ParseError {
                span: t.span,
                msg: "expected identifier".to_string(),
            }),
        }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut classes = HashMap::new();
        let mut functions = HashMap::new();
        let mut top_level = Vec::new();

        while !self.at_eof() {
            if self.looks_like_class_def() {
                let (class_def, instance_decls) = self.parse_class_def()?;
                classes.insert(class_def.name.clone(), class_def);
                if !instance_decls.is_empty() {
                    top_level.push(Stmt::VarDecls {
                        decls: instance_decls,
                    });
                }
                continue;
            }
            if self.looks_like_function_def() {
                let func = self.parse_function_def()?;
                functions.insert(func.name.clone(), func);
                continue;
            }
            top_level.push(self.parse_stmt()?);
        }

        Ok(Program {
            classes,
            functions,
            top_level,
            bins_by_file: HashMap::new(),
        })
    }

    fn parse_expr_only(tokens: Vec<Token>) -> Result<Expr, ParseError> {
        let mut p = Parser::new(tokens);
        let expr = p.parse_expr()?;
        if !p.at_eof() {
            let t = p.peek().clone();
            return Err(ParseError {
                span: t.span,
                msg: "unexpected trailing tokens".to_string(),
            });
        }
        Ok(expr)
    }

    fn looks_like_function_def(&self) -> bool {
        let mut i = self.pos;

        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Ident(s)) if s == "public" || s == "extern" || s == "static"
        ) {
            i += 1;
        }

        let Some(t0) = self.tokens.get(i) else {
            return false;
        };
        let TokenKind::Ident(ty) = &t0.kind else {
            return false;
        };
        if !self.is_type_ident(ty) {
            return false;
        }

        i += 1;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Sym(Sym::Star))
        ) {
            i += 1;
        }

        if !matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Ident(_))
        ) {
            return false;
        }
        i += 1;

        if !matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Sym(Sym::LParen))
        ) {
            return false;
        }

        // Scan forward to find the matching ')' and ensure a '{' follows.
        let mut depth = 0i32;
        while let Some(tok) = self.tokens.get(i) {
            match tok.kind {
                TokenKind::Sym(Sym::LParen) => depth += 1,
                TokenKind::Sym(Sym::RParen) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        if depth != 0 {
            return false;
        }

        matches!(
            self.tokens.get(i + 1).map(|t| &t.kind),
            Some(TokenKind::Sym(Sym::LBrace))
        )
    }

    fn parse_function_def(&mut self) -> Result<Function, ParseError> {
        while self.is_kw("public") || self.is_kw("extern") || self.is_kw("static") {
            self.bump();
        }
        let (_ty_span, _ret_ty) = self.expect_ident()?;
        while self.eat_sym(Sym::Star) {}
        let (_name_span, name) = self.expect_ident()?;
        self.expect_sym(Sym::LParen)?;
        let params = self.parse_param_list()?;
        let body = self.parse_block()?;
        Ok(Function { name, params, body })
    }

    fn parse_param_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut params: Vec<String> = Vec::new();
        if self.eat_sym(Sym::RParen) {
            return Ok(params);
        }

        let mut idx = 0usize;
        loop {
            let (ty_span, ty) = self.expect_ident()?;
            if !self.is_type_ident(&ty) {
                return Err(ParseError {
                    span: ty_span,
                    msg: "expected parameter type".to_string(),
                });
            }

            while self.eat_sym(Sym::Star) {}

            let name = match &self.peek().kind {
                TokenKind::Ident(_) => {
                    let (_span, name) = self.expect_ident()?;
                    name
                }
                _ => format!("_arg{idx}"),
            };
            idx += 1;

            if self.eat_sym(Sym::LBracket) {
                let _ = self.parse_expr()?;
                self.expect_sym(Sym::RBracket)?;
            }

            if self.eat_sym(Sym::Assign) {
                let _ = self.parse_expr()?;
            }

            params.push(name);

            if self.eat_sym(Sym::Comma) {
                continue;
            }
            self.expect_sym(Sym::RParen)?;
            break;
        }
        Ok(params)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect_sym(Sym::LBrace)?;
        let mut out = Vec::new();
        while !self.eat_sym(Sym::RBrace) {
            if self.at_eof() {
                let t = self.peek().clone();
                return Err(ParseError {
                    span: t.span,
                    msg: "unexpected EOF in block".to_string(),
                });
            }
            out.push(self.parse_stmt()?);
        }
        Ok(out)
    }

    fn parse_stmt_or_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        if matches!(self.peek().kind, TokenKind::Sym(Sym::LBrace)) {
            return self.parse_block();
        }
        Ok(vec![self.parse_stmt()?])
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        if self.eat_sym(Sym::Semicolon) {
            return Ok(Stmt::Empty);
        }

        // TempleOS source files often embed DolDoc commands (sprites, formatting) as standalone
        // lines. They are not HolyC statements; ignore them.
        if matches!(self.peek().kind, TokenKind::DolDocCmd(_)) {
            self.bump();
            return Ok(Stmt::Empty);
        }

        // Labels (used by `goto`). Example: `sq_done:`.
        if let TokenKind::Ident(name) = &self.peek().kind {
            if matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                Some(TokenKind::Sym(Sym::Colon))
            ) {
                let name = name.clone();
                self.bump();
                self.expect_sym(Sym::Colon)?;
                return Ok(Stmt::Label(name));
            }
        }

        if matches!(self.peek().kind, TokenKind::Str(_) | TokenKind::Char(_)) {
            let mut parts = Vec::new();
            parts.push(self.parse_expr()?);
            while self.eat_sym(Sym::Comma) {
                parts.push(self.parse_expr()?);
            }
            self.expect_sym(Sym::Semicolon)?;
            return Ok(Stmt::Print { parts });
        }

        if self.is_kw("throw") {
            self.bump();
            self.expect_sym(Sym::Semicolon)?;
            return Ok(Stmt::Throw);
        }

        if self.is_kw("try") {
            self.bump();
            let try_block = self.parse_stmt_or_block()?;
            if !self.is_kw("catch") {
                let t = self.peek().clone();
                return Err(ParseError {
                    span: t.span,
                    msg: "expected catch after try block".to_string(),
                });
            }
            self.bump(); // catch
            let catch_block = self.parse_stmt_or_block()?;
            return Ok(Stmt::TryCatch {
                try_block,
                catch_block,
            });
        }

        if self.is_kw("enum") {
            self.bump(); // enum

            // Optional tag name: `enum Foo { ... };`
            if matches!(
                (
                    self.tokens.get(self.pos).map(|t| &t.kind),
                    self.tokens.get(self.pos + 1).map(|t| &t.kind),
                ),
                (Some(TokenKind::Ident(_)), Some(TokenKind::Sym(Sym::LBrace)))
            ) {
                self.bump();
            }

            self.expect_sym(Sym::LBrace)?;
            let mut decls = Vec::new();
            let mut prev_name: Option<String> = None;
            if !self.eat_sym(Sym::RBrace) {
                loop {
                    let (_span, name) = self.expect_ident()?;
                    let init = if self.eat_sym(Sym::Assign) {
                        self.parse_expr()?
                    } else if let Some(prev) = prev_name.as_ref() {
                        Expr::Binary {
                            op: BinOp::Add,
                            left: Box::new(Expr::Var(prev.clone())),
                            right: Box::new(Expr::Int(1)),
                        }
                    } else {
                        Expr::Int(0)
                    };

                    decls.push(Decl {
                        ty: "I64".to_string(),
                        name: name.clone(),
                        pointer: false,
                        array_lens: Vec::new(),
                        init: Some(init),
                    });
                    prev_name = Some(name);

                    if self.eat_sym(Sym::Comma) {
                        if self.eat_sym(Sym::RBrace) {
                            break;
                        }
                        continue;
                    }

                    self.expect_sym(Sym::RBrace)?;
                    break;
                }
            }

            // `enum { ... };`
            if self.eat_sym(Sym::Semicolon) {
                return Ok(Stmt::VarDecls { decls });
            }

            // Optional instance declarations: `enum Foo { ... } a, b;`
            if matches!(
                self.peek().kind,
                TokenKind::Ident(_) | TokenKind::Sym(Sym::Star)
            ) {
                loop {
                    let mut pointer = false;
                    while self.eat_sym(Sym::Star) {
                        pointer = true;
                    }

                    let (_span, name) = self.expect_ident()?;
                    let mut array_lens = Vec::new();
                    while self.eat_sym(Sym::LBracket) {
                        if self.eat_sym(Sym::RBracket) {
                            let t = self.peek().clone();
                            return Err(ParseError {
                                span: t.span,
                                msg: "expected array length in enum instance decl".to_string(),
                            });
                        }
                        let len = self.parse_expr()?;
                        self.expect_sym(Sym::RBracket)?;
                        array_lens.push(len);
                    }

                    let init = self
                        .eat_sym(Sym::Assign)
                        .then(|| self.parse_expr())
                        .transpose()?;
                    decls.push(Decl {
                        ty: "I64".to_string(),
                        name,
                        pointer,
                        array_lens,
                        init,
                    });

                    if self.eat_sym(Sym::Comma) {
                        continue;
                    }
                    break;
                }
            }

            self.expect_sym(Sym::Semicolon)?;
            return Ok(Stmt::VarDecls { decls });
        }

        if self.is_kw("if") {
            self.bump();
            self.expect_sym(Sym::LParen)?;
            let cond = self.parse_expr()?;
            self.expect_sym(Sym::RParen)?;
            let then_block = self.parse_stmt_or_block()?;
            let else_block = if self.is_kw("else") {
                self.bump();
                Some(self.parse_stmt_or_block()?)
            } else {
                None
            };
            return Ok(Stmt::If {
                cond,
                then_block,
                else_block,
            });
        }

        if self.is_kw("while") {
            self.bump();
            self.expect_sym(Sym::LParen)?;
            let cond = self.parse_expr()?;
            self.expect_sym(Sym::RParen)?;
            let body = self.parse_stmt_or_block()?;
            return Ok(Stmt::While { cond, body });
        }

        if self.is_kw("do") {
            self.bump(); // do
            let body = self.parse_stmt_or_block()?;
            if !self.is_kw("while") {
                let t = self.peek().clone();
                return Err(ParseError {
                    span: t.span,
                    msg: "expected while after do {...}".to_string(),
                });
            }
            self.bump(); // while
            self.expect_sym(Sym::LParen)?;
            let cond = self.parse_expr()?;
            self.expect_sym(Sym::RParen)?;
            self.expect_sym(Sym::Semicolon)?;
            return Ok(Stmt::DoWhile { body, cond });
        }

        if self.is_kw("for") {
            self.bump();
            self.expect_sym(Sym::LParen)?;

            let init = if self.eat_sym(Sym::Semicolon) {
                None
            } else {
                let stmt = if self.looks_like_var_decl() {
                    let decls = self.parse_var_decl_list()?;
                    if decls.len() == 1 {
                        let decl = decls.into_iter().next().unwrap();
                        Stmt::VarDecl { decl }
                    } else {
                        Stmt::VarDecls { decls }
                    }
                } else {
                    let lhs = self.parse_expr()?;
                    if self.eat_sym(Sym::Assign) {
                        let expr = self.parse_expr()?;
                        Stmt::Assign { lhs, expr }
                    } else {
                        Stmt::ExprStmt(lhs)
                    }
                };
                self.expect_sym(Sym::Semicolon)?;
                Some(Box::new(stmt))
            };

            let cond = if self.eat_sym(Sym::Semicolon) {
                None
            } else {
                let expr = self.parse_expr()?;
                self.expect_sym(Sym::Semicolon)?;
                Some(expr)
            };

            let post = if self.eat_sym(Sym::RParen) {
                None
            } else {
                let expr = self.parse_expr_with_comma()?;
                self.expect_sym(Sym::RParen)?;
                Some(expr)
            };

            let body = self.parse_stmt_or_block()?;
            return Ok(Stmt::For {
                init,
                cond,
                post,
                body,
            });
        }

        if self.is_kw("switch") {
            self.bump();
            self.expect_sym(Sym::LParen)?;
            let expr = self.parse_expr()?;
            self.expect_sym(Sym::RParen)?;
            let arms = self.parse_switch_arms()?;
            return Ok(Stmt::Switch { expr, arms });
        }

        if self.is_kw("break") {
            self.bump();
            self.expect_sym(Sym::Semicolon)?;
            return Ok(Stmt::Break);
        }

        if self.is_kw("continue") {
            self.bump();
            self.expect_sym(Sym::Semicolon)?;
            return Ok(Stmt::Continue);
        }

        if self.is_kw("goto") {
            self.bump();
            let (_span, label) = self.expect_ident()?;
            self.expect_sym(Sym::Semicolon)?;
            return Ok(Stmt::Goto(label));
        }

        if self.is_kw("return") {
            self.bump();
            if self.eat_sym(Sym::Semicolon) {
                return Ok(Stmt::Return(None));
            }
            let expr = self.parse_expr()?;
            self.expect_sym(Sym::Semicolon)?;
            return Ok(Stmt::Return(Some(expr)));
        }

        if self.looks_like_var_decl() {
            let decls = self.parse_var_decl_list()?;
            self.expect_sym(Sym::Semicolon)?;
            if decls.len() == 1 {
                let decl = decls.into_iter().next().unwrap();
                return Ok(Stmt::VarDecl { decl });
            }
            return Ok(Stmt::VarDecls { decls });
        }

        let lhs = self.parse_expr()?;
        if self.eat_sym(Sym::Assign) {
            let expr = self.parse_expr()?;
            self.expect_sym(Sym::Semicolon)?;
            Ok(Stmt::Assign { lhs, expr })
        } else {
            self.expect_sym(Sym::Semicolon)?;
            Ok(Stmt::ExprStmt(lhs))
        }
    }

    fn at_label(&self, name: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Ident(s) if s == name)
            && matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                Some(TokenKind::Sym(Sym::Colon))
            )
    }

    fn parse_switch_arms(&mut self) -> Result<Vec<SwitchArm>, ParseError> {
        #[derive(Clone, Copy, Debug)]
        enum Label {
            Case(i64),
            Start,
            End,
        }

        #[derive(Debug)]
        struct Section {
            label: Label,
            body: Vec<Stmt>,
        }

        self.expect_sym(Sym::LBrace)?;

        let mut sections: Vec<Section> = Vec::new();
        let mut cur_label: Option<Label> = None;
        let mut cur_body: Vec<Stmt> = Vec::new();
        let mut last_case: Option<i64> = None;

        while !self.eat_sym(Sym::RBrace) {
            if self.at_eof() {
                let t = self.peek().clone();
                return Err(ParseError {
                    span: t.span,
                    msg: "unexpected EOF in switch".to_string(),
                });
            }

            if self.is_kw("case") {
                if let Some(label) = cur_label.take() {
                    sections.push(Section {
                        label,
                        body: std::mem::take(&mut cur_body),
                    });
                } else {
                    cur_body.clear();
                }

                self.bump(); // case

                let value = if self.eat_sym(Sym::Colon) {
                    let v = last_case.map(|v| v + 1).unwrap_or(0);
                    last_case = Some(v);
                    v
                } else {
                    let expr = self.parse_expr()?;
                    self.expect_sym(Sym::Colon)?;
                    let v = match expr {
                        Expr::Int(v) => v,
                        Expr::Char(v) => v as i64,
                        other => {
                            let t = self.peek().clone();
                            return Err(ParseError {
                                span: t.span,
                                msg: format!(
                                    "case label must be an int/char literal, got {other:?}"
                                ),
                            });
                        }
                    };
                    last_case = Some(v);
                    v
                };

                cur_label = Some(Label::Case(value));
                continue;
            }

            if self.at_label("start") {
                if let Some(label) = cur_label.take() {
                    sections.push(Section {
                        label,
                        body: std::mem::take(&mut cur_body),
                    });
                } else {
                    cur_body.clear();
                }

                self.bump(); // start
                self.expect_sym(Sym::Colon)?;
                cur_label = Some(Label::Start);
                continue;
            }

            if self.at_label("end") {
                if let Some(label) = cur_label.take() {
                    sections.push(Section {
                        label,
                        body: std::mem::take(&mut cur_body),
                    });
                } else {
                    cur_body.clear();
                }

                self.bump(); // end
                self.expect_sym(Sym::Colon)?;
                cur_label = Some(Label::End);
                continue;
            }

            cur_body.push(self.parse_stmt()?);
        }

        if let Some(label) = cur_label.take() {
            sections.push(Section {
                label,
                body: cur_body,
            });
        }

        struct GroupBuilder {
            prefix: Vec<Stmt>,
            arms: Vec<SwitchArm>,
        }

        let mut out: Vec<SwitchArm> = Vec::new();
        let mut stack: Vec<GroupBuilder> = Vec::new();

        let push_arm = |arm: SwitchArm, stack: &mut Vec<GroupBuilder>, out: &mut Vec<SwitchArm>| {
            if let Some(top) = stack.last_mut() {
                top.arms.push(arm);
            } else {
                out.push(arm);
            }
        };

        for sec in sections {
            match sec.label {
                Label::Case(value) => {
                    push_arm(
                        SwitchArm::Case {
                            value,
                            body: sec.body,
                        },
                        &mut stack,
                        &mut out,
                    );
                }
                Label::Start => {
                    stack.push(GroupBuilder {
                        prefix: sec.body,
                        arms: Vec::new(),
                    });
                }
                Label::End => {
                    let Some(group) = stack.pop() else {
                        let t = self.peek().clone();
                        return Err(ParseError {
                            span: t.span,
                            msg: "end: without matching start:".to_string(),
                        });
                    };
                    push_arm(
                        SwitchArm::Group {
                            prefix: group.prefix,
                            arms: group.arms,
                            suffix: sec.body,
                        },
                        &mut stack,
                        &mut out,
                    );
                }
            }
        }

        if let Some(_unclosed) = stack.pop() {
            let t = self.peek().clone();
            return Err(ParseError {
                span: t.span,
                msg: "start: without matching end:".to_string(),
            });
        }

        Ok(out)
    }

    fn looks_like_var_decl(&self) -> bool {
        let Some(t0) = self.tokens.get(self.pos) else {
            return false;
        };
        let TokenKind::Ident(ty) = &t0.kind else {
            return false;
        };
        if !self.is_type_ident(ty) {
            return false;
        }

        let mut i = self.pos + 1;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Sym(Sym::Star))
        ) {
            i += 1;
        }
        if matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Ident(_))
        ) {
            return true;
        }

        // Function-pointer declarator: `Ty (*name)(...)`
        matches!(
            (
                self.tokens.get(i).map(|t| &t.kind),
                self.tokens.get(i + 1).map(|t| &t.kind),
                self.tokens.get(i + 2).map(|t| &t.kind),
                self.tokens.get(i + 3).map(|t| &t.kind),
                self.tokens.get(i + 4).map(|t| &t.kind),
            ),
            (
                Some(TokenKind::Sym(Sym::LParen)),
                Some(TokenKind::Sym(Sym::Star)),
                Some(TokenKind::Ident(_)),
                Some(TokenKind::Sym(Sym::RParen)),
                Some(TokenKind::Sym(Sym::LParen)),
            )
        )
    }

    fn skip_paren_group(&mut self) -> Result<(), ParseError> {
        let mut depth = 1usize;
        while depth > 0 {
            let t = self.bump();
            match t.kind {
                TokenKind::Sym(Sym::LParen) => depth = depth.saturating_add(1),
                TokenKind::Sym(Sym::RParen) => depth = depth.saturating_sub(1),
                TokenKind::Eof => {
                    return Err(ParseError {
                        span: t.span,
                        msg: "unterminated parenthesized group".to_string(),
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_var_decl_list(&mut self) -> Result<Vec<Decl>, ParseError> {
        let (_span, ty) = self.expect_ident()?; // type name

        // Pointer stars can appear either after the type or before each declarator.
        let mut base_pointer = false;
        while self.eat_sym(Sym::Star) {
            base_pointer = true;
        }

        let mut decls: Vec<Decl> = Vec::new();
        loop {
            let mut pointer = base_pointer;
            while self.eat_sym(Sym::Star) {
                pointer = true;
            }

            // Function pointer variable declaration (C-style), e.g.:
            //   U0 (*cb)(CDC *dc,I64 x,I64 y);
            let (_span, name) = if self.eat_sym(Sym::LParen) {
                self.expect_sym(Sym::Star)?;
                pointer = true;
                let (span, name) = self.expect_ident()?;
                self.expect_sym(Sym::RParen)?;

                if self.eat_sym(Sym::LParen) {
                    self.skip_paren_group()?;
                } else {
                    let t = self.peek().clone();
                    return Err(ParseError {
                        span: t.span,
                        msg: "function pointer declarator missing parameter list".to_string(),
                    });
                }

                (span, name)
            } else {
                self.expect_ident()?
            };

            let mut array_lens: Vec<Expr> = Vec::new();
            while self.eat_sym(Sym::LBracket) {
                let len = self.parse_expr()?;
                self.expect_sym(Sym::RBracket)?;
                array_lens.push(len);
            }

            let init = if self.eat_sym(Sym::Assign) {
                Some(self.parse_expr()?)
            } else {
                None
            };

            decls.push(Decl {
                ty: ty.clone(),
                name,
                pointer,
                array_lens,
                init,
            });

            if self.eat_sym(Sym::Comma) {
                continue;
            }
            break;
        }
        Ok(decls)
    }

    fn is_kw(&self, kw: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Ident(s) if s == kw)
    }

    fn looks_like_cast_in_parens(&self) -> Option<(String, usize)> {
        let Some(TokenKind::Ident(ty)) = self.tokens.get(self.pos).map(|t| &t.kind) else {
            return None;
        };
        if !self.is_type_ident(ty) {
            return None;
        }

        let mut i = self.pos + 1;
        let mut pointer_depth = 0usize;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Sym(Sym::Star))
        ) {
            pointer_depth += 1;
            i += 1;
        }

        matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Sym(Sym::RParen))
        )
        .then(|| (ty.clone(), pointer_depth))
    }

    fn looks_like_class_def(&self) -> bool {
        let mut i = self.pos;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Ident(s)) if s == "public" || s == "extern"
        ) {
            i += 1;
        }

        // Optional base type: `I64 class Foo { ... }`
        if let Some(TokenKind::Ident(ty)) = self.tokens.get(i).map(|t| &t.kind) {
            if self.is_type_ident(ty)
                && matches!(
                    self.tokens.get(i + 1).map(|t| &t.kind),
                    Some(TokenKind::Ident(s)) if s == "class"
                )
            {
                i += 1;
            }
        }

        matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Ident(s)) if s == "class"
        )
    }

    fn parse_class_instance_decls(&mut self, class_name: &str) -> Result<Vec<Decl>, ParseError> {
        let mut decls: Vec<Decl> = Vec::new();
        loop {
            let mut pointer = false;
            while self.eat_sym(Sym::Star) {
                pointer = true;
            }

            let (_name_span, name) = self.expect_ident()?;

            let mut array_lens: Vec<Expr> = Vec::new();
            while self.eat_sym(Sym::LBracket) {
                let len = self.parse_expr()?;
                self.expect_sym(Sym::RBracket)?;
                array_lens.push(len);
            }

            let init = if self.eat_sym(Sym::Assign) {
                Some(self.parse_expr()?)
            } else {
                None
            };

            decls.push(Decl {
                ty: class_name.to_string(),
                name,
                pointer,
                array_lens,
                init,
            });

            if self.eat_sym(Sym::Comma) {
                continue;
            }
            break;
        }
        self.expect_sym(Sym::Semicolon)?;
        Ok(decls)
    }

    fn parse_class_def(&mut self) -> Result<(ClassDef, Vec<Decl>), ParseError> {
        let mut is_extern = false;
        while self.is_kw("public") || self.is_kw("extern") {
            if self.is_kw("extern") {
                is_extern = true;
            }
            self.bump();
        }

        // Optional base type: `I64 class CDate { ... }`
        let base_ty = match (
            &self.peek().kind,
            self.tokens.get(self.pos + 1).map(|t| &t.kind),
        ) {
            (TokenKind::Ident(ty), Some(TokenKind::Ident(kw)))
                if kw == "class" && self.is_type_ident(ty) =>
            {
                let (_span, base_ty) = self.expect_ident()?;
                Some(base_ty)
            }
            _ => None,
        };

        let (kw_span, kw) = self.expect_ident()?;
        if kw != "class" {
            return Err(ParseError {
                span: kw_span,
                msg: "expected class".to_string(),
            });
        }

        let (_name_span, name) = self.expect_ident()?;
        // Register early so the class can be self-referential (e.g. `TimeEntry *next` inside `class TimeEntry`).
        self.user_types.insert(name.clone());

        if is_extern && self.eat_sym(Sym::Semicolon) {
            return Ok((
                ClassDef {
                    name,
                    base_ty,
                    fields: Vec::new(),
                    is_extern,
                },
                Vec::new(),
            ));
        }

        self.expect_sym(Sym::LBrace)?;
        let mut fields: Vec<FieldDef> = Vec::new();
        while !self.eat_sym(Sym::RBrace) {
            if self.at_eof() {
                let t = self.peek().clone();
                return Err(ParseError {
                    span: t.span,
                    msg: "unexpected EOF in class body".to_string(),
                });
            }

            // Field declarations are essentially variable declarations without requiring runtime execution.
            let decls = self.parse_var_decl_list()?;
            self.expect_sym(Sym::Semicolon)?;
            for decl in decls {
                fields.push(FieldDef {
                    ty: decl.ty,
                    name: decl.name,
                    pointer: decl.pointer,
                    array_lens: decl.array_lens,
                    init: decl.init,
                });
            }
        }
        let mut instance_decls = Vec::new();
        if self.eat_sym(Sym::Semicolon) {
            // Standard `class Foo { ... };` (no instances).
        } else if matches!(
            self.peek().kind,
            TokenKind::Ident(_) | TokenKind::Sym(Sym::Star)
        ) {
            // TempleOS HolyC pattern: `class Foo { ... } foo;` (declare instances after the body).
            instance_decls = self.parse_class_instance_decls(&name)?;
        }

        Ok((
            ClassDef {
                name,
                base_ty,
                fields,
                is_extern,
            },
            instance_decls,
        ))
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_assign()
    }

    fn parse_expr_with_comma(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_assign()?;
        while self.eat_sym(Sym::Comma) {
            let right = self.parse_assign()?;
            left = Expr::Binary {
                op: BinOp::Comma,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_assign(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_or()?;
        let op = match &self.peek().kind {
            TokenKind::Sym(Sym::Assign) => Some(AssignOp::Assign),
            TokenKind::Sym(Sym::PlusAssign) => Some(AssignOp::Add),
            TokenKind::Sym(Sym::MinusAssign) => Some(AssignOp::Sub),
            TokenKind::Sym(Sym::StarAssign) => Some(AssignOp::Mul),
            TokenKind::Sym(Sym::SlashAssign) => Some(AssignOp::Div),
            TokenKind::Sym(Sym::PercentAssign) => Some(AssignOp::Rem),
            TokenKind::Sym(Sym::AmpersandAssign) => Some(AssignOp::BitAnd),
            TokenKind::Sym(Sym::PipeAssign) => Some(AssignOp::BitOr),
            TokenKind::Sym(Sym::CaretAssign) => Some(AssignOp::BitXor),
            TokenKind::Sym(Sym::ShlAssign) => Some(AssignOp::Shl),
            TokenKind::Sym(Sym::ShrAssign) => Some(AssignOp::Shr),
            _ => None,
        };

        let Some(op) = op else {
            return Ok(left);
        };
        self.bump();
        let right = self.parse_assign()?;
        Ok(Expr::Assign {
            op,
            lhs: Box::new(left),
            rhs: Box::new(right),
        })
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.eat_sym(Sym::OrOr) {
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_eq()?;
        while self.eat_sym(Sym::AndAnd) {
            let right = self.parse_eq()?;
            left = Expr::Binary {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_eq(&mut self) -> Result<Expr, ParseError> {
        let first = self.parse_rel()?;
        let mut rest: Vec<(BinOp, Expr)> = Vec::new();
        loop {
            let op = if self.eat_sym(Sym::EqEq) {
                Some(BinOp::Eq)
            } else if self.eat_sym(Sym::NotEq) {
                Some(BinOp::Ne)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_rel()?;
            rest.push((op, right));
        }

        match rest.len() {
            0 => Ok(first),
            1 => {
                let (op, right) = rest.into_iter().next().expect("len checked");
                Ok(Expr::Binary {
                    op,
                    left: Box::new(first),
                    right: Box::new(right),
                })
            }
            _ => Ok(Expr::CompareChain {
                first: Box::new(first),
                rest,
            }),
        }
    }

    fn parse_rel(&mut self) -> Result<Expr, ParseError> {
        let first = self.parse_add()?;
        let mut rest: Vec<(BinOp, Expr)> = Vec::new();
        loop {
            let op = if self.eat_sym(Sym::Lt) {
                Some(BinOp::Lt)
            } else if self.eat_sym(Sym::Le) {
                Some(BinOp::Le)
            } else if self.eat_sym(Sym::Gt) {
                Some(BinOp::Gt)
            } else if self.eat_sym(Sym::Ge) {
                Some(BinOp::Ge)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_add()?;
            rest.push((op, right));
        }

        match rest.len() {
            0 => Ok(first),
            1 => {
                let (op, right) = rest.into_iter().next().expect("len checked");
                Ok(Expr::Binary {
                    op,
                    left: Box::new(first),
                    right: Box::new(right),
                })
            }
            _ => Ok(Expr::CompareChain {
                first: Box::new(first),
                rest,
            }),
        }
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_or()?;
        loop {
            let op = if self.eat_sym(Sym::Plus) {
                Some(BinOp::Add)
            } else if self.eat_sym(Sym::Minus) {
                Some(BinOp::Sub)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_bit_or()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_xor()?;
        while self.eat_sym(Sym::Pipe) {
            let right = self.parse_bit_xor()?;
            left = Expr::Binary {
                op: BinOp::BitOr,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_xor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_and()?;
        while self.eat_sym(Sym::Caret) {
            let right = self.parse_bit_and()?;
            left = Expr::Binary {
                op: BinOp::BitXor,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_mul()?;
        while self.eat_sym(Sym::Ampersand) {
            let right = self.parse_mul()?;
            left = Expr::Binary {
                op: BinOp::BitAnd,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift()?;
        loop {
            let op = if self.eat_sym(Sym::Star) {
                Some(BinOp::Mul)
            } else if self.eat_sym(Sym::Slash) {
                Some(BinOp::Div)
            } else if self.eat_sym(Sym::Percent) {
                Some(BinOp::Rem)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_shift()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = if self.eat_sym(Sym::Shl) {
                Some(BinOp::Shl)
            } else if self.eat_sym(Sym::Shr) {
                Some(BinOp::Shr)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.is_kw("sizeof") {
            self.bump();
            self.expect_sym(Sym::LParen)?;
            let inner = self.parse_expr()?;
            self.expect_sym(Sym::RParen)?;
            return Ok(Expr::SizeOf(Box::new(inner)));
        }

        if self.eat_sym(Sym::PlusPlus) {
            let t = self.bump();
            match t.kind {
                TokenKind::Ident(name) => return Ok(Expr::PreInc(name)),
                _ => {
                    return Err(ParseError {
                        span: t.span,
                        msg: "expected identifier after ++".to_string(),
                    });
                }
            }
        }
        if self.eat_sym(Sym::MinusMinus) {
            let t = self.bump();
            match t.kind {
                TokenKind::Ident(name) => return Ok(Expr::PreDec(name)),
                _ => {
                    return Err(ParseError {
                        span: t.span,
                        msg: "expected identifier after --".to_string(),
                    });
                }
            }
        }
        if self.eat_sym(Sym::Minus) {
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            });
        }
        if self.eat_sym(Sym::Tilde) {
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::BitNot,
                expr: Box::new(expr),
            });
        }
        if self.eat_sym(Sym::Ampersand) {
            let expr = self.parse_unary()?;
            if !matches!(
                expr,
                Expr::Var(_)
                    | Expr::Index { .. }
                    | Expr::Member { .. }
                    | Expr::PtrMember { .. }
                    | Expr::Deref(_)
            ) {
                let t = self.peek().clone();
                return Err(ParseError {
                    span: t.span,
                    msg: "address-of (&) expects an lvalue".to_string(),
                });
            }
            return Ok(Expr::AddrOf(Box::new(expr)));
        }
        if self.eat_sym(Sym::Star) {
            let expr = self.parse_unary()?;
            return Ok(Expr::Deref(Box::new(expr)));
        }
        if self.eat_sym(Sym::Bang) {
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.eat_sym(Sym::LParen) {
                if let Some((_ty, _pointer_depth)) = self.looks_like_cast_in_parens() {
                    let (_ty_span, ty) = self.expect_ident()?;
                    let mut pointer_depth = 0usize;
                    while self.eat_sym(Sym::Star) {
                        pointer_depth += 1;
                    }
                    self.expect_sym(Sym::RParen)?;
                    expr = Expr::Cast {
                        expr: Box::new(expr),
                        ty,
                        pointer_depth,
                    };
                    continue;
                }

                let mut args = Vec::new();
                if !self.eat_sym(Sym::RParen) {
                    loop {
                        args.push(self.parse_call_arg()?);
                        if self.eat_sym(Sym::Comma) {
                            continue;
                        }
                        self.expect_sym(Sym::RParen)?;
                        break;
                    }
                }
                expr = Expr::Call {
                    callee: Box::new(std::mem::replace(&mut expr, Expr::Int(0))),
                    args,
                };
                continue;
            }
            if self.eat_sym(Sym::Dot) {
                let (_span, field) = self.expect_ident()?;
                expr = Expr::Member {
                    base: Box::new(expr),
                    field,
                };
                continue;
            }
            if self.eat_sym(Sym::Arrow) {
                let (_span, field) = self.expect_ident()?;
                expr = Expr::PtrMember {
                    base: Box::new(expr),
                    field,
                };
                continue;
            }
            if self.eat_sym(Sym::LBracket) {
                let index = self.parse_expr()?;
                self.expect_sym(Sym::RBracket)?;
                expr = Expr::Index {
                    base: Box::new(expr),
                    index: Box::new(index),
                };
                continue;
            }
            if self.eat_sym(Sym::PlusPlus) {
                expr = match std::mem::replace(&mut expr, Expr::Int(0)) {
                    Expr::Var(name) => Expr::PostInc(name),
                    other => Expr::PostIncExpr(Box::new(other)),
                };
                continue;
            }
            if self.eat_sym(Sym::MinusMinus) {
                expr = match std::mem::replace(&mut expr, Expr::Int(0)) {
                    Expr::Var(name) => Expr::PostDec(name),
                    other => Expr::PostDecExpr(Box::new(other)),
                };
                continue;
            }
            break;
        }
        Ok(expr)
    }

    fn parse_call_arg(&mut self) -> Result<Expr, ParseError> {
        if matches!(
            self.peek().kind,
            TokenKind::Sym(Sym::Comma) | TokenKind::Sym(Sym::RParen)
        ) {
            Ok(Expr::DefaultArg)
        } else {
            self.parse_expr()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let t = self.bump();
        match t.kind {
            TokenKind::Int(v) => Ok(Expr::Int(v)),
            TokenKind::Float(v) => Ok(Expr::Float(v)),
            TokenKind::Str(mut s) => {
                // HolyC (like C) allows adjacent string literals to concatenate:
                //   "a" "b"  => "ab"
                while matches!(self.peek().kind, TokenKind::Str(_)) {
                    let t2 = self.bump();
                    if let TokenKind::Str(s2) = t2.kind {
                        s.push_str(&s2);
                    }
                }
                Ok(Expr::Str(s))
            }
            TokenKind::Char(v) => Ok(Expr::Char(v)),
            TokenKind::DolDocCmd(cmd) => {
                let cmd = cmd.trim();
                let (op_flags, args) = cmd.split_once(',').unwrap_or((cmd, ""));
                let op = op_flags.split('+').next().unwrap_or("").trim();

                fn parse_bi(args: &str) -> Option<u32> {
                    let needle = "BI=";
                    let idx = args.find(needle)? + needle.len();
                    let rest = args.get(idx..)?.trim_start();
                    let end = rest
                        .find(|c: char| c == ',' || c.is_ascii_whitespace())
                        .unwrap_or(rest.len());
                    rest[..end].trim().parse::<u32>().ok()
                }

                let Some(bin_num) = parse_bi(args) else {
                    return Err(ParseError {
                        span: t.span,
                        msg: "DolDoc cmd missing BI=<n>".to_string(),
                    });
                };

                match op {
                    "IB" => Ok(Expr::DolDocBinPtr {
                        file: t.span.file.clone(),
                        bin_num,
                    }),
                    "IS" => Ok(Expr::DolDocBinSize {
                        file: t.span.file.clone(),
                        bin_num,
                    }),
                    _ => Err(ParseError {
                        span: t.span,
                        msg: format!("unsupported DolDoc cmd in expression: {op}"),
                    }),
                }
            }
            TokenKind::Ident(name) => Ok(Expr::Var(name)),
            TokenKind::Sym(Sym::LParen) => {
                let e = self.parse_expr()?;
                self.expect_sym(Sym::RParen)?;
                Ok(e)
            }
            TokenKind::Sym(Sym::LBrace) => {
                let mut elems = Vec::new();
                if !self.eat_sym(Sym::RBrace) {
                    loop {
                        elems.push(self.parse_expr()?);
                        if self.eat_sym(Sym::Comma) {
                            if self.eat_sym(Sym::RBrace) {
                                break;
                            }
                            continue;
                        }
                        self.expect_sym(Sym::RBrace)?;
                        break;
                    }
                }
                Ok(Expr::InitList(elems))
            }
            _ => Err(ParseError {
                span: t.span,
                msg: "expected expression".to_string(),
            }),
        }
    }
}
