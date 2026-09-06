# Bellows

**An optimizing compiler you can read end to end — for Ash, a small typed
language — from source text to x86-64 machine code, with an IR interpreter
that must agree with the native binary on every program.**

```
$ cat examples/fib.ash
fn fib(n: i64) -> i64 {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}
fn main() { print(fib(20)); }

$ bellows run examples/fib.ash          # compile → gcc assembles/links → execute
6765
$ bellows interp examples/fib.ash       # same program through the IR interpreter
6765
$ bellows build examples/fib.ash --emit ir | head -8
fn fib(params=1, slots=1, temps=6) {
  b0:
    %t0 = load s0
    %t1 = lt %t0, 2
    br %t1, b1, b2
  b1:
    ret %t0
  ...
```

Zero dependencies, ~2,600 lines of Rust. Runs on Linux and Windows
(MinGW); the interpreter runs anywhere.

## What is it?

| Stage | File | What it does |
|---|---|---|
| Lexer | `lexer.rs` | Tokens with byte spans; nested block comments, string escapes, `1_000` literals; every malformed input is a spanned error |
| Parser | `parser.rs` | Recursive descent with precedence climbing → AST; `else if` chains; every expression gets an id for the type table |
| Semantic analysis | `sema.rs` | Scopes, types (`i64`, `bool`, `()`), mutability, arity, `main` contract, **missing-return analysis**, unreachable code, useless expressions; 20 diagnostic codes |
| IR | `ir.rs` | Three-address code over basic blocks: single-assignment temps, numbered local slots, `br`/`jmp`/`ret`, short-circuit lowering; a printer and an **interpreter** |
| Optimizer | `opt.rs` | Constant folding + propagation, algebraic identities, store→load forwarding, dead temps, dead stores, branch folding, jump threading, block merging, unreachable-block removal — to a fixpoint |
| Code generator | `codegen.rs` | x86-64 AT&T assembly for System V (Linux) and MS x64 (Windows); **checked arithmetic that traps** like the interpreter; DWARF `.loc` line info with `-g` |
| Diagnostics | `diag.rs` | rustc-style rendering: code, message, `file:line:col`, source line, caret, label, notes; colour when a terminal |

```
$ bellows check examples/bad.ash
error[E011]: type mismatch: `x` is declared `bool` but initialised with `i64`
  --> examples/bad.ash:2:19
   |
 2 |     let x: bool = 3;
   |                   ^ this has type `i64`
```

## Who is it for?

- People **learning how compilers work** who want every stage in one small
  codebase with tests that show what each stage guarantees.
- People **teaching** compilers: `--emit tokens|ast|ir|asm` prints each
  intermediate form; `stats` prints what the optimizer did.
- Anyone who wants a **worked example of "the optimizer must not change
  behaviour"** — every example runs at -O0 and -O1, interpreted and native,
  and all four outputs must match.

## Why does it exist?

Real compilers are too large to read, and tutorial compilers stop at the
first thing that prints `42` — no type checker with real diagnostics, no
optimizer, no proof that optimization preserved semantics, no runtime
traps. Bellows is the middle ground: a complete pipeline where each pass is
a few hundred lines, and where correctness is *tested* rather than assumed.

## What makes it different?

- **Two backends that must agree.** The IR interpreter and the native code
  share one arithmetic definition (`ir::eval`), and the golden tests run
  every example through both at both optimization levels.
- **Traps are part of the language.** Integer overflow and division by zero
  are runtime errors (exit code 101) — in native code via `jo`/`jz` checks,
  in the interpreter via checked arithmetic. The optimizer never folds a
  trapping operation away.
- **Diagnostics with a caret**, a label, and notes that suggest the fix
  (`declare it with let mut x`, `call it with f(...)`).
- **The optimizer is observable**: `bellows stats` reports folds, forwards,
  dead code and threaded jumps; `tests/opt.rs` checks IR shape *and*
  behavioural equivalence *and* that fewer instructions execute.
- **DWARF line info** in one flag (`-g`): step through Ash source in gdb.

## Why is this not just a tutorial?

Because the failure modes are covered: 36 negative tests pin the exact
`line:col` of every diagnostic; a deterministic fuzzer feeds 30,000 token
soups and 5,000 random byte strings through the pipeline and mutates valid
programs to check -O0/-O1 agreement; the golden suite compiles and runs
every example natively on Linux and Windows in CI; overflow, `MIN / -1`,
`MIN % -1` and division by zero each have a test in both backends.

## Benchmarks

`examples/bench.ash` (recursive `fib(30)` + a 10-million-iteration
arithmetic loop), Windows 11 laptop, one run each:

| Configuration | Time |
|---|---|
| Native, -O1 | 500 ms |
| Native, -O0 | 500 ms |
| Interpreter, -O1 | 534 ms |
| Interpreter, -O0 | 595 ms |
| Compile `collatz.ash` (all stages, no linking) | 14 ms |

Honest reading: the code generator keeps every value in the stack frame
("spill everything"), so native code is only marginally faster than the
interpreter today. Register allocation is the first roadmap item, and the
tests are in place to make it safe.

## Build, test, run

```
cargo build --release                  # target/release/bellows
cargo test                             # golden, errors, optimizer, fuzz (17 tests)
bellows run    examples/primes.ash     # needs gcc (or CC=clang on Linux)
bellows interp examples/primes.ash     # no toolchain needed
bellows build  examples/fib.ash -o fib [-O0] [-g] [--target linux|windows]
bellows build  examples/fib.ash --emit asm
bellows stats  examples/collatz.ash
```

## The Ash language in one screen

```
fn gcd(a: i64, b: i64) -> i64 {       // i64 and bool; functions must return on every path
    let mut x = a;                     // immutable by default; `mut` to assign
    let mut y = b;
    while y != 0 { let t = y; y = x % y; x = t; }
    return x;
}
fn main() {
    let big = 9223372036854775807;
    print(gcd(48, 18));                // 6
    print(big > 0 && !(big < 0));      // true — && and || short-circuit
    print("done");                     // string literals only in print
    if false { print(big + 1); }       // would trap: integer overflow
}
```

Full reference: [`docs/LANGUAGE.md`](docs/LANGUAGE.md).

## Documentation

- [`docs/LANGUAGE.md`](docs/LANGUAGE.md) — syntax, types, semantics, traps, diagnostics
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — each stage, the IR, the optimizer's invariants, the calling convention
- [`SECURITY.md`](SECURITY.md), [`TESTING.md`](TESTING.md), [`ROADMAP.md`](ROADMAP.md), [`CONTRIBUTING.md`](CONTRIBUTING.md), [`CHANGELOG.md`](CHANGELOG.md)

## Why star or contribute?

Star it if you want a compiler small enough to understand and honest enough
to test itself. Contribute a register allocator, a new type, arrays, an
ARM64 backend, or an LSP — each is a scoped project against a pipeline that
tells you immediately when you broke something.

## License

MIT. See [`LICENSE`](LICENSE).
