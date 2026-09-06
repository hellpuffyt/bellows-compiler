# Roadmap

Each item is one reviewable PR; the golden/opt/fuzz suites make each safe.

## 0.2 — make native code fast

- [ ] Linear-scan register allocation over temps (values live in registers
  between defs and last uses; spill only under pressure). Expected: 3–10×
  on `bench.ash`.
- [ ] Peephole pass on the emitted assembly (`mov`/`mov` pairs, redundant
  loads after stores).
- [ ] Tail-call optimization for self-recursive `return f(...)`.

## 0.3 — grow the language

- [ ] Fixed-size arrays of `i64` with bounds-checked indexing (a trap, like
  overflow).
- [ ] `str` as a real type with `len` and comparison; `print` formatting.
- [ ] `for i in a..b` loops (desugared to `while`).
- [ ] Function-local `const` and global constants.
- [ ] `read()` builtin for stdin integers so programs can take input.

## 0.4 — more targets and tooling

- [ ] macOS (Mach-O directives and `_` symbol prefix) so native tests run on
  all three CI hosts.
- [ ] AArch64 backend (Linux).
- [ ] `bellows fmt` — a formatter built on the parser.
- [ ] LSP server for diagnostics-as-you-type using the same `Diagnostic`.

## Someday

- SSA with phi nodes and a proper dominator-based GVN.
- A `#[test]`-like block in Ash and `bellows test`.

## Non-goals

- Becoming a general-purpose language. Ash stays small enough that the
  whole compiler is readable in a sitting.
- Depending on LLVM or Cranelift; the point is owning the backend.
