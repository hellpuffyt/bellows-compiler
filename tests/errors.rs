//! Negative tests: every diagnostic code, with the message and the exact
//! line:column the caret points at.

use bellows::{Options, compile, render};

fn err(src: &str) -> (String, usize, usize, String) {
    let d = compile(src, "t.ash", Options::default())
        .err()
        .expect("expected a diagnostic");
    let (line, col) = bellows::diag::Source::new("t.ash", src).position(d.span.start);
    (d.code.to_string(), line, col, d.message.clone())
}

#[test]
fn lexer_errors() {
    let cases = [
        (
            "fn main() { let x = 99999999999999999999; }",
            "E001",
            1,
            21,
            "too large",
        ),
        (
            "fn main() { print(\"open); }",
            "E001",
            1,
            19,
            "unterminated string",
        ),
        (
            "fn main() { let x = 1 $ 2; }",
            "E001",
            1,
            23,
            "unexpected character",
        ),
        (
            "fn main() { /* never closed }",
            "E001",
            1,
            13,
            "unterminated block comment",
        ),
        (
            "fn main() { print(\"bad \\q\"); }",
            "E001",
            1,
            24,
            "unknown escape",
        ),
        (
            "fn main() { let x = 12abc; }",
            "E001",
            1,
            21,
            "invalid suffix",
        ),
    ];
    for (src, code, line, col, msg) in cases {
        let (c, l, k, m) = err(src);
        assert_eq!((c.as_str(), l, k), (code, line, col), "{src}: {m}");
        assert!(m.contains(msg), "{src}: {m}");
    }
}

#[test]
fn parser_errors() {
    let cases = [
        (
            "fn main() { let x = ; }",
            "E002",
            1,
            21,
            "expected an expression",
        ),
        ("fn main() { let x 5; }", "E002", 1, 19, "expected `=`"),
        ("fn main() { if x { }", "E002", 1, 21, "expected `}`"),
        ("main() {}", "E002", 1, 1, "expected `fn`"),
        ("fn f(x) {}", "E002", 1, 7, "expected `:`"),
        ("fn f(x: string) {}", "E002", 1, 9, "unknown type"),
        ("fn main() { 1 + 2 }", "E002", 1, 19, "expected `;`"),
        ("fn main() { print(1) }", "E002", 1, 22, "expected `;`"),
    ];
    for (src, code, line, col, msg) in cases {
        let (c, l, k, m) = err(src);
        assert_eq!((c.as_str(), l, k), (code, line, col), "{src}: {m}");
        assert!(m.contains(msg), "{src}: {m}");
    }
}

#[test]
fn semantic_errors() {
    let cases = [
        (
            "fn main() { print(y); }",
            "E010",
            1,
            19,
            "cannot find variable `y`",
        ),
        (
            "fn main() { let x = true + 1; }",
            "E011",
            1,
            21,
            "expects `i64` operands",
        ),
        (
            "fn main() { let x: bool = 3; }",
            "E011",
            1,
            27,
            "declared `bool`",
        ),
        (
            "fn main() { let x = 1; x = 2; }",
            "E012",
            1,
            24,
            "immutable",
        ),
        (
            "fn f(a: i64) -> i64 { return a; } fn main() { print(f(1, 2)); }",
            "E013",
            1,
            53,
            "takes 1 argument but 2 were",
        ),
        (
            "fn main() { print(g(1)); }",
            "E014",
            1,
            19,
            "cannot find function `g`",
        ),
        (
            "fn f(a: i64) -> i64 { if a > 0 { return 1; } } fn main() { print(f(1)); }",
            "E015",
            1,
            4,
            "not every path returns",
        ),
        (
            "fn f() {} fn f() {} fn main() {}",
            "E016",
            1,
            14,
            "defined twice",
        ),
        (
            "fn main() { let x = 1; let x = 2; }",
            "E016",
            1,
            28,
            "already declared",
        ),
        ("fn f() {}", "E017", 1, 1, "no `main`"),
        (
            "fn main() -> i64 { return 1; }",
            "E017",
            1,
            4,
            "`main` must",
        ),
        (
            "fn main() { if 1 { } }",
            "E018",
            1,
            16,
            "condition must be `bool`",
        ),
        (
            "fn f() -> i64 { return 1; print(2); } fn main() {}",
            "E019",
            1,
            27,
            "unreachable",
        ),
        ("fn main() { 1 + 2; }", "E020", 1, 13, "no effect"),
        (
            "fn main() { let s = \"x\"; }",
            "E011",
            1,
            21,
            "string literals",
        ),
        (
            "fn f() {} fn main() { let v = f(); }",
            "E011",
            1,
            31,
            "type `()`",
        ),
        (
            "fn main() { return 5; }",
            "E011",
            1,
            20,
            "declared without `->`",
        ),
        (
            "fn f() -> i64 { return; } fn main() {}",
            "E011",
            1,
            17,
            "without a value",
        ),
        (
            "fn main() { print(1 == true); }",
            "E011",
            1,
            19,
            "cannot compare",
        ),
        (
            "fn f(a: bool) {} fn main() { f(1); }",
            "E011",
            1,
            32,
            "parameter is `bool`",
        ),
        (
            "fn main() { let b = -true; }",
            "E011",
            1,
            22,
            "cannot apply `-`",
        ),
        (
            "fn main() { print(main); }",
            "E010",
            1,
            19,
            "cannot find variable `main`",
        ),
    ];
    for (src, code, line, col, msg) in cases {
        let (c, l, k, m) = err(src);
        assert_eq!((c.as_str(), l, k), (code, line, col), "{src}: {m}");
        assert!(m.contains(msg), "{src}: {m}");
    }
}

#[test]
fn rendering_shows_source_and_caret() {
    let src = "fn main() {\n    let x: bool = 3;\n}\n";
    let d = compile(src, "demo.ash", Options::default()).unwrap_err();
    let text = render(src, "demo.ash", &d, false);
    assert!(text.starts_with("error[E011]: type mismatch"), "{text}");
    assert!(text.contains("--> demo.ash:2:19"), "{text}");
    assert!(text.contains("2 |     let x: bool = 3;"), "{text}");
    assert!(text.contains("^ this has type `i64`"), "{text}");
    let colored = render(src, "demo.ash", &d, true);
    assert!(colored.contains("\x1b[31;1m"));
}

#[test]
fn notes_help_common_mistakes() {
    let d = compile(
        "fn main() { let x = 1; x = 2; }",
        "t.ash",
        Options::default(),
    )
    .unwrap_err();
    assert!(d.notes.iter().any(|n| n.contains("let mut x")));
    let d = compile("fn main() { print(print); }", "t.ash", Options::default()).unwrap_err();
    assert_eq!(d.code, "E010");
    let d = compile("fn f() {} fn main() { f; }", "t.ash", Options::default()).unwrap_err();
    assert!(d.notes.iter().any(|n| n.contains("call it")));
}
