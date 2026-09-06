//! Optimizer tests: each pass is checked on IR shape *and* the optimized
//! program must still produce the same output as the unoptimized one.

use bellows::ir::{Instr, Operand, Term};
use bellows::{Options, compile};

fn ir(src: &str, optimize: bool) -> bellows::ir::Program {
    compile(
        src,
        "t.ash",
        Options {
            optimize,
            ..Default::default()
        },
    )
    .unwrap()
    .ir
}

fn run(p: &bellows::ir::Program) -> (String, u64) {
    let mut out = Vec::new();
    let mut i = bellows::ir::Interp::new(p, &mut out);
    i.run().unwrap();
    let steps = i.steps;
    (String::from_utf8(out).unwrap(), steps)
}

fn count_instrs(p: &bellows::ir::Program) -> usize {
    p.funcs
        .iter()
        .flat_map(|f| &f.blocks)
        .map(|b| b.instrs.len())
        .sum()
}

#[test]
fn constant_folding_removes_arithmetic_on_literals() {
    let src = "fn main() { let x = 2 + 3 * 4; let y = x - 14 + 0; print(y * 1); print(!(1 < 2) || false); }";
    let p = ir(src, true);
    let main = &p.funcs[p.main];
    let all: Vec<&Instr> = main.blocks.iter().flat_map(|b| &b.instrs).collect();
    assert!(
        all.iter()
            .all(|i| !matches!(i, Instr::Bin { .. } | Instr::Not { .. })),
        "arithmetic should be folded: {}",
        bellows::ir::dump(&p)
    );
    assert!(
        all.iter()
            .any(|i| matches!(i, Instr::PrintInt(Operand::Const(0)))),
        "{}",
        bellows::ir::dump(&p)
    );
    assert!(
        all.iter()
            .any(|i| matches!(i, Instr::PrintBool(Operand::Const(0)))),
        "short-circuit through a merged block folds to a constant: {}",
        bellows::ir::dump(&p)
    );
    assert_eq!(
        main.blocks.len(),
        1,
        "blocks merged: {}",
        bellows::ir::dump(&p)
    );
    assert_eq!(run(&p).0, run(&ir(src, false)).0);
}

#[test]
fn store_load_forwarding_and_dead_stores() {
    let src = "fn main() { let a = 5; let b = a; let c = b + a; print(c); }";
    let p = ir(src, true);
    let main = &p.funcs[p.main];
    let all: Vec<&Instr> = main.blocks.iter().flat_map(|b| &b.instrs).collect();
    assert!(
        all.iter().all(|i| !matches!(i, Instr::Load { .. })),
        "loads forwarded: {}",
        bellows::ir::dump(&p)
    );
    assert!(
        all.iter().all(|i| !matches!(i, Instr::Store { .. })),
        "stores are dead: {}",
        bellows::ir::dump(&p)
    );
    assert_eq!(
        all.len(),
        1,
        "only the print remains: {}",
        bellows::ir::dump(&p)
    );
    assert_eq!(run(&p).0, "10\n");
}

#[test]
fn dead_branches_and_blocks_are_removed_but_traps_are_kept() {
    let src = "fn main() { if 1 < 2 { print(1); } else { print(2); } while false { print(3); } }";
    let p = ir(src, true);
    let main = &p.funcs[p.main];
    assert!(
        main.blocks.iter().all(|b| !matches!(
            b.term,
            Term::Br {
                cond: Operand::Const(_),
                ..
            }
        )),
        "{}",
        bellows::ir::dump(&p)
    );
    let printed: Vec<i64> = main
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter_map(|i| {
            if let Instr::PrintInt(Operand::Const(c)) = i {
                Some(*c)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(printed, vec![1]);
    assert!(
        main.blocks.len() <= 2,
        "unreachable blocks removed: {}",
        bellows::ir::dump(&p)
    );
    // A division that could trap is not deleted even if its result is unused.
    let src = "fn main() { let z = 0; let x = 10 / z; print(1); }";
    let p = ir(src, true);
    let all: Vec<&Instr> = p.funcs[p.main]
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .collect();
    assert!(
        all.iter().any(|i| matches!(
            i,
            Instr::Bin {
                op: bellows::ir::Op::Div,
                ..
            }
        )),
        "{}",
        bellows::ir::dump(&p)
    );
    let mut out = Vec::new();
    assert!(bellows::ir::Interp::new(&p, &mut out).run().is_err());
}

#[test]
fn optimization_preserves_output_and_reduces_work() {
    let srcs = [
        include_str!("../examples/fib.ash"),
        include_str!("../examples/primes.ash"),
        include_str!("../examples/collatz.ash"),
        include_str!("../examples/fizzbuzz.ash"),
        include_str!("../examples/logic.ash"),
    ];
    for src in srcs {
        let (out0, steps0) = run(&ir(src, false));
        let (out1, steps1) = run(&ir(src, true));
        assert_eq!(out0, out1);
        assert!(
            steps1 < steps0,
            "optimized program should execute fewer IR instructions ({steps1} vs {steps0})"
        );
        assert!(count_instrs(&ir(src, true)) < count_instrs(&ir(src, false)));
    }
}

#[test]
fn stats_report_what_happened() {
    let u = compile(
        "fn main() { let x = 1 + 2; let y = x; print(y); if true { print(0); } }",
        "t.ash",
        Options::default(),
    )
    .unwrap();
    assert!(u.stats.folded >= 1);
    assert!(u.stats.forwarded >= 1);
    assert!(u.stats.branches_folded >= 1);
    assert!(u.stats.blocks_removed >= 1);
}
