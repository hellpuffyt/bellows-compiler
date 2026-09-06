# Contributing

## Ground rules

- Zero dependencies, zero `unsafe`. CI fails on either.
- **Both backends must agree.** Any change to semantics touches `ir::eval`
  (used by the interpreter *and* the constant folder) and `codegen.rs`, and
  gets a golden example that runs through both.
- Every new diagnostic gets a case in `tests/errors.rs` with its exact
  `line:col`.
- Every optimizer change gets an IR-shape test in `tests/opt.rs`; the
  equivalence and fuzz tests must stay green.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

## Workflow

```
git clone https://github.com/hellpuffyt/bellows-compiler
cd bellows-compiler
cargo test
cargo run -- run examples/fib.ash
cargo run -- build examples/fib.ash --emit ir
tools/bench.sh          # before/after for codegen or optimizer work
```

## Where things live

| Want to… | Look in |
|---|---|
| Add syntax | `lexer.rs` (tokens), `parser.rs` (AST + grammar), `docs/LANGUAGE.md` |
| Add a type rule or diagnostic | `sema.rs`, then `tests/errors.rs` |
| Add an IR instruction | `ir.rs` (enum, lowering, `dump`, interpreter), `opt.rs` (operand helpers), `codegen.rs` |
| Add an optimizer pass | `opt.rs` — add it to the fixpoint loop and a `Stats` counter |
| Change calling conventions or ABI details | `codegen.rs` (`libc_call`, `arg_regs`, `func`) |

## Reporting bugs

The smallest `.ash` program that shows the problem, plus `--emit ir` and
`--emit asm` output if it is a miscompilation. Interpreter/native
disagreements are the most valuable reports — the golden test harness makes
them easy to turn into a regression test.
