//! x86-64 code generation (GNU assembler, AT&T syntax) for Linux (System V)
//! and Windows (Microsoft x64). Internal calls use Bellows' own convention
//! (arguments pushed right-to-left, result in `%rax`); only calls into
//! libc (`printf`, `puts`, `exit`) follow the platform ABI. Every slot and
//! temp lives in the stack frame — simple, correct, and easy to read in
//! `--emit asm`. Integer overflow and division by zero trap at run time
//! with the same semantics as the interpreter.

use std::fmt::Write;

use crate::ir::{Func, Instr, Op, Operand, Program, Term};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Linux,
    Windows,
}

impl Target {
    pub fn host() -> Target {
        if cfg!(windows) {
            Target::Windows
        } else {
            Target::Linux
        }
    }
    pub fn parse(s: &str) -> Option<Target> {
        match s {
            "linux" | "linux-x86_64" => Some(Target::Linux),
            "windows" | "windows-x86_64" | "mingw" => Some(Target::Windows),
            _ => None,
        }
    }
}

struct Gen<'a> {
    out: String,
    target: Target,
    debug: bool,
    prog: &'a Program,
}

impl Gen<'_> {
    fn line(&mut self, s: &str) {
        self.out.push_str("    ");
        self.out.push_str(s);
        self.out.push('\n');
    }
    fn label(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push_str(":\n");
    }
    fn slot_off(_f: &Func, slot: u32) -> i64 {
        -8 * (slot as i64 + 1)
    }
    fn temp_off(f: &Func, t: u32) -> i64 {
        -8 * (f.slots as i64 + t as i64 + 1)
    }
    fn load(&mut self, f: &Func, o: Operand, reg: &str) {
        match o {
            Operand::Const(c) => {
                if i32::try_from(c).is_ok() {
                    self.line(&format!("movq ${c}, {reg}"));
                } else {
                    self.line(&format!("movabsq ${c}, {reg}"));
                }
            }
            Operand::Temp(t) => self.line(&format!("movq {}(%rbp), {reg}", Self::temp_off(f, t))),
        }
    }
    fn store_temp(&mut self, f: &Func, t: u32, reg: &str) {
        self.line(&format!("movq {reg}, {}(%rbp)", Self::temp_off(f, t)));
    }

    /// Calls a libc function with the stack 16-byte aligned. `%rbx` is
    /// callee-saved on both ABIs, so it can carry the old `%rsp` across.
    fn libc_call(&mut self, name: &str) {
        self.line("pushq %rbx");
        self.line("movq %rsp, %rbx");
        self.line("andq $-16, %rsp");
        match self.target {
            Target::Linux => {
                self.line("xorl %eax, %eax");
                self.line(&format!("call {name}@PLT"));
            }
            Target::Windows => {
                self.line("subq $32, %rsp");
                self.line(&format!("call {name}"));
            }
        }
        self.line("movq %rbx, %rsp");
        self.line("popq %rbx");
    }

    fn arg_regs(&self) -> (&'static str, &'static str, &'static str) {
        match self.target {
            Target::Linux => ("%rdi", "%rsi", "%rdx"),
            Target::Windows => ("%rcx", "%rdx", "%r8"),
        }
    }

    fn func(&mut self, f: &Func) {
        let name = format!("ash_{}", f.name);
        if self.target == Target::Linux {
            self.line(&format!(".type {name}, @function"));
        }
        self.label(&name);
        let frame = (8 * (f.slots as u64 + f.temps as u64)).div_ceil(16) * 16;
        self.line("pushq %rbp");
        self.line("movq %rsp, %rbp");
        if frame > 0 {
            self.line(&format!("subq ${frame}, %rsp"));
        }
        for p in 0..f.params {
            self.line(&format!("movq {}(%rbp), %rax", 16 + 8 * p as i64));
            self.line(&format!("movq %rax, {}(%rbp)", Self::slot_off(f, p)));
        }
        let (a0, a1, a2) = self.arg_regs();
        for (bi, b) in f.blocks.iter().enumerate() {
            self.label(&format!(".L{}_b{bi}", f.name));
            if self.debug && b.line > 0 {
                self.line(&format!(".loc 1 {} 0", b.line));
            }
            for ins in &b.instrs {
                match ins {
                    Instr::Load { dst, slot } => {
                        self.line(&format!("movq {}(%rbp), %rax", Self::slot_off(f, *slot)));
                        self.store_temp(f, *dst, "%rax");
                    }
                    Instr::Store { slot, src } => {
                        self.load(f, *src, "%rax");
                        self.line(&format!("movq %rax, {}(%rbp)", Self::slot_off(f, *slot)));
                    }
                    Instr::Bin { dst, op, a, b } => {
                        self.load(f, *a, "%rax");
                        self.load(f, *b, "%rcx");
                        match op {
                            Op::Add => {
                                self.line("addq %rcx, %rax");
                                self.line("jo .Ltrap_overflow");
                            }
                            Op::Sub => {
                                self.line("subq %rcx, %rax");
                                self.line("jo .Ltrap_overflow");
                            }
                            Op::Mul => {
                                self.line("imulq %rcx, %rax");
                                self.line("jo .Ltrap_overflow");
                            }
                            Op::Div | Op::Rem => {
                                self.line("testq %rcx, %rcx");
                                self.line("jz .Ltrap_divzero");
                                // MIN / -1 overflows (and faults in idiv): trap explicitly.
                                self.line("cmpq $-1, %rcx");
                                self.line(&format!("jne .L{}_div{bi}_{}", f.name, dst));
                                self.line("movabsq $-9223372036854775808, %rdx");
                                self.line("cmpq %rdx, %rax");
                                self.line("je .Ltrap_overflow");
                                self.label(&format!(".L{}_div{bi}_{}", f.name, dst));
                                self.line("cqto");
                                self.line("idivq %rcx");
                                if *op == Op::Rem {
                                    self.line("movq %rdx, %rax");
                                }
                            }
                            Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Eq | Op::Ne => {
                                self.line("cmpq %rcx, %rax");
                                let set = match op {
                                    Op::Lt => "setl",
                                    Op::Le => "setle",
                                    Op::Gt => "setg",
                                    Op::Ge => "setge",
                                    Op::Eq => "sete",
                                    _ => "setne",
                                };
                                self.line(&format!("{set} %al"));
                                self.line("movzbq %al, %rax");
                            }
                        }
                        self.store_temp(f, *dst, "%rax");
                    }
                    Instr::Neg { dst, a } => {
                        self.load(f, *a, "%rax");
                        self.line("negq %rax");
                        self.line("jo .Ltrap_overflow");
                        self.store_temp(f, *dst, "%rax");
                    }
                    Instr::Not { dst, a } => {
                        self.load(f, *a, "%rax");
                        self.line("xorq $1, %rax");
                        self.store_temp(f, *dst, "%rax");
                    }
                    Instr::Call { dst, func, args } => {
                        for a in args.iter().rev() {
                            self.load(f, *a, "%rax");
                            self.line("pushq %rax");
                        }
                        self.line(&format!("call ash_{}", self.prog.funcs[*func].name));
                        if !args.is_empty() {
                            self.line(&format!("addq ${}, %rsp", 8 * args.len()));
                        }
                        if let Some(d) = dst {
                            self.store_temp(f, *d, "%rax");
                        }
                    }
                    Instr::PrintInt(v) => {
                        self.load(f, *v, a1);
                        self.line(&format!("leaq .Lfmt_int(%rip), {a0}"));
                        self.libc_call("printf");
                    }
                    Instr::PrintBool(v) => {
                        self.load(f, *v, "%rax");
                        self.line(&format!("leaq .Lstr_true(%rip), {a1}"));
                        self.line(&format!("leaq .Lstr_false(%rip), {a2}"));
                        self.line("testq %rax, %rax");
                        self.line(&format!("cmovzq {a2}, {a1}"));
                        self.line(&format!("leaq .Lfmt_str(%rip), {a0}"));
                        self.libc_call("printf");
                    }
                    Instr::PrintStr(i) => {
                        self.line(&format!("leaq .Lstr{i}(%rip), {a0}"));
                        self.libc_call("puts");
                    }
                }
            }
            match &b.term {
                Term::Jmp(t) => self.line(&format!("jmp .L{}_b{t}", f.name)),
                Term::Br { cond, t, f: fb } => {
                    self.load(f, *cond, "%rax");
                    self.line("testq %rax, %rax");
                    self.line(&format!("jne .L{}_b{t}", f.name));
                    self.line(&format!("jmp .L{}_b{fb}", f.name));
                }
                Term::Ret(v) => {
                    match v {
                        Some(v) => self.load(f, *v, "%rax"),
                        None => self.line("xorl %eax, %eax"),
                    }
                    self.line("movq %rbp, %rsp");
                    self.line("popq %rbp");
                    self.line("ret");
                }
                Term::None => unreachable!(),
            }
        }
        if self.target == Target::Linux {
            self.line(&format!(".size {name}, .-{name}"));
        }
        self.out.push('\n');
    }
}

/// Emits a complete assembly file. `debug_file` enables DWARF line info.
pub fn generate(p: &Program, target: Target, debug_file: Option<&str>) -> String {
    let mut g = Gen {
        out: String::new(),
        target,
        debug: debug_file.is_some(),
        prog: p,
    };
    let _ = writeln!(
        g.out,
        "# generated by bellows — target {}",
        match target {
            Target::Linux => "x86_64-linux (System V)",
            Target::Windows => "x86_64-windows (MS x64)",
        }
    );
    if let Some(file) = debug_file {
        let _ = writeln!(g.out, "    .file 1 \"{}\"", file.replace('\\', "/"));
    }
    g.line(".text");
    for f in &p.funcs {
        g.func(f);
    }
    // Entry point: the C runtime calls `main`; we call the user's main.
    g.line(".globl main");
    if target == Target::Linux {
        g.line(".type main, @function");
    }
    g.label("main");
    g.line("pushq %rbp");
    g.line("movq %rsp, %rbp");
    g.line(&format!("call ash_{}", p.funcs[p.main].name));
    g.line("xorl %eax, %eax");
    g.line("popq %rbp");
    g.line("ret");
    g.out.push('\n');
    // Runtime traps.
    for (label, msg) in [
        (".Ltrap_overflow", ".Lmsg_overflow"),
        (".Ltrap_divzero", ".Lmsg_divzero"),
    ] {
        g.label(label);
        let (a0, _, _) = g.arg_regs();
        g.line(&format!("leaq {msg}(%rip), {a0}"));
        g.libc_call("puts");
        g.line(&format!(
            "movl $101, {}",
            if target == Target::Linux {
                "%edi"
            } else {
                "%ecx"
            }
        ));
        g.libc_call("exit");
    }
    g.out.push('\n');
    match target {
        Target::Linux => g.line(".section .rodata"),
        Target::Windows => g.line(".section .rdata,\"dr\""),
    }
    g.label(".Lfmt_int");
    g.line(".asciz \"%lld\\n\"");
    g.label(".Lfmt_str");
    g.line(".asciz \"%s\\n\"");
    g.label(".Lstr_true");
    g.line(".asciz \"true\"");
    g.label(".Lstr_false");
    g.line(".asciz \"false\"");
    g.label(".Lmsg_overflow");
    g.line(".asciz \"runtime error: integer overflow\"");
    g.label(".Lmsg_divzero");
    g.line(".asciz \"runtime error: division by zero\"");
    for (i, s) in p.strings.iter().enumerate() {
        g.label(&format!(".Lstr{i}"));
        let escaped: String = s
            .bytes()
            .map(|b| match b {
                b'"' => "\\\"".to_string(),
                b'\\' => "\\\\".to_string(),
                b'\n' => "\\n".to_string(),
                b'\t' => "\\t".to_string(),
                32..=126 => (b as char).to_string(),
                _ => format!("\\{b:03o}"),
            })
            .collect();
        g.line(&format!(".asciz \"{escaped}\""));
    }
    if target == Target::Linux {
        g.line(".section .note.GNU-stack,\"\",@progbits");
    }
    g.out
}
