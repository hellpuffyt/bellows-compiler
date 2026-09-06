# Architecture

```
 source ─► lexer ─► parser ─► sema ─► lower ─► opt ─► codegen ─► .s ─► gcc ─► exe
           Token     AST     Typed    IR      IR      asm text
                                       └────────► Interp (bellows interp, tests)
```

Every stage is a pure function of its input and returns `Result<_, Diagnostic>`;
`lib.rs::compile` strings them together into a `Unit` that keeps every
intermediate form so `--emit` can print any of them.

## Lexer

Byte-oriented scanner producing `Token { tok, span }`. Spans are byte
ranges; `diag::Source` turns them into line/column on demand and renders
the source line with a caret. Nested `/* */` comments, `_` digit
separators, escape sequences and overflow are all diagnosed with a span.

## Parser

Recursive descent for statements, precedence climbing for expressions
(`||` < `&&` < `== !=` < `< <= > >=` < `+ -` < `* / %` < unary). `else if`
is desugared to an `else` block containing an `if`. Each `Expr` carries a
dense `id` so sema can record its type in a `Vec<Ty>` without mutating the
AST.

## Semantic analysis

A scope stack of `name → (type, mutable, span)`. Checks: undefined names,
type mismatches (with the operand that has the wrong type labelled),
assignment to immutables, arity and argument types, redefinitions, `main`'s
signature, conditions being `bool`, string literals only under `print`,
expression statements that are not calls (`E020`), unreachable statements
after `return` (`E019`), and the **missing-return** rule: a function
returning a value must return on every path, computed structurally (a block
returns if any statement returns; an `if` returns only if both branches do;
a `while` never does).

## IR

```
fn fib(params=1, slots=1, temps=6) {
  b0:
    %t0 = load s0
    %t1 = lt %t0, 2
    br %t1, b1, b2
```

- **Temps** (`%tN`) are written exactly once, by exactly one instruction.
- **Slots** (`sN`) are parameters (first `params` of them) and locals; they
  are read and written with explicit `load`/`store`. Every `let` gets a
  fresh slot, so shadowing needs no renaming.
- Blocks end in `jmp`, `br cond`, or `ret`. `&&`/`||` lower to a slot
  written in two blocks and a `br`.
- Each block remembers the source line of its first statement for `.loc`.

The split between single-assignment temps and explicitly loaded slots is
what makes the optimizer simple: temps can be propagated globally by a
single map; slots only need per-block reasoning because locals cannot
alias and calls cannot touch them.

## Optimizer

Passes run per function to a fixpoint (stats stop changing):

| Pass | Invariant it relies on |
|---|---|
| Fold + propagate temps | temps single-assignment; `ir::eval` is the one definition of arithmetic |
| Algebraic identities | `x+0`, `x*1`, `x*0`, `x-0`, `x/1` never trap |
| Store→load forwarding | locals have no aliases; only a `store` to the same slot invalidates |
| Dead temps | pure instructions only; `div`/`rem` are kept because they can trap |
| Dead stores | a slot never loaded anywhere in the function is write-only |
| Branch folding | `br const` → `jmp` |
| Jump threading | empty blocks that only `jmp` are skipped; an empty entry block absorbs its unique successor |
| Block merging | `A: … jmp B` with B's only predecessor A → concatenate |
| Unreachable removal | DFS from the entry; blocks renumbered |

`tests/opt.rs` asserts IR shape after each pass and that optimized programs
print the same output while executing fewer IR instructions.

## Interpreter

A direct executor of the IR with checked arithmetic, a call-depth limit and
a step limit (both configurable). It is the reference semantics: the golden
tests compare native output against it, and `eval` is shared with the
constant folder so folding cannot disagree with execution.

## Code generation

AT&T syntax for GNU `as`, assembled and linked by the system C compiler.

- **Frame**: `push rbp; mov rsp,rbp; sub frame,rsp` with slots at
  `-8(i+1)(%rbp)` and temps after them. Every value lives in memory
  (ponytail: spill-everything; a linear-scan allocator over temps is the
  upgrade path and the tests make it safe).
- **Internal calls**: arguments pushed right-to-left, callee copies
  `16+8i(%rbp)` into its parameter slots, result in `%rax`, caller pops.
  Independent of the platform ABI.
- **libc calls** (`printf`, `puts`, `exit`): the stack is 16-byte aligned
  through `%rbx` (callee-saved on both ABIs); Linux passes `%rdi/%rsi` and
  zeroes `%al`, Windows passes `%rcx/%rdx` and reserves 32 bytes of shadow
  space. Symbols are `printf@PLT` on Linux, `printf` on MinGW.
- **Traps**: `jo` after `add/sub/imul/neg`; division checks for zero and for
  `MIN / -1` before `idiv`; trap handlers print a message and `exit(101)`.
- **Entry**: `main` (called by the C runtime) calls `ash_main` and returns 0.
- **Debug info**: with `-g`, `.file 1 "…"` and `.loc 1 LINE 0` per block,
  and `-g` is passed to the C compiler, so gdb/lldb step by Ash line.

## Diagnostics

`Diagnostic { code, message, span, label, notes }` rendered by
`Source::render` with the source line, a caret run covering the span, an
optional label after the carets, and `= note:` lines. Colour is ANSI when
enabled; tests render without.
