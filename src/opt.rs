//! IR optimizer. Passes run to a fixpoint:
//!
//! 1. constant folding + propagation of temps (temps are single-assignment,
//!    so a temp known to be constant is constant everywhere);
//! 2. algebraic simplification (`x+0`, `x*1`, `x*0`, `x-0`, `x/1`);
//! 3. store-to-load forwarding within a block (locals have no aliases);
//! 4. dead temp elimination (pure instructions whose result is unused);
//! 5. dead store elimination (slots never loaded anywhere in the function);
//! 6. branch folding (`br const`), jump threading through empty blocks,
//!    merging a block into its unique predecessor, unreachable block removal.
//!
//! Every pass preserves trapping behaviour: folding uses the interpreter's
//! `eval`, and an operation that would trap is left in place to trap at
//! run time.

use std::collections::{HashMap, HashSet};

use crate::ir::{Func, Instr, Op, Operand, Program, Term, eval};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub folded: usize,
    pub forwarded: usize,
    pub dead_temps: usize,
    pub dead_stores: usize,
    pub branches_folded: usize,
    pub blocks_removed: usize,
    pub jumps_threaded: usize,
}

pub fn optimize(p: &mut Program) -> Stats {
    let mut stats = Stats::default();
    for f in &mut p.funcs {
        loop {
            let before = stats;
            fold_and_propagate(f, &mut stats);
            forward_stores(f, &mut stats);
            dead_temps(f, &mut stats);
            dead_stores(f, &mut stats);
            fold_branches(f, &mut stats);
            thread_jumps(f, &mut stats);
            merge_blocks(f, &mut stats);
            remove_unreachable(f, &mut stats);
            if stats == before {
                break;
            }
        }
    }
    stats
}

fn operands_mut(ins: &mut Instr) -> Vec<&mut Operand> {
    match ins {
        Instr::Store { src, .. } => vec![src],
        Instr::Bin { a, b, .. } => vec![a, b],
        Instr::Neg { a, .. } | Instr::Not { a, .. } => vec![a],
        Instr::Call { args, .. } => args.iter_mut().collect(),
        Instr::PrintInt(v) | Instr::PrintBool(v) => vec![v],
        Instr::Load { .. } | Instr::PrintStr(_) => vec![],
    }
}

fn term_operands_mut(t: &mut Term) -> Vec<&mut Operand> {
    match t {
        Term::Br { cond, .. } => vec![cond],
        Term::Ret(Some(v)) => vec![v],
        _ => vec![],
    }
}

/// Replaces every use of an aliased temp, in every block.
fn rewrite_all(f: &mut Func, alias: &HashMap<u32, Operand>) {
    if alias.is_empty() {
        return;
    }
    // Aliases can chain (t1 -> t0 -> 5); follow them to the end.
    let fix = |o: &mut Operand| {
        let mut hops = 0;
        while let Operand::Temp(t) = o
            && let Some(v) = alias.get(t)
            && hops < 64
        {
            *o = *v;
            hops += 1;
        }
    };
    for b in &mut f.blocks {
        for ins in &mut b.instrs {
            operands_mut(ins).into_iter().for_each(fix);
        }
        term_operands_mut(&mut b.term).into_iter().for_each(fix);
    }
}

fn fold_and_propagate(f: &mut Func, stats: &mut Stats) {
    // temp → constant or another temp. Single-assignment temps make this a
    // global fact once discovered.
    let mut alias: HashMap<u32, Operand> = HashMap::new();
    for b in &mut f.blocks {
        let mut keep = Vec::with_capacity(b.instrs.len());
        for mut ins in std::mem::take(&mut b.instrs) {
            for o in operands_mut(&mut ins) {
                if let Operand::Temp(t) = o
                    && let Some(v) = alias.get(t)
                {
                    *o = *v;
                }
            }
            let folded: Option<Operand> = match &ins {
                Instr::Bin { op, a, b, .. } => match (*op, *a, *b) {
                    (op, Operand::Const(x), Operand::Const(y)) => {
                        eval(op, x, y).ok().map(Operand::Const)
                    }
                    (Op::Add, x, Operand::Const(0))
                    | (Op::Add, Operand::Const(0), x)
                    | (Op::Sub, x, Operand::Const(0)) => Some(x),
                    (Op::Mul, x, Operand::Const(1))
                    | (Op::Mul, Operand::Const(1), x)
                    | (Op::Div, x, Operand::Const(1)) => Some(x),
                    (Op::Mul, _, Operand::Const(0)) | (Op::Mul, Operand::Const(0), _) => {
                        Some(Operand::Const(0))
                    }
                    _ => None,
                },
                Instr::Neg {
                    a: Operand::Const(x),
                    ..
                } => x.checked_neg().map(Operand::Const),
                Instr::Not {
                    a: Operand::Const(x),
                    ..
                } => Some(Operand::Const(i64::from(*x == 0))),
                _ => None,
            };
            if let Some(v) = folded {
                let dst = match &ins {
                    Instr::Bin { dst, .. } | Instr::Neg { dst, .. } | Instr::Not { dst, .. } => {
                        *dst
                    }
                    _ => unreachable!(),
                };
                alias.insert(dst, v);
                stats.folded += 1;
                continue;
            }
            keep.push(ins);
        }
        b.instrs = keep;
        for o in term_operands_mut(&mut b.term) {
            if let Operand::Temp(t) = o
                && let Some(v) = alias.get(t)
            {
                *o = *v;
            }
        }
    }
    rewrite_all(f, &alias);
}

/// Within a block, a load of a slot that was just stored (with no
/// intervening store to that slot) reads the stored operand instead, and a
/// second load of an unchanged slot reuses the first.
fn forward_stores(f: &mut Func, stats: &mut Stats) {
    let mut alias: HashMap<u32, Operand> = HashMap::new();
    for b in &mut f.blocks {
        let mut known: HashMap<u32, Operand> = HashMap::new();
        let mut keep = Vec::with_capacity(b.instrs.len());
        for ins in std::mem::take(&mut b.instrs) {
            match &ins {
                Instr::Store { slot, src } => {
                    known.insert(*slot, *src);
                }
                Instr::Load { dst, slot } => {
                    if let Some(v) = known.get(slot) {
                        alias.insert(*dst, *v);
                        stats.forwarded += 1;
                        continue;
                    }
                    known.insert(*slot, Operand::Temp(*dst));
                }
                _ => {}
            }
            keep.push(ins);
        }
        b.instrs = keep;
    }
    rewrite_all(f, &alias);
}

fn used_temps(f: &Func) -> HashSet<u32> {
    let mut used = HashSet::new();
    let mut note = |o: &Operand| {
        if let Operand::Temp(t) = o {
            used.insert(*t);
        }
    };
    for b in &f.blocks {
        for ins in &b.instrs {
            match ins {
                Instr::Store { src, .. } => note(src),
                Instr::Bin { a, b, .. } => {
                    note(a);
                    note(b);
                }
                Instr::Neg { a, .. } | Instr::Not { a, .. } => note(a),
                Instr::Call { args, .. } => args.iter().for_each(&mut note),
                Instr::PrintInt(v) | Instr::PrintBool(v) => note(v),
                Instr::Load { .. } | Instr::PrintStr(_) => {}
            }
        }
        match &b.term {
            Term::Br { cond, .. } => note(cond),
            Term::Ret(Some(v)) => note(v),
            _ => {}
        }
    }
    used
}

fn dead_temps(f: &mut Func, stats: &mut Stats) {
    let used = used_temps(f);
    for b in &mut f.blocks {
        let before = b.instrs.len();
        b.instrs.retain(|i| match i {
            // Division may trap: keep it even if unused.
            Instr::Bin { dst, op, .. } if !matches!(op, Op::Div | Op::Rem) => used.contains(dst),
            Instr::Load { dst, .. } | Instr::Neg { dst, .. } | Instr::Not { dst, .. } => {
                used.contains(dst)
            }
            _ => true, // calls, prints, stores, trapping ops have effects
        });
        stats.dead_temps += before - b.instrs.len();
    }
}

fn dead_stores(f: &mut Func, stats: &mut Stats) {
    let mut loaded = HashSet::new();
    for b in &f.blocks {
        for ins in &b.instrs {
            if let Instr::Load { slot, .. } = ins {
                loaded.insert(*slot);
            }
        }
    }
    for b in &mut f.blocks {
        let before = b.instrs.len();
        b.instrs
            .retain(|i| !matches!(i, Instr::Store { slot, .. } if !loaded.contains(slot)));
        stats.dead_stores += before - b.instrs.len();
    }
}

fn fold_branches(f: &mut Func, stats: &mut Stats) {
    for b in &mut f.blocks {
        if let Term::Br {
            cond: Operand::Const(c),
            t,
            f: fb,
        } = b.term
        {
            b.term = Term::Jmp(if c != 0 { t } else { fb });
            stats.branches_folded += 1;
        }
    }
}

/// A block that is empty and just jumps somewhere can be skipped by its
/// predecessors.
fn thread_jumps(f: &mut Func, stats: &mut Stats) {
    let targets: Vec<Option<u32>> = f
        .blocks
        .iter()
        .map(|b| match (&b.instrs.is_empty(), &b.term) {
            (true, Term::Jmp(t)) => Some(*t),
            _ => None,
        })
        .collect();
    let resolve = |mut x: u32| {
        let mut hops = 0;
        while let Some(Some(t)) = targets.get(x as usize) {
            if *t == x || hops > 64 {
                break;
            }
            x = *t;
            hops += 1;
        }
        x
    };
    // An empty entry block that only jumps to a block with no other
    // predecessor can absorb that block.
    if let (true, Term::Jmp(t)) = (f.blocks[0].instrs.is_empty(), f.blocks[0].term.clone())
        && t != 0
    {
        let preds = f
            .blocks
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                *i != 0
                    && match &b.term {
                        Term::Jmp(x) => *x == t,
                        Term::Br { t: a, f: c, .. } => *a == t || *c == t,
                        _ => false,
                    }
            })
            .count();
        if preds == 0 {
            let moved = std::mem::replace(
                &mut f.blocks[t as usize],
                crate::ir::BasicBlock {
                    instrs: Vec::new(),
                    term: Term::Jmp(0),
                    line: 0,
                },
            );
            f.blocks[0] = moved;
            // Anything that jumped to t (none besides the old entry) now targets 0.
            stats.jumps_threaded += 1;
        }
    }
    for b in &mut f.blocks {
        let edges: Vec<&mut u32> = match &mut b.term {
            Term::Jmp(t) => vec![t],
            Term::Br { t, f: fb, .. } => vec![t, fb],
            _ => vec![],
        };
        for x in edges {
            let r = resolve(*x);
            if r != *x {
                *x = r;
                stats.jumps_threaded += 1;
            }
        }
    }
}

fn remove_unreachable(f: &mut Func, stats: &mut Stats) {
    let mut reach = vec![false; f.blocks.len()];
    let mut stack = vec![0usize];
    while let Some(b) = stack.pop() {
        if reach[b] {
            continue;
        }
        reach[b] = true;
        match &f.blocks[b].term {
            Term::Jmp(t) => stack.push(*t as usize),
            Term::Br { t, f: fb, .. } => {
                stack.push(*t as usize);
                stack.push(*fb as usize);
            }
            _ => {}
        }
    }
    if reach.iter().all(|r| *r) {
        return;
    }
    let mut remap = vec![0u32; f.blocks.len()];
    let mut next = 0u32;
    for (i, r) in reach.iter().enumerate() {
        if *r {
            remap[i] = next;
            next += 1;
        }
    }
    let old = std::mem::take(&mut f.blocks);
    for (i, mut b) in old.into_iter().enumerate() {
        if !reach[i] {
            stats.blocks_removed += 1;
            continue;
        }
        match &mut b.term {
            Term::Jmp(t) => *t = remap[*t as usize],
            Term::Br { t, f: fb, .. } => {
                *t = remap[*t as usize];
                *fb = remap[*fb as usize];
            }
            _ => {}
        }
        f.blocks.push(b);
    }
}

/// A block that ends in `jmp B` where `B` has no other predecessor absorbs
/// `B`, which lets the per-block passes (forwarding) see across the seam.
fn merge_blocks(f: &mut Func, stats: &mut Stats) {
    let n = f.blocks.len();
    let mut preds = vec![0usize; n];
    for b in &f.blocks {
        match &b.term {
            Term::Jmp(t) => preds[*t as usize] += 1,
            Term::Br { t, f: fb, .. } => {
                preds[*t as usize] += 1;
                preds[*fb as usize] += 1;
            }
            _ => {}
        }
    }
    for a in 0..n {
        while let Term::Jmp(t) = f.blocks[a].term {
            let t = t as usize;
            if t == a || t == 0 || preds[t] != 1 {
                break;
            }
            let mut absorbed = std::mem::replace(
                &mut f.blocks[t],
                crate::ir::BasicBlock {
                    instrs: Vec::new(),
                    term: Term::Jmp(t as u32),
                    line: 0,
                },
            );
            f.blocks[a].instrs.append(&mut absorbed.instrs);
            f.blocks[a].term = absorbed.term;
            preds[t] = 0; // now unreachable (self-loop stub), removed later
            stats.jumps_threaded += 1;
        }
    }
}
