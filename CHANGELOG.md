# Changelog

Format: [Keep a Changelog](https://keepachangelog.com). Versions: SemVer.
Language changes and IR changes are called out explicitly.

## [Unreleased]

## [0.1.0] — 2026-09-06

First release. Ash language v0.1.

### Added
- Ash: `i64`, `bool`, functions with typed parameters and return types,
  `let`/`let mut`, assignment, `if`/`else if`/`else`, `while`, `return`,
  arithmetic/comparison/logical operators with short-circuit `&&`/`||`,
  `print` of integers, booleans and string literals, line and nested block
  comments.
- Lexer, parser, semantic analysis with 20 diagnostic codes and rustc-style
  rendering, three-address IR with an interpreter, optimizer (folding,
  propagation, identities, store forwarding, dead code, branch folding,
  jump threading, block merging, unreachable removal), x86-64 code
  generation for Linux and Windows with runtime traps and DWARF line info.
- CLI: `build`, `run`, `interp`, `check`, `stats`, `--emit tokens|ast|ir|asm`,
  `-O0`, `-g`, `--target`, `--cc`.
- Test suites: golden (interpreted + native, both optimization levels),
  diagnostics with exact positions, optimizer shape and equivalence,
  deterministic fuzzing.
