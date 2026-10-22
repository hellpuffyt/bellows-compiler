# Changelog

Format: [Keep a Changelog](https://keepachangelog.com). Versions: SemVer.
Language changes and IR changes are called out explicitly.

## [Unreleased]

### Fixed
- CI lint job: `tests/fuzz.rs` tripped clippy's `manual_is_multiple_of`
  (Rust 1.98), failing the job on the default branch since 2026-09-06.
- CI "No unsafe" step could never fail (`! grep` is ignored by `bash -e`);
  `#![forbid(unsafe_code)]` now enforces it and the step checks for it.
- The IR interpreter recursed on the host stack, so Ash recursion well
  inside the documented 100,000-call depth limit aborted the process with a
  stack overflow; calls now use an explicit frame stack and trap cleanly.
- Deeply nested input (10,000 nested parentheses, blocks, unary operators,
  calls, `else if` arms, or a 100,000-term `1 + 1 + …` chain) crashed the
  compiler; the parser now limits nesting (64 levels) and expression height
  (256) with an E002 diagnostic, and the CLI compiles on a 64 MiB stack.
- `bellows run` on a host or `--target` it cannot execute (e.g. macOS) fed
  foreign assembly to the C compiler; it now exits 2 with a clear message.
- Diagnostics were coloured even when stderr was not a terminal.
- Native golden tests now report as *ignored* on hosts without a native
  backend instead of passing silently, and fail if the C compiler is missing.

### Changed
- Docs: the diagnostic-code count is 13 (E001–E002, E010–E020), not 20.

## [0.1.0] — 2026-09-06

First release. Ash language v0.1.

### Added
- Ash: `i64`, `bool`, functions with typed parameters and return types,
  `let`/`let mut`, assignment, `if`/`else if`/`else`, `while`, `return`,
  arithmetic/comparison/logical operators with short-circuit `&&`/`||`,
  `print` of integers, booleans and string literals, line and nested block
  comments.
- Lexer, parser, semantic analysis with 13 diagnostic codes and rustc-style
  rendering, three-address IR with an interpreter, optimizer (folding,
  propagation, identities, store forwarding, dead code, branch folding,
  jump threading, block merging, unreachable removal), x86-64 code
  generation for Linux and Windows with runtime traps and DWARF line info.
- CLI: `build`, `run`, `interp`, `check`, `stats`, `--emit tokens|ast|ir|asm`,
  `-O0`, `-g`, `--target`, `--cc`.
- Test suites: golden (interpreted + native, both optimization levels),
  diagnostics with exact positions, optimizer shape and equivalence,
  deterministic fuzzing.
