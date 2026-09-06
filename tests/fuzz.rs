//! Deterministic fuzzing on stable Rust: random token soup and random
//! mutations of valid programs must never panic any stage, and whatever
//! survives to IR must run identically at -O0 and -O1.

use bellows::{Options, compile};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

const ATOMS: &[&str] = &[
    "fn",
    "main",
    "(",
    ")",
    "{",
    "}",
    "let",
    "mut",
    "x",
    "y",
    "f",
    "=",
    ";",
    ":",
    "i64",
    "bool",
    "->",
    "return",
    "if",
    "else",
    "while",
    "print",
    "1",
    "0",
    "-1",
    "9223372036854775807",
    "true",
    "false",
    "\"s\"",
    "+",
    "-",
    "*",
    "/",
    "%",
    "<",
    "<=",
    "==",
    "!=",
    "&&",
    "||",
    "!",
    ",",
];

fn run_both(src: &str) {
    let Ok(u0) = compile(
        src,
        "fuzz.ash",
        Options {
            optimize: false,
            ..Default::default()
        },
    ) else {
        return;
    };
    let u1 = compile(
        src,
        "fuzz.ash",
        Options {
            optimize: true,
            ..Default::default()
        },
    )
    .expect("-O1 must accept what -O0 accepts");
    let mut o0 = Vec::new();
    let mut o1 = Vec::new();
    let mut i0 = bellows::ir::Interp::new(&u0.ir, &mut o0);
    i0.max_depth = 200;
    i0.max_steps = 200_000;
    let r0 = i0.run();
    let mut i1 = bellows::ir::Interp::new(&u1.ir, &mut o1);
    i1.max_depth = 200;
    i1.max_steps = 200_000;
    let r1 = i1.run();
    // The optimizer removes work, so a program that hits the step limit at
    // -O0 may finish at -O1; only compare when neither hit the limit.
    if r0 == Err(bellows::ir::Trap::StepLimit) || r1 == Err(bellows::ir::Trap::StepLimit) {
        return;
    }
    assert_eq!(r0.is_ok(), r1.is_ok(), "trap behaviour differs for:\n{src}");
    if r0.is_ok() {
        assert_eq!(o0, o1, "output differs for:\n{src}");
    }
    assert!(!u1.asm.is_empty());
}

#[test]
fn token_soup_never_panics() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..30_000 {
        let n = 1 + (rng.next() % 30) as usize;
        let src: Vec<&str> = (0..n).map(|_| rng.pick(ATOMS)).collect();
        let src = src.join(" ");
        let _ = compile(&src, "fuzz.ash", Options::default());
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(99);
    for _ in 0..5_000 {
        let n = (rng.next() % 80) as usize;
        let bytes: Vec<u8> = (0..n).map(|_| (rng.next() % 256) as u8).collect();
        let _ = compile(
            &String::from_utf8_lossy(&bytes),
            "fuzz.ash",
            Options::default(),
        );
    }
}

#[test]
fn mutated_programs_agree_between_optimization_levels() {
    let seeds = [
        include_str!("../examples/fib.ash"),
        include_str!("../examples/logic.ash"),
        include_str!("../examples/fizzbuzz.ash"),
        "fn main() { let mut i = 0; while i < 5 { let a = i * 3 + 1; if a % 2 == 0 && i != 2 { print(a / 2); } else { print(-a); } i = i + 1; } }",
    ];
    let mut rng = Rng(7);
    let mut accepted = 0;
    for seed in seeds {
        for _ in 0..400 {
            // Mutate: replace a random token-ish span with a random atom.
            let toks: Vec<&str> = seed.split_whitespace().collect();
            let mut m = toks.clone();
            let k = (rng.next() % m.len() as u64) as usize;
            m[k] = rng.pick(ATOMS);
            if rng.next() % 3 == 0 {
                let j = (rng.next() % m.len() as u64) as usize;
                m[j] = rng.pick(&["1", "0", "-1", "2", "x", "i"]);
            }
            let src = m.join(" ");
            if compile(&src, "fuzz.ash", Options::default()).is_ok() {
                accepted += 1;
            }
            run_both(&src);
        }
    }
    assert!(
        accepted >= 10,
        "mutation should keep some programs valid ({accepted})"
    );
}
