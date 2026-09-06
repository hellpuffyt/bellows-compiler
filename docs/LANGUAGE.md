# The Ash language

Ash is a small, statically typed, expression-oriented-enough language
designed to make every compiler stage interesting without making any of
them large.

## Program structure

A program is a sequence of function declarations. Execution starts at
`fn main()`, which takes no parameters and returns nothing.

```
fn name(param: type, ...) -> type { statements }
fn name(param: type, ...) { statements }          // returns ()
```

Functions may be declared in any order and may be recursive. Each name may
be declared once; `print` is reserved.

## Types

| Type | Values | Notes |
|---|---|---|
| `i64` | 64-bit signed integers | arithmetic **traps** on overflow and division by zero |
| `bool` | `true`, `false` | |
| `()` | the result of a function without `->` | cannot be bound or printed |
| string literal | `"text"` with `\n \t \\ \"` escapes | only as the argument of `print` |

There are no implicit conversions.

## Statements

```
let x = expr;             // immutable binding; type inferred from expr
let mut y: i64 = 0;       // mutable, with an optional annotation
y = y + 1;                // assignment (E012 if not `mut`)
if cond { ... } else if cond { ... } else { ... }
while cond { ... }
return expr;              // value must match the declared return type
return;                   // only in functions without `->`
print(expr);              // i64, bool, or a string literal; newline appended
f(args);                  // a call as a statement (any other bare expression is E020)
```

Blocks introduce scopes; a name cannot be declared twice in the same scope
but may shadow an outer one. Statements after a `return` in the same block
are an error (E019). A function returning a value must return on every
path (E015): both branches of an `if` must return, or a `return` must
follow.

## Expressions

Precedence from lowest to highest:

| Level | Operators | Operand types → result |
|---|---|---|
| 1 | `\|\|` | bool, bool → bool (short-circuit) |
| 2 | `&&` | bool, bool → bool (short-circuit) |
| 3 | `==` `!=` | same type (i64 or bool) → bool |
| 4 | `<` `<=` `>` `>=` | i64, i64 → bool |
| 5 | `+` `-` | i64, i64 → i64 |
| 6 | `*` `/` `%` | i64, i64 → i64 (truncating division, remainder keeps the sign of the left operand) |
| 7 | unary `-`, `!` | i64 → i64, bool → bool |
| 8 | literals, names, calls, `( expr )` | |

Integer literals may use `_` separators and must fit in `i64`.

## Runtime errors

These terminate the program with exit code 101 and a message on stdout
(native) or stderr (interpreter):

- `integer overflow` — from `+ - *`, unary `-`, or `MIN / -1`, `MIN % -1`
- `division by zero` — from `/` or `%`

Deep recursion exhausts the machine stack in native code (a crash); the
interpreter reports `stack overflow (call depth limit)`.

## Diagnostics

| Code | Meaning |
|---|---|
| E001 | lexical error (bad character, literal, escape, comment) |
| E002 | syntax error |
| E010 | unknown variable |
| E011 | type mismatch |
| E012 | assignment to an immutable variable |
| E013 | wrong number of arguments |
| E014 | unknown function |
| E015 | not every path returns a value |
| E016 | duplicate declaration |
| E017 | `main` missing or has the wrong signature |
| E018 | condition is not `bool` |
| E019 | unreachable statement |
| E020 | expression statement with no effect |

## Grammar

```
program  := fn*
fn       := "fn" ident "(" (param ("," param)*)? ")" ("->" type)? block
param    := ident ":" type
type     := "i64" | "bool"
block    := "{" stmt* "}"
stmt     := "let" "mut"? ident (":" type)? "=" expr ";"
          | ident "=" expr ";"
          | "return" expr? ";"
          | "if" expr block ("else" (block | "if" ...))?
          | "while" expr block
          | "print" "(" expr ")" ";"
          | expr ";"
expr     := or
or       := and ("||" and)*
and      := eq ("&&" eq)*
eq       := cmp (("==" | "!=") cmp)*
cmp      := add (("<" | "<=" | ">" | ">=") add)*
add      := mul (("+" | "-") mul)*
mul      := unary (("*" | "/" | "%") unary)*
unary    := ("-" | "!") unary | primary
primary  := int | "true" | "false" | string | ident ("(" (expr ("," expr)*)? ")")? | "(" expr ")"
```
