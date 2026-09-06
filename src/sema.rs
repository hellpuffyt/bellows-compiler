//! Semantic analysis: name resolution, type checking, mutability, arity,
//! missing-return analysis, and the `main` contract. Produces a `Typed`
//! side table the lowering pass consumes.

use std::collections::HashMap;

use crate::diag::{Diagnostic, Result, Span};
use crate::parser::{BinOp, Block, Expr, ExprKind, FnDecl, Program, Stmt, StmtKind, Ty, UnOp};

pub struct Typed {
    /// Type of every expression, indexed by `Expr::id`.
    pub expr_types: Vec<Ty>,
    /// Function name → index into `Program::funcs`.
    pub fn_index: HashMap<String, usize>,
}

struct Scope {
    vars: Vec<HashMap<String, (Ty, bool, Span)>>,
}

impl Scope {
    fn push(&mut self) {
        self.vars.push(HashMap::new());
    }
    fn pop(&mut self) {
        self.vars.pop();
    }
    fn declare(&mut self, name: &str, ty: Ty, mutable: bool, span: Span) -> Result<()> {
        let top = self.vars.last_mut().unwrap();
        if let Some((_, _, prev)) = top.get(name) {
            return Err(Diagnostic::new(
                "E016",
                span,
                format!("`{name}` is already declared in this scope"),
            )
            .note(format!("previous declaration at byte {}", prev.start)));
        }
        top.insert(name.to_string(), (ty, mutable, span));
        Ok(())
    }
    fn lookup(&self, name: &str) -> Option<(Ty, bool)> {
        self.vars
            .iter()
            .rev()
            .find_map(|m| m.get(name).map(|(t, m, _)| (*t, *m)))
    }
}

struct Checker<'p> {
    prog: &'p Program,
    fn_index: HashMap<String, usize>,
    types: Vec<Ty>,
    ret: Ty,
    scope: Scope,
}

pub fn check(prog: &Program) -> Result<Typed> {
    let mut fn_index = HashMap::new();
    for (i, f) in prog.funcs.iter().enumerate() {
        if f.name == "print" {
            return Err(Diagnostic::new(
                "E016",
                f.name_span,
                "`print` is a builtin and cannot be redefined",
            ));
        }
        if fn_index.insert(f.name.clone(), i).is_some() {
            return Err(Diagnostic::new(
                "E016",
                f.name_span,
                format!("function `{}` is defined twice", f.name),
            ));
        }
    }
    let Some(&main) = fn_index.get("main") else {
        return Err(
            Diagnostic::new("E017", Span::default(), "no `main` function")
                .note("every Ash program starts at `fn main() { ... }`"),
        );
    };
    let m = &prog.funcs[main];
    if !m.params.is_empty() || m.ret != Ty::Unit {
        return Err(Diagnostic::new(
            "E017",
            m.name_span,
            "`main` must take no parameters and return nothing",
        ));
    }
    let mut c = Checker {
        prog,
        fn_index,
        types: vec![Ty::Unit; prog.expr_count],
        ret: Ty::Unit,
        scope: Scope { vars: Vec::new() },
    };
    for f in &prog.funcs {
        c.func(f)?;
    }
    Ok(Typed {
        expr_types: c.types,
        fn_index: c.fn_index,
    })
}

impl Checker<'_> {
    fn func(&mut self, f: &FnDecl) -> Result<()> {
        self.ret = f.ret;
        self.scope = Scope { vars: Vec::new() };
        self.scope.push();
        for p in &f.params {
            self.scope.declare(&p.name, p.ty, false, p.span)?;
        }
        let returns = self.block(&f.body)?;
        self.scope.pop();
        if f.ret != Ty::Unit && !returns {
            return Err(Diagnostic::new(
                "E015",
                f.name_span,
                format!(
                    "function `{}` returns `{}` but not every path returns a value",
                    f.name, f.ret
                ),
            )
            .label("declared here")
            .note("add a `return` at the end, or make every `if` branch return"));
        }
        Ok(())
    }

    /// Returns whether the block definitely returns.
    fn block(&mut self, b: &Block) -> Result<bool> {
        self.scope.push();
        let mut returns = false;
        for s in &b.stmts {
            if returns {
                return Err(Diagnostic::new("E019", s.span, "unreachable statement")
                    .note("a `return` above it always exits the function"));
            }
            returns |= self.stmt(s)?;
        }
        self.scope.pop();
        Ok(returns)
    }

    fn stmt(&mut self, s: &Stmt) -> Result<bool> {
        match &s.kind {
            StmtKind::Let {
                name,
                name_span,
                mutable,
                ty,
                init,
            } => {
                let it = self.expr(init)?;
                if it == Ty::Unit {
                    return Err(Diagnostic::new(
                        "E011",
                        init.span,
                        "cannot bind a value of type `()`",
                    )
                    .label("this call returns nothing"));
                }
                if it == Ty::Str {
                    return Err(Diagnostic::new(
                        "E011",
                        init.span,
                        "string literals can only be used as arguments to `print`",
                    ));
                }
                if let Some(t) = ty
                    && *t != it
                {
                    return Err(Diagnostic::new(
                        "E011",
                        init.span,
                        format!(
                            "type mismatch: `{name}` is declared `{t}` but initialised with `{it}`"
                        ),
                    )
                    .label(format!("this has type `{it}`")));
                }
                self.scope.declare(name, it, *mutable, *name_span)?;
                Ok(false)
            }
            StmtKind::Assign {
                name,
                name_span,
                value,
            } => {
                let vt = self.expr(value)?;
                let Some((t, mutable)) = self.scope.lookup(name) else {
                    return Err(Diagnostic::new(
                        "E010",
                        *name_span,
                        format!("cannot find variable `{name}` in this scope"),
                    ));
                };
                if !mutable {
                    return Err(Diagnostic::new(
                        "E012",
                        *name_span,
                        format!("cannot assign twice to immutable variable `{name}`"),
                    )
                    .note(format!(
                        "declare it with `let mut {name}` to allow assignment"
                    )));
                }
                if t != vt {
                    return Err(Diagnostic::new(
                        "E011",
                        value.span,
                        format!("type mismatch: `{name}` is `{t}` but the value is `{vt}`"),
                    )
                    .label(format!("this has type `{vt}`")));
                }
                Ok(false)
            }
            StmtKind::Expr(e) => {
                let t = self.expr(e)?;
                if !matches!(e.kind, ExprKind::Call(..)) {
                    return Err(Diagnostic::new(
                        "E020",
                        e.span,
                        "expression statement has no effect",
                    )
                    .note("only function calls may be used as statements"));
                }
                let _ = t;
                Ok(false)
            }
            StmtKind::Return(v) => {
                match (v, self.ret) {
                    (None, Ty::Unit) => {}
                    (None, t) => {
                        return Err(Diagnostic::new(
                            "E011",
                            s.span,
                            format!("`return` without a value in a function returning `{t}`"),
                        ));
                    }
                    (Some(e), Ty::Unit) => {
                        return Err(Diagnostic::new(
                            "E011",
                            e.span,
                            "returning a value from a function declared without `->`",
                        )
                        .label("remove this value or add a return type"));
                    }
                    (Some(e), t) => {
                        let et = self.expr(e)?;
                        if et != t {
                            return Err(Diagnostic::new(
                                "E011",
                                e.span,
                                format!("type mismatch: expected `{t}`, found `{et}`"),
                            )
                            .label(format!("this has type `{et}`")));
                        }
                    }
                }
                Ok(true)
            }
            StmtKind::If { cond, then, els } => {
                self.cond(cond)?;
                let a = self.block(then)?;
                let b = match els {
                    Some(e) => self.block(e)?,
                    None => false,
                };
                Ok(a && b)
            }
            StmtKind::While { cond, body } => {
                self.cond(cond)?;
                self.block(body)?;
                Ok(false)
            }
            StmtKind::Print(e) => {
                let t = self.expr(e)?;
                if t == Ty::Unit {
                    return Err(Diagnostic::new(
                        "E011",
                        e.span,
                        "cannot print a value of type `()`",
                    ));
                }
                Ok(false)
            }
        }
    }

    fn cond(&mut self, e: &Expr) -> Result<()> {
        let t = self.expr(e)?;
        if t != Ty::Bool {
            return Err(Diagnostic::new(
                "E018",
                e.span,
                format!("condition must be `bool`, found `{t}`"),
            )
            .label(format!("this has type `{t}`")));
        }
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> Result<Ty> {
        let t = match &e.kind {
            ExprKind::Int(_) => Ty::I64,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Str(_) => Ty::Str,
            ExprKind::Var(name) => match self.scope.lookup(name) {
                Some((t, _)) => t,
                None => {
                    let mut d = Diagnostic::new(
                        "E010",
                        e.span,
                        format!("cannot find variable `{name}` in this scope"),
                    );
                    if self.fn_index.contains_key(name) {
                        d = d.note(format!(
                            "`{name}` is a function; call it with `{name}(...)`"
                        ));
                    }
                    return Err(d);
                }
            },
            ExprKind::Unary(op, x) => {
                let xt = self.expr(x)?;
                let want = if *op == UnOp::Neg { Ty::I64 } else { Ty::Bool };
                if xt != want {
                    return Err(Diagnostic::new(
                        "E011",
                        x.span,
                        format!(
                            "cannot apply `{}` to `{xt}`",
                            if *op == UnOp::Neg { "-" } else { "!" }
                        ),
                    )
                    .label(format!("expected `{want}`")));
                }
                want
            }
            ExprKind::Binary(op, a, b) => {
                let at = self.expr(a)?;
                let bt = self.expr(b)?;
                match op {
                    BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Rem
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge => {
                        for (t, x) in [(at, a), (bt, b)] {
                            if t != Ty::I64 {
                                return Err(Diagnostic::new(
                                    "E011",
                                    x.span,
                                    format!(
                                        "operator `{}` expects `i64` operands, found `{t}`",
                                        op.text()
                                    ),
                                )
                                .label(format!("this has type `{t}`")));
                            }
                        }
                        if matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge) {
                            Ty::Bool
                        } else {
                            Ty::I64
                        }
                    }
                    BinOp::Eq | BinOp::Ne => {
                        if at != bt || at == Ty::Str || at == Ty::Unit {
                            return Err(Diagnostic::new(
                                "E011",
                                e.span,
                                format!("cannot compare `{at}` with `{bt}`"),
                            ));
                        }
                        Ty::Bool
                    }
                    BinOp::And | BinOp::Or => {
                        for (t, x) in [(at, a), (bt, b)] {
                            if t != Ty::Bool {
                                return Err(Diagnostic::new(
                                    "E011",
                                    x.span,
                                    format!(
                                        "operator `{}` expects `bool` operands, found `{t}`",
                                        op.text()
                                    ),
                                )
                                .label(format!("this has type `{t}`")));
                            }
                        }
                        Ty::Bool
                    }
                }
            }
            ExprKind::Call(name, args) => {
                let Some(&idx) = self.fn_index.get(name) else {
                    let mut d =
                        Diagnostic::new("E014", e.span, format!("cannot find function `{name}`"));
                    if name == "print" {
                        d = d.note("`print` is a statement: `print(x);`");
                    }
                    return Err(d);
                };
                let f = &self.prog.funcs[idx];
                if f.params.len() != args.len() {
                    return Err(Diagnostic::new(
                        "E013",
                        e.span,
                        format!(
                            "`{name}` takes {} argument{} but {} {} supplied",
                            f.params.len(),
                            if f.params.len() == 1 { "" } else { "s" },
                            args.len(),
                            if args.len() == 1 { "was" } else { "were" }
                        ),
                    ));
                }
                let param_tys: Vec<Ty> = f.params.iter().map(|p| p.ty).collect();
                let ret = f.ret;
                for (arg, pt) in args.iter().zip(param_tys) {
                    let at = self.expr(arg)?;
                    if at != pt {
                        return Err(Diagnostic::new(
                            "E011",
                            arg.span,
                            format!("type mismatch: parameter is `{pt}`, argument is `{at}`"),
                        )
                        .label(format!("this has type `{at}`")));
                    }
                }
                ret
            }
        };
        self.types[e.id] = t;
        Ok(t)
    }
}
