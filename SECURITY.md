# Security

Bellows is a compiler: its inputs are source files it does not trust, and
its outputs are programs that run with the user's privileges. The relevant
properties are (1) the compiler itself cannot be crashed or made to misbehave
by hostile input, and (2) compiled programs have the semantics the language
promises, including on arithmetic edge cases.

## Guarantees (tested)

| Property | Mechanism | Evidence |
|---|---|---|
| No panic on any input | Every stage returns spanned diagnostics; the lexer bounds numbers, strings and comments; the parser never indexes past `Eof` | `tests/fuzz.rs`: 30k token soups, 5k random byte strings |
| Integer overflow is a defined runtime error, not UB | Interpreter uses checked ops; native code uses `jo` after every add/sub/imul/neg and explicit `MIN / -1` checks; exit code 101 | `tests/golden.rs::runtime_traps_agree…` on both backends |
| Division by zero is a defined runtime error | Explicit zero test before `idiv`; checked in the interpreter | same |
| The optimizer never changes behaviour | Folding uses the interpreter's `eval`; trapping instructions are never deleted; every example and every fuzzed mutation is run at -O0 and -O1 and compared | `tests/opt.rs`, `tests/fuzz.rs`, `tests/golden.rs` |
| No arbitrary code from data | The language has no pointers, arrays, or indirect calls; the only I/O is `print` to stdout through libc | by construction |
| Memory safety of the compiler | 100% safe Rust, no dependencies | CI grep |

## Non-guarantees

- **Stack overflow in compiled programs.** Deep recursion (e.g. `fib`
  without a base case) overflows the machine stack and crashes with a
  signal; there is no guard page probe or depth limit in native code. The
  interpreter has a configurable depth limit.
- **`bellows run` executes the program you compiled.** It is your program;
  Bellows does not sandbox it.
- **The C compiler is trusted.** `bellows build`/`run` invoke `gcc` (or
  `$CC`) to assemble and link; the generated `.s` file is written next to
  the output. Point `--cc` only at a toolchain you trust.
- **Resource use.** Pathological programs (very long expressions, huge
  functions) use memory proportional to their size; there is no limit.
- **Timing.** Nothing is constant-time; Ash is not a language for secrets.

## Reporting

Open a security advisory on the GitHub repository with the `.ash` file that
triggers the issue. Acknowledgement within 7 days.
