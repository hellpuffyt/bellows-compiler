//! Three-address IR with basic blocks, lowering from the typed AST, a
//! printer (`--emit ir`), and an interpreter (`bellows interp`).
//!
//! Temps are single-assignment (each `%tN` is written by exactly one
//! instruction), locals and parameters live in numbered slots accessed by
//! explicit `load`/`store`. That split keeps the optimizer simple: temps
//! can be propagated globally, slots only within a block.

use std::fmt::Write as _;
use std::io::Write;

use crate::parser::{
    BinOp, Block as AstBlock, Expr, ExprKind, Program as Ast, Stmt, StmtKind, Ty, UnOp,
};
use crate::sema::Typed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operand {
    Temp(u32),
    Const(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
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
}

impl Op {
    pub fn text(self) -> &'static str {
        match self {
            Op::Add => "add",
            Op::Sub => "sub",
            Op::Mul => "mul",
            Op::Div => "div",
            Op::Rem => "rem",
            Op::Lt => "lt",
            Op::Le => "le",
            Op::Gt => "gt",
            Op::Ge => "ge",
            Op::Eq => "eq",
            Op::Ne => "ne",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instr {
    /// `%dst = load slot`
    Load {
        dst: u32,
        slot: u32,
    },
    /// `store slot, src`
    Store {
        slot: u32,
        src: Operand,
    },
    /// `%dst = op a, b`
    Bin {
        dst: u32,
        op: Op,
        a: Operand,
        b: Operand,
    },
    /// `%dst = neg a`
    Neg {
        dst: u32,
        a: Operand,
    },
    /// `%dst = not a` (boolean)
    Not {
        dst: u32,
        a: Operand,
    },
    /// `%dst = call f(args)`; dst is None for unit functions.
    Call {
        dst: Option<u32>,
        func: usize,
        args: Vec<Operand>,
    },
    PrintInt(Operand),
    PrintBool(Operand),
    /// Index into `Program::strings`.
    PrintStr(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    Jmp(u32),
    Br {
        cond: Operand,
        t: u32,
        f: u32,
    },
    Ret(Option<Operand>),
    /// Placeholder while a block is being built.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlock {
    pub instrs: Vec<Instr>,
    pub term: Term,
    /// Source line of the first statement lowered into this block (for `.loc`).
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub name: String,
    pub params: u32,
    pub slots: u32,
    pub temps: u32,
    pub blocks: Vec<BasicBlock>,
    pub returns_value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Program {
    pub funcs: Vec<Func>,
    pub strings: Vec<String>,
    pub main: usize,
}

// ------------------------------------------------------------- lowering --

struct Lower<'a> {
    typed: &'a Typed,
    func: Func,
    cur: u32,
    scopes: Vec<Vec<(String, u32)>>,
    strings: &'a mut Vec<String>,
    line_of: &'a dyn Fn(usize) -> u32,
}

impl Lower<'_> {
    fn new_block(&mut self) -> u32 {
        self.func.blocks.push(BasicBlock {
            instrs: Vec::new(),
            term: Term::None,
            line: 0,
        });
        (self.func.blocks.len() - 1) as u32
    }
    fn temp(&mut self) -> u32 {
        let t = self.func.temps;
        self.func.temps += 1;
        t
    }
    fn emit(&mut self, i: Instr) {
        self.func.blocks[self.cur as usize].instrs.push(i);
    }
    fn terminated(&self) -> bool {
        self.func.blocks[self.cur as usize].term != Term::None
    }
    fn term(&mut self, t: Term) {
        if !self.terminated() {
            self.func.blocks[self.cur as usize].term = t;
        }
    }
    fn slot(&mut self, name: &str) -> u32 {
        let s = self.func.slots;
        self.func.slots += 1;
        self.scopes.last_mut().unwrap().push((name.to_string(), s));
        s
    }
    fn lookup(&self, name: &str) -> u32 {
        self.scopes
            .iter()
            .rev()
            .find_map(|sc| sc.iter().rev().find(|(n, _)| n == name).map(|(_, s)| *s))
            .expect("sema checked")
    }

    fn block(&mut self, b: &AstBlock) {
        self.scopes.push(Vec::new());
        for s in &b.stmts {
            if self.terminated() {
                break;
            }
            self.stmt(s);
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        let line = (self.line_of)(s.span.start);
        if self.func.blocks[self.cur as usize].line == 0 {
            self.func.blocks[self.cur as usize].line = line;
        }
        match &s.kind {
            StmtKind::Let { name, init, .. } => {
                let v = self.expr(init);
                let slot = self.slot(name);
                self.emit(Instr::Store { slot, src: v });
            }
            StmtKind::Assign { name, value, .. } => {
                let v = self.expr(value);
                let slot = self.lookup(name);
                self.emit(Instr::Store { slot, src: v });
            }
            StmtKind::Expr(e) => {
                self.expr(e);
            }
            StmtKind::Return(v) => {
                let v = v.as_ref().map(|e| self.expr(e));
                self.term(Term::Ret(v));
            }
            StmtKind::Print(e) => match self.typed.expr_types[e.id] {
                Ty::Str => {
                    let ExprKind::Str(s) = &e.kind else {
                        unreachable!()
                    };
                    self.strings.push(s.clone());
                    let idx = self.strings.len() - 1;
                    self.emit(Instr::PrintStr(idx));
                }
                Ty::Bool => {
                    let v = self.expr(e);
                    self.emit(Instr::PrintBool(v));
                }
                _ => {
                    let v = self.expr(e);
                    self.emit(Instr::PrintInt(v));
                }
            },
            StmtKind::If { cond, then, els } => {
                let c = self.expr(cond);
                let t = self.new_block();
                let f = self.new_block();
                let join = self.new_block();
                self.term(Term::Br { cond: c, t, f });
                self.cur = t;
                self.block(then);
                self.term(Term::Jmp(join));
                self.cur = f;
                if let Some(e) = els {
                    self.block(e);
                }
                self.term(Term::Jmp(join));
                self.cur = join;
            }
            StmtKind::While { cond, body } => {
                let head = self.new_block();
                let b = self.new_block();
                let exit = self.new_block();
                self.term(Term::Jmp(head));
                self.cur = head;
                self.func.blocks[head as usize].line = line;
                let c = self.expr(cond);
                self.term(Term::Br {
                    cond: c,
                    t: b,
                    f: exit,
                });
                self.cur = b;
                self.block(body);
                self.term(Term::Jmp(head));
                self.cur = exit;
            }
        }
    }

    fn expr(&mut self, e: &Expr) -> Operand {
        match &e.kind {
            ExprKind::Int(n) => Operand::Const(*n),
            ExprKind::Bool(b) => Operand::Const(i64::from(*b)),
            ExprKind::Str(_) => Operand::Const(0),
            ExprKind::Var(name) => {
                let slot = self.lookup(name);
                let dst = self.temp();
                self.emit(Instr::Load { dst, slot });
                Operand::Temp(dst)
            }
            ExprKind::Unary(op, x) => {
                let a = self.expr(x);
                let dst = self.temp();
                self.emit(match op {
                    UnOp::Neg => Instr::Neg { dst, a },
                    UnOp::Not => Instr::Not { dst, a },
                });
                Operand::Temp(dst)
            }
            ExprKind::Binary(BinOp::And | BinOp::Or, a, b) => {
                // Short-circuit through a slot: result = a; if (a == shortcircuit) skip else result = b.
                let is_and = matches!(e.kind, ExprKind::Binary(BinOp::And, ..));
                let av = self.expr(a);
                let slot = self.slot("$sc");
                self.emit(Instr::Store { slot, src: av });
                let rhs = self.new_block();
                let join = self.new_block();
                self.term(if is_and {
                    Term::Br {
                        cond: av,
                        t: rhs,
                        f: join,
                    }
                } else {
                    Term::Br {
                        cond: av,
                        t: join,
                        f: rhs,
                    }
                });
                self.cur = rhs;
                let bv = self.expr(b);
                self.emit(Instr::Store { slot, src: bv });
                self.term(Term::Jmp(join));
                self.cur = join;
                let dst = self.temp();
                self.emit(Instr::Load { dst, slot });
                Operand::Temp(dst)
            }
            ExprKind::Binary(op, a, b) => {
                let av = self.expr(a);
                let bv = self.expr(b);
                let dst = self.temp();
                let op = match op {
                    BinOp::Add => Op::Add,
                    BinOp::Sub => Op::Sub,
                    BinOp::Mul => Op::Mul,
                    BinOp::Div => Op::Div,
                    BinOp::Rem => Op::Rem,
                    BinOp::Lt => Op::Lt,
                    BinOp::Le => Op::Le,
                    BinOp::Gt => Op::Gt,
                    BinOp::Ge => Op::Ge,
                    BinOp::Eq => Op::Eq,
                    BinOp::Ne => Op::Ne,
                    BinOp::And | BinOp::Or => unreachable!(),
                };
                self.emit(Instr::Bin {
                    dst,
                    op,
                    a: av,
                    b: bv,
                });
                Operand::Temp(dst)
            }
            ExprKind::Call(name, args) => {
                let vals: Vec<Operand> = args.iter().map(|a| self.expr(a)).collect();
                let func = self.typed.fn_index[name];
                let dst = if self.typed.expr_types[e.id] == Ty::Unit {
                    None
                } else {
                    Some(self.temp())
                };
                self.emit(Instr::Call {
                    dst,
                    func,
                    args: vals,
                });
                dst.map_or(Operand::Const(0), Operand::Temp)
            }
        }
    }
}

/// Lowers a checked program to IR.
pub fn lower(ast: &Ast, typed: &Typed, line_of: &dyn Fn(usize) -> u32) -> Program {
    let mut strings = Vec::new();
    let mut funcs = Vec::new();
    for f in &ast.funcs {
        let mut l = Lower {
            typed,
            func: Func {
                name: f.name.clone(),
                params: f.params.len() as u32,
                slots: 0,
                temps: 0,
                blocks: Vec::new(),
                returns_value: f.ret != Ty::Unit,
            },
            cur: 0,
            scopes: vec![Vec::new()],
            strings: &mut strings,
            line_of,
        };
        l.new_block();
        for p in &f.params {
            l.slot(&p.name);
        }
        l.func.blocks[0].line = line_of(f.span.start);
        l.block(&f.body);
        // Falling off the end of a unit function is an implicit return; sema
        // guarantees value-returning functions never get here on a real path.
        for b in &mut l.func.blocks {
            if b.term == Term::None {
                b.term = Term::Ret(None);
            }
        }
        funcs.push(l.func);
    }
    Program {
        main: typed.fn_index["main"],
        funcs,
        strings,
    }
}

// -------------------------------------------------------------- printing --

impl std::fmt::Display for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operand::Temp(t) => write!(f, "%t{t}"),
            Operand::Const(c) => write!(f, "{c}"),
        }
    }
}

pub fn dump(p: &Program) -> String {
    let mut s = String::new();
    for (i, st) in p.strings.iter().enumerate() {
        let _ = writeln!(s, "str{i} = {st:?}");
    }
    for f in &p.funcs {
        let _ = writeln!(
            s,
            "fn {}(params={}, slots={}, temps={}) {{",
            f.name, f.params, f.slots, f.temps
        );
        for (i, b) in f.blocks.iter().enumerate() {
            let _ = writeln!(s, "  b{i}:");
            for ins in &b.instrs {
                let line = match ins {
                    Instr::Load { dst, slot } => format!("%t{dst} = load s{slot}"),
                    Instr::Store { slot, src } => format!("store s{slot}, {src}"),
                    Instr::Bin { dst, op, a, b } => format!("%t{dst} = {} {a}, {b}", op.text()),
                    Instr::Neg { dst, a } => format!("%t{dst} = neg {a}"),
                    Instr::Not { dst, a } => format!("%t{dst} = not {a}"),
                    Instr::Call { dst, func, args } => {
                        let args: Vec<String> = args.iter().map(ToString::to_string).collect();
                        let callee = &p.funcs[*func].name;
                        match dst {
                            Some(d) => format!("%t{d} = call {callee}({})", args.join(", ")),
                            None => format!("call {callee}({})", args.join(", ")),
                        }
                    }
                    Instr::PrintInt(v) => format!("print.int {v}"),
                    Instr::PrintBool(v) => format!("print.bool {v}"),
                    Instr::PrintStr(i) => format!("print.str str{i}"),
                };
                let _ = writeln!(s, "    {line}");
            }
            let term = match &b.term {
                Term::Jmp(t) => format!("jmp b{t}"),
                Term::Br { cond, t, f } => format!("br {cond}, b{t}, b{f}"),
                Term::Ret(Some(v)) => format!("ret {v}"),
                Term::Ret(None) => "ret".into(),
                Term::None => "<unterminated>".into(),
            };
            let _ = writeln!(s, "    {term}");
        }
        let _ = writeln!(s, "}}");
    }
    s
}

// ----------------------------------------------------------- interpreter --

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    DivideByZero,
    Overflow,
    StackOverflow,
    StepLimit,
    Io,
}

impl std::fmt::Display for Trap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Trap::DivideByZero => "division by zero",
            Trap::Overflow => "integer overflow",
            Trap::StackOverflow => "stack overflow (call depth limit)",
            Trap::StepLimit => "step limit reached",
            Trap::Io => "write failed",
        })
    }
}

pub struct Interp<'a, W: Write> {
    prog: &'a Program,
    out: W,
    depth: usize,
    pub max_depth: usize,
    /// Abort with `Trap::StepLimit` after this many instructions.
    pub max_steps: u64,
    /// Instructions executed (for benchmarks and tests).
    pub steps: u64,
}

impl<'a, W: Write> Interp<'a, W> {
    pub fn new(prog: &'a Program, out: W) -> Self {
        Interp {
            prog,
            out,
            depth: 0,
            max_depth: 100_000,
            max_steps: u64::MAX,
            steps: 0,
        }
    }

    pub fn run(&mut self) -> Result<(), Trap> {
        self.call(self.prog.main, &[]).map(|_| ())
    }

    fn call(&mut self, fi: usize, args: &[i64]) -> Result<i64, Trap> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(Trap::StackOverflow);
        }
        let f = &self.prog.funcs[fi];
        let mut slots = vec![0i64; f.slots as usize];
        slots[..args.len()].copy_from_slice(args);
        let mut temps = vec![0i64; f.temps as usize];
        let mut b = 0usize;
        let val = |o: Operand, temps: &[i64]| match o {
            Operand::Temp(t) => temps[t as usize],
            Operand::Const(c) => c,
        };
        loop {
            let blk = &f.blocks[b];
            for ins in &blk.instrs {
                self.steps += 1;
                if self.steps > self.max_steps {
                    return Err(Trap::StepLimit);
                }
                match ins {
                    Instr::Load { dst, slot } => temps[*dst as usize] = slots[*slot as usize],
                    Instr::Store { slot, src } => slots[*slot as usize] = val(*src, &temps),
                    Instr::Bin { dst, op, a, b } => {
                        let (x, y) = (val(*a, &temps), val(*b, &temps));
                        temps[*dst as usize] = eval(*op, x, y)?;
                    }
                    Instr::Neg { dst, a } => {
                        temps[*dst as usize] =
                            val(*a, &temps).checked_neg().ok_or(Trap::Overflow)?
                    }
                    Instr::Not { dst, a } => temps[*dst as usize] = i64::from(val(*a, &temps) == 0),
                    Instr::Call { dst, func, args } => {
                        let argv: Vec<i64> = args.iter().map(|a| val(*a, &temps)).collect();
                        let r = self.call(*func, &argv)?;
                        if let Some(d) = dst {
                            temps[*d as usize] = r;
                        }
                    }
                    Instr::PrintInt(v) => {
                        writeln!(self.out, "{}", val(*v, &temps)).map_err(|_| Trap::Io)?
                    }
                    Instr::PrintBool(v) => {
                        writeln!(self.out, "{}", val(*v, &temps) != 0).map_err(|_| Trap::Io)?
                    }
                    Instr::PrintStr(i) => {
                        writeln!(self.out, "{}", self.prog.strings[*i]).map_err(|_| Trap::Io)?
                    }
                }
            }
            self.steps += 1;
            match &blk.term {
                Term::Jmp(t) => b = *t as usize,
                Term::Br { cond, t, f: fb } => {
                    b = if val(*cond, &temps) != 0 {
                        *t as usize
                    } else {
                        *fb as usize
                    }
                }
                Term::Ret(v) => {
                    self.depth -= 1;
                    return Ok(v.map_or(0, |v| val(v, &temps)));
                }
                Term::None => unreachable!("unterminated block"),
            }
        }
    }
}

/// Shared by the interpreter and the constant folder, so folding can never
/// disagree with execution (both trap the same way).
pub fn eval(op: Op, x: i64, y: i64) -> Result<i64, Trap> {
    Ok(match op {
        Op::Add => x.checked_add(y).ok_or(Trap::Overflow)?,
        Op::Sub => x.checked_sub(y).ok_or(Trap::Overflow)?,
        Op::Mul => x.checked_mul(y).ok_or(Trap::Overflow)?,
        Op::Div => {
            if y == 0 {
                return Err(Trap::DivideByZero);
            }
            x.checked_div(y).ok_or(Trap::Overflow)?
        }
        Op::Rem => {
            if y == 0 {
                return Err(Trap::DivideByZero);
            }
            x.checked_rem(y).ok_or(Trap::Overflow)?
        }
        Op::Lt => i64::from(x < y),
        Op::Le => i64::from(x <= y),
        Op::Gt => i64::from(x > y),
        Op::Ge => i64::from(x >= y),
        Op::Eq => i64::from(x == y),
        Op::Ne => i64::from(x != y),
    })
}
