//! AST and recursive-descent parser for Ash.
//!
//! ```text
//! program  := fn*
//! fn       := "fn" ident "(" (param ("," param)*)? ")" ("->" type)? block
//! param    := ident ":" type
//! type     := "i64" | "bool"
//! block    := "{" stmt* "}"
//! stmt     := "let" "mut"? ident (":" type)? "=" expr ";"
//!           | ident "=" expr ";"
//!           | "return" expr? ";"
//!           | "if" expr block ("else" (block | if-stmt))?
//!           | "while" expr block
//!           | "print" "(" expr ")" ";"
//!           | expr ";"
//! expr     := or ; or := and ("||" and)* ; and := eq ("&&" eq)*
//! eq       := cmp (("=="|"!=") cmp)* ; cmp := add (("<"|"<="|">"|">=") add)*
//! add      := mul (("+"|"-") mul)* ; mul := unary (("*"|"/"|"%") unary)*
//! unary    := ("-"|"!") unary | primary
//! primary  := int | "true" | "false" | string | ident ("(" args ")")? | "(" expr ")"
//! ```

use crate::diag::{Diagnostic, Result, Span};
use crate::lexer::{Tok, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    I64,
    Bool,
    Unit,
    /// Only string literals have this type, and only `print` accepts it.
    Str,
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Ty::I64 => "i64",
            Ty::Bool => "bool",
            Ty::Unit => "()",
            Ty::Str => "str",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

impl BinOp {
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// Dense id assigned by the parser; sema records each expr's type by id.
    pub id: usize,
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Bool(bool),
    Str(String),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let {
        name: String,
        name_span: Span,
        mutable: bool,
        ty: Option<Ty>,
        init: Expr,
    },
    Assign {
        name: String,
        name_span: Span,
        value: Expr,
    },
    Expr(Expr),
    Return(Option<Expr>),
    If {
        cond: Expr,
        then: Block,
        els: Option<Block>,
    },
    While {
        cond: Expr,
        body: Block,
    },
    Print(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub ret: Ty,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub funcs: Vec<FnDecl>,
    /// Number of expression ids handed out.
    pub expr_count: usize,
}

struct Parser<'t> {
    toks: &'t [Token],
    pos: usize,
    next_id: usize,
}

impl<'t> Parser<'t> {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn peek_span(&self) -> Span {
        self.toks[self.pos].span
    }
    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: Tok, what: &str) -> Result<Token> {
        if self.at(&t) {
            Ok(self.bump())
        } else {
            Err(Diagnostic::new(
                "E002",
                self.peek_span(),
                format!("expected {what}, found {}", self.peek().describe()),
            ))
        }
    }
    fn ident(&mut self, what: &str) -> Result<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                let sp = self.bump().span;
                Ok((s, sp))
            }
            other => Err(Diagnostic::new(
                "E002",
                self.peek_span(),
                format!("expected {what}, found {}", other.describe()),
            )),
        }
    }
    fn expr_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn program(&mut self) -> Result<Program> {
        let mut funcs = Vec::new();
        while !self.at(&Tok::Eof) {
            if !self.at(&Tok::Fn) {
                return Err(Diagnostic::new(
                    "E002",
                    self.peek_span(),
                    format!(
                        "expected `fn` at top level, found {}",
                        self.peek().describe()
                    ),
                )
                .note("Ash programs are a sequence of function declarations"));
            }
            funcs.push(self.func()?);
        }
        Ok(Program {
            funcs,
            expr_count: self.next_id,
        })
    }

    fn ty(&mut self) -> Result<Ty> {
        let (name, span) = self.ident("a type")?;
        match name.as_str() {
            "i64" => Ok(Ty::I64),
            "bool" => Ok(Ty::Bool),
            _ => Err(
                Diagnostic::new("E002", span, format!("unknown type `{name}`"))
                    .note("Ash has two value types: `i64` and `bool`"),
            ),
        }
    }

    fn func(&mut self) -> Result<FnDecl> {
        let start = self.expect(Tok::Fn, "`fn`")?.span;
        let (name, name_span) = self.ident("a function name")?;
        self.expect(Tok::LParen, "`(` after the function name")?;
        let mut params = Vec::new();
        if !self.at(&Tok::RParen) {
            loop {
                let (pname, pspan) = self.ident("a parameter name")?;
                self.expect(Tok::Colon, "`:` followed by the parameter type")?;
                let ty = self.ty()?;
                params.push(Param {
                    name: pname,
                    ty,
                    span: pspan,
                });
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "`)` after parameters")?;
        let ret = if self.eat(&Tok::Arrow) {
            self.ty()?
        } else {
            Ty::Unit
        };
        let body = self.block()?;
        Ok(FnDecl {
            name,
            name_span,
            params,
            ret,
            span: start.to(body.span),
            body,
        })
    }

    fn block(&mut self) -> Result<Block> {
        let start = self.expect(Tok::LBrace, "`{`")?.span;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) {
            if self.at(&Tok::Eof) {
                return Err(Diagnostic::new(
                    "E002",
                    self.peek_span(),
                    "unexpected end of file, expected `}`",
                )
                .label("this block was never closed"));
            }
            stmts.push(self.stmt()?);
        }
        let end = self.bump().span;
        Ok(Block {
            stmts,
            span: start.to(end),
        })
    }

    fn stmt(&mut self) -> Result<Stmt> {
        let start = self.peek_span();
        match self.peek().clone() {
            Tok::Let => {
                self.bump();
                let mutable = self.eat(&Tok::Mut);
                let (name, name_span) = self.ident("a variable name")?;
                let ty = if self.eat(&Tok::Colon) {
                    Some(self.ty()?)
                } else {
                    None
                };
                self.expect(Tok::Assign, "`=` (Ash variables must be initialised)")?;
                let init = self.expr()?;
                let end = self.expect(Tok::Semi, "`;` after the initialiser")?.span;
                Ok(Stmt {
                    kind: StmtKind::Let {
                        name,
                        name_span,
                        mutable,
                        ty,
                        init,
                    },
                    span: start.to(end),
                })
            }
            Tok::Return => {
                self.bump();
                let value = if self.at(&Tok::Semi) {
                    None
                } else {
                    Some(self.expr()?)
                };
                let end = self.expect(Tok::Semi, "`;` after `return`")?.span;
                Ok(Stmt {
                    kind: StmtKind::Return(value),
                    span: start.to(end),
                })
            }
            Tok::If => {
                self.bump();
                let cond = self.expr()?;
                let then = self.block()?;
                let els = if self.eat(&Tok::Else) {
                    if self.at(&Tok::If) {
                        let s = self.stmt()?;
                        Some(Block {
                            span: s.span,
                            stmts: vec![s],
                        })
                    } else {
                        Some(self.block()?)
                    }
                } else {
                    None
                };
                let end = els.as_ref().map_or(then.span, |b| b.span);
                Ok(Stmt {
                    kind: StmtKind::If { cond, then, els },
                    span: start.to(end),
                })
            }
            Tok::While => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                let end = body.span;
                Ok(Stmt {
                    kind: StmtKind::While { cond, body },
                    span: start.to(end),
                })
            }
            Tok::Ident(name) if name == "print" && self.toks[self.pos + 1].tok == Tok::LParen => {
                self.bump();
                self.bump();
                let e = self.expr()?;
                self.expect(Tok::RParen, "`)` after the print argument")?;
                let end = self.expect(Tok::Semi, "`;` after `print(...)`")?.span;
                Ok(Stmt {
                    kind: StmtKind::Print(e),
                    span: start.to(end),
                })
            }
            Tok::Ident(name) if self.toks[self.pos + 1].tok == Tok::Assign => {
                let name_span = self.bump().span;
                self.bump();
                let value = self.expr()?;
                let end = self.expect(Tok::Semi, "`;` after the assignment")?.span;
                Ok(Stmt {
                    kind: StmtKind::Assign {
                        name,
                        name_span,
                        value,
                    },
                    span: start.to(end),
                })
            }
            _ => {
                let e = self.expr()?;
                let end = self
                    .expect(Tok::Semi, "`;` after the expression")
                    .map_err(|d| d.note("only function calls are useful as statements"))?
                    .span;
                Ok(Stmt {
                    kind: StmtKind::Expr(e),
                    span: start.to(end),
                })
            }
        }
    }

    fn expr(&mut self) -> Result<Expr> {
        self.binary(0)
    }

    /// Precedence climbing over the binary operator table.
    fn binary(&mut self, min_prec: u8) -> Result<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let (op, prec) = match self.peek() {
                Tok::OrOr => (BinOp::Or, 1),
                Tok::AndAnd => (BinOp::And, 2),
                Tok::EqEq => (BinOp::Eq, 3),
                Tok::Ne => (BinOp::Ne, 3),
                Tok::Lt => (BinOp::Lt, 4),
                Tok::Le => (BinOp::Le, 4),
                Tok::Gt => (BinOp::Gt, 4),
                Tok::Ge => (BinOp::Ge, 4),
                Tok::Plus => (BinOp::Add, 5),
                Tok::Minus => (BinOp::Sub, 5),
                Tok::Star => (BinOp::Mul, 6),
                Tok::Slash => (BinOp::Div, 6),
                Tok::Percent => (BinOp::Rem, 6),
                _ => return Ok(lhs),
            };
            if prec < min_prec {
                return Ok(lhs);
            }
            self.bump();
            let rhs = self.binary(prec + 1)?;
            let span = lhs.span.to(rhs.span);
            lhs = Expr {
                id: self.expr_id(),
                kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                span,
            };
        }
    }

    fn unary(&mut self) -> Result<Expr> {
        let start = self.peek_span();
        let op = match self.peek() {
            Tok::Minus => Some(UnOp::Neg),
            Tok::Bang => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let inner = self.unary()?;
            let span = start.to(inner.span);
            return Ok(Expr {
                id: self.expr_id(),
                kind: ExprKind::Unary(op, Box::new(inner)),
                span,
            });
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr> {
        let t = self.bump();
        let kind = match t.tok {
            Tok::Int(n) => ExprKind::Int(n),
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(Tok::RParen, "`)`")?;
                return Ok(e);
            }
            Tok::Ident(name) => {
                if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    if !self.at(&Tok::RParen) {
                        loop {
                            args.push(self.expr()?);
                            if !self.eat(&Tok::Comma) {
                                break;
                            }
                        }
                    }
                    let end = self.expect(Tok::RParen, "`)` after arguments")?.span;
                    return Ok(Expr {
                        id: self.expr_id(),
                        kind: ExprKind::Call(name, args),
                        span: t.span.to(end),
                    });
                }
                ExprKind::Var(name)
            }
            other => {
                return Err(Diagnostic::new(
                    "E002",
                    t.span,
                    format!("expected an expression, found {}", other.describe()),
                ));
            }
        };
        Ok(Expr {
            id: self.expr_id(),
            kind,
            span: t.span,
        })
    }
}

pub fn parse(toks: &[Token]) -> Result<Program> {
    let mut p = Parser {
        toks,
        pos: 0,
        next_id: 0,
    };
    p.program()
}

/// Pretty-prints the AST as an indented tree (for `--emit ast`).
pub fn dump(p: &Program) -> String {
    let mut s = String::new();
    for f in &p.funcs {
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.ty))
            .collect();
        s += &format!("fn {}({}) -> {}\n", f.name, params.join(", "), f.ret);
        dump_block(&f.body, 1, &mut s);
    }
    s
}

fn dump_block(b: &Block, depth: usize, s: &mut String) {
    for st in &b.stmts {
        let pad = "  ".repeat(depth);
        match &st.kind {
            StmtKind::Let {
                name,
                mutable,
                ty,
                init,
                ..
            } => {
                *s += &format!(
                    "{pad}let {}{name}{}= {}\n",
                    if *mutable { "mut " } else { "" },
                    ty.map_or(String::new(), |t| format!(": {t} ")),
                    dump_expr(init)
                );
            }
            StmtKind::Assign { name, value, .. } => {
                *s += &format!("{pad}{name} = {}\n", dump_expr(value))
            }
            StmtKind::Expr(e) => *s += &format!("{pad}{}\n", dump_expr(e)),
            StmtKind::Return(e) => {
                *s += &format!(
                    "{pad}return {}\n",
                    e.as_ref().map_or(String::new(), dump_expr)
                )
            }
            StmtKind::Print(e) => *s += &format!("{pad}print {}\n", dump_expr(e)),
            StmtKind::If { cond, then, els } => {
                *s += &format!("{pad}if {}\n", dump_expr(cond));
                dump_block(then, depth + 1, s);
                if let Some(e) = els {
                    *s += &format!("{pad}else\n");
                    dump_block(e, depth + 1, s);
                }
            }
            StmtKind::While { cond, body } => {
                *s += &format!("{pad}while {}\n", dump_expr(cond));
                dump_block(body, depth + 1, s);
            }
        }
    }
}

pub fn dump_expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(n) => n.to_string(),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Str(s) => format!("{s:?}"),
        ExprKind::Var(v) => v.clone(),
        ExprKind::Unary(op, x) => format!(
            "({}{})",
            if *op == UnOp::Neg { "-" } else { "!" },
            dump_expr(x)
        ),
        ExprKind::Binary(op, a, b) => format!("({} {} {})", dump_expr(a), op.text(), dump_expr(b)),
        ExprKind::Call(f, args) => format!(
            "{f}({})",
            args.iter().map(dump_expr).collect::<Vec<_>>().join(", ")
        ),
    }
}
