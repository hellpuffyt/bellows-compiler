//! Interpreter limits. These run on the default test thread (2 MiB stack),
//! which is exactly where a recursive interpreter used to overflow.

use bellows::ir::{Interp, Trap};
use bellows::{Options, compile};

fn run(src: &str, max_depth: Option<usize>) -> (Result<(), Trap>, String) {
    let unit = compile(src, "t.ash", Options::default()).unwrap();
    let mut out = Vec::new();
    let mut i = Interp::new(&unit.ir, &mut out);
    if let Some(d) = max_depth {
        i.max_depth = d;
    }
    let r = i.run();
    (r, String::from_utf8(out).unwrap())
}

const COUNT_DOWN: &str = "fn f(n: i64) -> i64 { if n == 0 { return 0; } return 1 + f(n - 1); } \
                          fn main() { print(f(90000)); }";

#[test]
fn default_depth_limit_is_reachable_without_host_stack_overflow() {
    let (r, out) = run(COUNT_DOWN, None);
    assert_eq!(r, Ok(()));
    assert_eq!(out, "90000\n");
}

#[test]
fn unbounded_recursion_traps_at_the_default_limit() {
    let (r, _) = run(
        "fn f(n: i64) -> i64 { return f(n + 1); } fn main() { print(f(0)); }",
        None,
    );
    assert_eq!(r, Err(Trap::StackOverflow));
}

#[test]
fn depth_limit_is_exact() {
    // main + f(3) + f(2) + f(1) + f(0) = 5 frames.
    let src = "fn f(n: i64) -> i64 { if n == 0 { return 0; } return 1 + f(n - 1); } \
               fn main() { print(f(3)); }";
    assert_eq!(run(src, Some(5)).0, Ok(()));
    assert_eq!(run(src, Some(4)).0, Err(Trap::StackOverflow));
}

#[test]
fn return_values_flow_back_through_nested_calls() {
    let src = "fn add(a: i64, b: i64) -> i64 { return a + b; } \
               fn twice(x: i64) -> i64 { return add(x, x); } \
               fn noop() { } \
               fn main() { noop(); print(add(twice(3), twice(add(1, 1)))); }";
    let (r, out) = run(src, None);
    assert_eq!(r, Ok(()));
    assert_eq!(out, "10\n");
}
