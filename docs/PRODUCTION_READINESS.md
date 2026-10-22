# Production readiness — bellows-compiler

Audit date: 2026-10-22. Tested commit: `50f8818` on branch
`audit/production-readiness` (this file was added in the following commit;
no code changed after the tested commit).

## Scope

Bellows is an educational optimizing compiler for Ash, a small statically
typed language: lexer → parser → semantic analysis → three-address IR →
optimizer → x86-64 AT&T assembly (System V Linux, MS x64 Windows/MinGW),
plus an IR interpreter that must agree with native code. Zero dependencies,
100% safe Rust (`#![forbid(unsafe_code)]`). Architecture: see
[`../ARCHITECTURE.md`](../ARCHITECTURE.md).

Maturity: experimental 0.1.0 — a teaching compiler, not a production
toolchain.

## Platforms and prerequisites

| Platform | Compiler, interpreter, `--emit` | `bellows build`/`run` (native) |
|---|---|---|
| x86-64 Linux | yes | yes, needs `gcc` (or `CC=clang`) |
| x86-64 Windows | yes | yes, needs MinGW `gcc` |
| macOS (any arch), AArch64 Linux | yes | no — no Mach-O/AArch64 backend; `run` exits 2 with a pointer to `interp`. `build --target linux|windows --cc <cross gcc>` can cross-compile. |

Rust ≥ 1.88 (`rust-version`), stable toolchain.

## Clean-clone verification (this audit, macOS 27 / Apple Silicon, rustc 1.98.0)

```
git clone -b audit/production-readiness <repo> cc && cd cc
cargo build --release                         # exit 0
cargo test                                    # exit 0: 31 passed, 0 failed, 2 ignored
target/release/bellows run examples/primes.ash        # exit 2 (macOS: refused, as documented)
target/release/bellows interp examples/primes.ash     # exit 0, prints 2 3 5 …
target/release/bellows build examples/fib.ash --emit asm   # exit 0, contains ash_fib:
target/release/bellows stats examples/collatz.ash     # exit 0
target/release/bellows check bad.ash          # exit 1, output identical to README
```

## Commands executed (exit codes)

| Command | Before (main `33c418a`) | After (`50f8818`) |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | 0 |
| `cargo clippy --all-targets -- -D warnings` (CI's exact command) | **101** — `clippy::manual_is_multiple_of` at `tests/fuzz.rs:149` | 0 |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | 101 | 0 |
| `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --locked` | — | 0 |
| `cargo build --release --all-targets` | 0 | 0 |
| `cargo test --release` | 0: 17 passed, 0 failed, 0 ignored (2 native tests *passed while skipping* on macOS) | 0: 31 passed, 0 failed, 2 ignored |
| `cargo test` (debug) | — | 0: 31 passed, 0 failed, 2 ignored |
| `cargo audit --db <advisory-db, current 2026-10-22> --no-fetch` | 0 (1 crate, no deps) | 0 |
| `gitleaks git` (full history) / `gitleaks dir` | — | no leaks found |
| Cross-assemble every example × {linux, windows} × {-O1, -O0, -g} with `clang --target=x86_64-unknown-linux-gnu` / `x86_64-w64-windows-gnu -c` | — | 36/36 assemble (not linked or executed here) |

Ignored tests (run only on x86-64 Linux/Windows with a C compiler, which CI
provides): `golden::examples_match_expectations_native`,
`golden::runtime_traps_agree_between_interpreter_and_native`.

## CI status

- Default branch (`main`, `33c418a`): **failure** since 2026-09-06 in job
  "fmt, clippy, unsafe, audit, secrets" at the clippy step; the three test
  jobs succeed (native execution verified on ubuntu and windows runners).
- Audit branch: pending until pushed. The audit/secret steps after clippy
  (cargo-audit install, gitleaks-action) have never executed in CI; their
  local equivalents pass.

## Bugs found and fixed

| # | Defect | Root cause | Fix | Regression test |
|---|---|---|---|---|
| 1 | CI lint job red on `main` | Rust/clippy 1.98 added `manual_is_multiple_of` (warn); CI uses `-D warnings` on stable | `rng.next().is_multiple_of(3)` (stable since 1.87 < MSRV 1.88) | the CI clippy command itself |
| 2 | CI "No unsafe" step could never fail | `! grep …` is exempt from `bash -e` | `#![forbid(unsafe_code)]` in `lib.rs`/`main.rs`; CI checks the attribute | compiler-enforced |
| 3 | `bellows interp` aborted (SIGABRT, exit 134) on Ash recursion far inside the documented 100,000 depth limit (e.g. depth 90,000), contradicting SECURITY.md | interpreter recursed on the Rust stack | explicit heap frame stack; depth limit now the only bound | `tests/interp.rs` (4), `tests/cli.rs::interp_*` (2); verified failing before the fix |
| 4 | Compiler aborted with stack overflow on 10,000 nested `(`, blocks, unary `-`, calls, `else if` arms, or a 100,000-term `+` chain ("no panic on any input" claim false) | unbounded recursion in parser/sema/lowering/dump/drop | parser limits nesting to 64 and expression height to 256 (E002 diagnostic); CLI compiles on a 64 MiB thread | `tests/limits.rs` (4, at-limit programs through every pass on an 8 MiB stack, debug and release), `tests/cli.rs::deeply_nested_input_*` |
| 5 | `bellows run` on macOS (or `--target` ≠ host) produced dozens of assembler errors | ELF/foreign asm handed to host `cc` | refuse with exit 2 and a pointer to `interp` / cross `build` | `tests/cli.rs::run_refuses_*` (2) |
| 6 | Diagnostics coloured when stderr is a pipe/file (README: "colour when a terminal") | only `NO_COLOR` checked | also require `stderr().is_terminal()` | `tests/cli.rs::diagnostics_are_plain_text_when_not_a_terminal` |
| 7 | Native golden tests reported "passed" on hosts where they cannot run, and silently skipped when no C compiler existed | `Option` early-return | `#[ignore = reason]` on unsupported hosts; missing C compiler is a failure | the tests themselves |
| 8 | Docs: "20 diagnostic codes" (13 exist), "~2,600 lines" (2,929 at baseline), README IR sample not real output, `examples/bad.ash` does not exist | stale docs | corrected; samples replaced with real output | — |

## Security review

- Inputs are untrusted source files: lexer/parser/sema fuzzed (30k token
  soups, 5k byte strings, 1.6k mutations) plus the new nesting limits.
- `bellows build/run` invokes `$CC`/`--cc`/`gcc` via `Command` with
  arguments (no shell); the user chooses the toolchain (documented trust).
- `bellows run` writes the executable to the system temp dir with a
  PID-based name and deletes it afterwards (predictable name; acceptable for
  a local developer tool, noted as a limitation).
- No network, no deserialization, no secrets. History: 2 commits, no secret
  hits (pattern scan + gitleaks). No rotation needed.
- Workflow permissions: `contents: read`. Actions are pinned to tags, not
  SHAs (left unchanged per audit rules; they are not broken).

## Known limitations / unresolved risks

- Native code cannot be linked or executed on this macOS host; native
  correctness evidence comes from the default-branch CI runs (Linux,
  Windows) and from cross-assembly here. The audit branch's native CI run is
  pending until pushed.
- Interpreter is ~15% slower on `examples/bench.ash` on this Mac after fix 3
  (0.57 s → ~0.68 s wall, 5 alternating runs, noisy shared machine). README
  benchmark table (Windows laptop) was not re-measured.
- Nesting limits (64 / 256) reject long `else if` chains (> ~63 arms) and
  extremely long single expressions; documented in `docs/LANGUAGE.md`.
- Native deep recursion still crashes with a signal (documented non-guarantee).
- `gitleaks-action@v2` and `cargo install cargo-audit` steps have never run
  in CI; if gitleaks-action requires a licence for this account type the step
  would fail (not verifiable locally).
- License is MIT only (kept as-is).

## Acceptance checklist

| Item | Expected | Command / test | Result | Actual | Reference |
|---|---|---|---|---|---|
| CI lint commands pass | exit 0 | `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings` | pass | 0 / 0 | `.github/workflows/ci.yml` |
| Builds from clean clone | exit 0 | `cargo build --release` | pass | 0 | — |
| Test suite | 0 failures | `cargo test` | pass | 31 passed, 2 ignored | `tests/` |
| Examples agree interpreted -O0/-O1 | outputs = `// expect:` | `golden::examples_match_expectations_interpreted` | pass | ok | `tests/golden.rs` |
| Examples agree natively | outputs = `// expect:` | `golden::examples_match_expectations_native` | not run here (ignored on macOS) | passed in main CI on ubuntu/windows | `tests/golden.rs` |
| Runtime traps | exit 101 + message | `golden::runtime_traps_in_the_interpreter`; `bellows interp` trap | pass | ok | `tests/golden.rs`, `tests/cli.rs` |
| Diagnostics with exact positions | 36 cases | `errors::*` | pass | ok | `tests/errors.rs` |
| Optimizer preserves behaviour | equal output, fewer steps | `opt::*`, `fuzz::mutated_programs_*` | pass | ok | `tests/opt.rs`, `tests/fuzz.rs` |
| No crash on hostile input | diagnostic, never abort | `fuzz::*`, `limits::*`, `cli::deeply_nested_*` | pass | ok | `src/parser.rs` |
| Deep interpreter recursion bounded | trap, never abort | `interp::*`, `cli::interp_*` | pass | ok | `src/ir.rs` |
| No unsafe / no deps | forbid attribute, 1 package in lock | CI step locally | pass | 0 | `src/lib.rs`, `Cargo.lock` |
| Dependency audit | no advisories | `cargo audit` | pass | 0 vulnerabilities | — |

## Classification proposal

**Partially Verified** — everything that can run on macOS passes from a clean
clone and the CI lint failure is fixed, but native code generation (the
compiler's core output) was not linked or executed on this host; that
evidence comes only from the pre-audit `main` CI runs and must be
re-confirmed by the audit branch's own CI (Linux + Windows) after push.
