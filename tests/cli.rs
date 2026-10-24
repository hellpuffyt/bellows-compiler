//! End-to-end tests of the `bellows` binary that do not need a C compiler.

use std::path::PathBuf;
use std::process::{Command, Output};

fn bellows(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bellows"))
        .args(args)
        .output()
        .expect("bellows binary runs")
}

fn source(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bellows-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// Regression: the interpreter used to recurse on the Rust stack, so an Ash
/// program recursing within the documented depth limit (100,000) crashed the
/// whole process with a host stack overflow (SIGABRT) instead of trapping.
#[test]
fn interp_unbounded_recursion_is_a_runtime_error_not_a_crash() {
    let f = source(
        "rec.ash",
        "fn f(n: i64) -> i64 { return f(n + 1); } fn main() { print(f(0)); }",
    );
    let out = bellows(&["interp", f.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(101), "stderr: {stderr}");
    assert!(
        stderr.contains("stack overflow (call depth limit)"),
        "{stderr}"
    );
}

#[test]
fn interp_deep_recursion_within_the_limit_completes() {
    let f = source(
        "deep.ash",
        "fn f(n: i64) -> i64 { if n == 0 { return 0; } return 1 + f(n - 1); } \
         fn main() { print(f(90000)); }",
    );
    let out = bellows(&["interp", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "90000\n");
}

/// Regression: `run` for a target the host cannot execute used to hand
/// foreign assembly to the C compiler and fail with a wall of assembler
/// errors; it now refuses with a pointer to `interp`.
#[test]
fn run_refuses_a_target_the_host_cannot_execute() {
    let f = source("fib_run.ash", include_str!("../examples/fib.ash"));
    let other = if cfg!(windows) { "linux" } else { "windows" };
    let out = bellows(&["run", f.to_str().unwrap(), "--target", other]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("bellows interp"), "{stderr}");
}

#[test]
#[cfg(not(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
)))]
fn run_refuses_on_a_host_without_a_native_backend() {
    let f = source("fib_host.ash", include_str!("../examples/fib.ash"));
    let out = bellows(&["run", f.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("bellows interp"), "{stderr}");
}

#[test]
fn emit_asm_works_on_every_host() {
    let out = bellows(&["build", "examples/fib.ash", "--emit", "asm"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("ash_fib:"));
}

/// Regression: diagnostics were coloured even when stderr was a pipe or a
/// file, although the README promises colour only on a terminal.
#[test]
fn diagnostics_are_plain_text_when_not_a_terminal() {
    let f = source("bad.ash", "fn main() {\n    let x: bool = 3;\n}\n");
    let out = bellows(&["check", f.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.starts_with("error[E011]: type mismatch"), "{stderr}");
    assert!(
        !stderr.contains('\x1b'),
        "escape codes in piped output: {stderr:?}"
    );
}

/// Regression: hostile nesting aborted the compiler with a stack overflow.
#[test]
fn deeply_nested_input_is_a_diagnostic_not_a_crash() {
    let n = 100_000;
    let f = source(
        "nest.ash",
        &format!(
            "fn main() {{ print({}1{}); }}",
            "(".repeat(n),
            ")".repeat(n)
        ),
    );
    let out = bellows(&["check", f.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("nested too deeply"), "{stderr}");
}
