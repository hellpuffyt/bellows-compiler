# Testing

`cargo test` — 17 tests, about ten seconds on a laptop (the native golden
tests invoke gcc).

| Suite | File | What it proves |
|---|---|---|
| Golden | `tests/golden.rs` | Every `examples/*.ash` declares `// expect:` lines; each program runs interpreted at -O0 and -O1 and, when a C compiler is present (Linux, Windows), compiled to native at both levels — all outputs must equal the expectations. Runtime traps (`/0`, `%0`, `*` overflow, `-MIN`, `MIN / -1`) produce the same message in the interpreter and exit 101 with the same message natively. |
| Diagnostics | `tests/errors.rs` | 36 programs, one per error situation, asserting the diagnostic **code**, the exact **line:column** of the caret, and a substring of the message; rendering (caret, label, `-->`), colour, and the helpful notes. |
| Optimizer | `tests/opt.rs` | Constant folding leaves no arithmetic; store→load forwarding and dead stores reduce `let a=5; let b=a; print(b+a)` to a single `print`; dead branches and unreachable blocks disappear while trapping divisions survive; block merging yields one block; for every example, -O1 executes fewer IR instructions and prints the same output; `stats` counts are populated. |
| Fuzz | `tests/fuzz.rs` | 30,000 random token sequences and 5,000 random byte strings never panic; 1,600 mutations of valid programs are compiled and, when accepted, run at -O0 and -O1 with a step limit — outputs and trap behaviour must agree. Deterministic seeds. |
| Doc test | `src/lib.rs` | The README's embedding example. |

## Real execution in CI

The workflow builds the compiler on Linux, Windows and macOS, runs the
suite, then on Linux and Windows compiles and runs `fib.ash`,
`fizzbuzz.ash` and `logic.ash` through `bellows run` with `-O0`, `-O1` and
`-g`, diffing stdout against the expectations; it checks that a trapping
program exits 101; on Linux it also runs `objdump --dwarf=decodedline` on a
`-g` build to prove the line table exists.

## Benchmarks

`tools/bench.sh` builds `examples/bench.ash` natively at -O0/-O1 and runs
the interpreter, printing wall-clock times; `bellows stats` prints what the
optimizer did. Numbers are informational and not asserted.

## Adding a test

- A new language feature: an example under `examples/` with `// expect:`
  lines (covers all four execution modes at once) plus negative cases in
  `tests/errors.rs`.
- A new optimizer pass: an IR-shape assertion in `tests/opt.rs`; the
  equivalence tests cover behaviour automatically.
- A new codegen path: make sure some example exercises it; the golden test
  will compare it against the interpreter.
