//! Hostile nesting must produce a diagnostic, never a compiler crash.
//!
//! Regression: before the parser enforced nesting limits, 10,000 nested
//! parentheses (or blocks, unary minuses, calls, `else if` arms, or a
//! 100,000-term `1 + 1 + …` chain) overflowed the compiler's own stack and
//! aborted the process. These tests run on the default 2 MiB test thread,
//! in whatever profile `cargo test` uses, and exercise every recursive pass
//! (sema, lowering, optimizer, codegen, AST dump, interpreter) at the limit.

use bellows::parser::{MAX_EXPR_DEPTH, MAX_NESTING};
use bellows::{Options, compile};

fn rejected(src: &str) -> String {
    let Err(d) = compile(src, "t.ash", Options::default()) else {
        panic!("must be rejected: {}…", &src[..src.len().min(60)]);
    };
    assert_eq!(d.code, "E002", "{}", d.message);
    d.message
}

fn runs(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || runs_here(&src))
        .unwrap()
        .join()
        .expect("compiler must not overflow an 8 MiB stack at the limits")
}

fn runs_here(src: &str) -> String {
    for optimize in [false, true] {
        let unit = compile(
            src,
            "t.ash",
            Options {
                optimize,
                debug: true,
                ..Default::default()
            },
        )
        .unwrap_or_else(|d| panic!("{}", d.message));
        let _ = bellows::parser::dump(&unit.ast);
        let _ = bellows::ir::dump(&unit.ir);
        assert!(!unit.asm.is_empty());
        let mut out = Vec::new();
        bellows::ir::Interp::new(&unit.ir, &mut out).run().unwrap();
        if optimize {
            return String::from_utf8(out).unwrap();
        }
    }
    unreachable!()
}

fn parens(n: usize) -> String {
    format!(
        "fn main() {{ print({}1{}); }}",
        "(".repeat(n),
        ")".repeat(n)
    )
}

fn blocks(n: usize) -> String {
    format!(
        "fn main() {{ {}print(7);{} }}",
        "if true { ".repeat(n),
        " }".repeat(n)
    )
}

fn chain(n: usize) -> String {
    format!("fn main() {{ print(0{}); }}", " + 1".repeat(n))
}

#[test]
fn nesting_at_the_limit_compiles_and_runs() {
    // Function body + n parentheses.
    assert_eq!(runs(&parens(MAX_NESTING - 1)), "1\n");
    // Function body + n nested blocks.
    assert_eq!(runs(&blocks(MAX_NESTING - 1)), "7\n");
    // A left-deep chain whose tree height is exactly the limit.
    let n = MAX_EXPR_DEPTH as usize - 1;
    assert_eq!(runs(&chain(n)), format!("{n}\n"));
    // Worst case: the deepest block nesting holding the tallest expression.
    let combo = format!(
        "fn main() {{ {}print(0{});{} }}",
        "if true { ".repeat(MAX_NESTING - 1),
        " + 1".repeat(n),
        " }".repeat(MAX_NESTING - 1)
    );
    assert_eq!(runs(&combo), format!("{n}\n"));
}

#[test]
fn deep_parentheses_are_a_diagnostic() {
    let m = rejected(&parens(MAX_NESTING));
    assert!(m.contains("nested too deeply"), "{m}");
    rejected(&parens(100_000));
}

#[test]
fn deep_blocks_unary_calls_and_else_if_chains_are_diagnostics() {
    rejected(&blocks(MAX_NESTING));
    rejected(&blocks(100_000));
    rejected(&format!("fn main() {{ print({}1); }}", "-".repeat(100_000)));
    rejected(&format!(
        "fn f(x: i64) -> i64 {{ return x; }} fn main() {{ print({}1{}); }}",
        "f(".repeat(100_000),
        ")".repeat(100_000)
    ));
    let arms: String = (1..100_000)
        .map(|i| format!("else if x == {i} {{ print({i}); }} "))
        .collect();
    rejected(&format!(
        "fn main() {{ let x = 1; if x == 0 {{ print(0); }} {arms}}}"
    ));
}

#[test]
fn long_operator_chains_are_a_diagnostic() {
    let m = rejected(&chain(MAX_EXPR_DEPTH as usize));
    assert!(m.contains("expression is nested too deeply"), "{m}");
    rejected(&chain(100_000));
}
