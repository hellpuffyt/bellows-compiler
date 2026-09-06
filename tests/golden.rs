//! Golden tests: every `examples/*.ash` declares its expected output in
//! `// expect:` comments. Each program is run three ways and all must
//! agree with the expectations: interpreted at -O0, interpreted at -O1,
//! and — when a C compiler is available — compiled to native code and
//! executed. Native runs happen on Linux and Windows hosts.

use std::path::Path;
use std::process::Command;

use bellows::{Options, compile};

fn expected(src: &str) -> String {
    src.lines()
        .filter_map(|l| l.trim().strip_prefix("// expect:"))
        .map(|s| s.trim().to_string() + "\n")
        .collect()
}

fn interpret(src: &str, name: &str, optimize: bool) -> String {
    let unit = compile(
        src,
        name,
        Options {
            optimize,
            ..Default::default()
        },
    )
    .unwrap_or_else(|d| panic!("{name}: {d}"));
    let mut out = Vec::new();
    bellows::ir::Interp::new(&unit.ir, &mut out)
        .run()
        .unwrap_or_else(|t| panic!("{name}: {t}"));
    String::from_utf8(out).unwrap()
}

fn cc() -> Option<String> {
    let cc = std::env::var("CC").unwrap_or_else(|_| "gcc".into());
    Command::new(&cc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| cc)
}

fn native(src: &str, name: &str, optimize: bool) -> Option<String> {
    let cc = cc()?;
    if cfg!(target_os = "macos") {
        return None; // Mach-O directives are not supported yet
    }
    let unit = compile(
        src,
        name,
        Options {
            optimize,
            ..Default::default()
        },
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("bellows-golden-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stem = Path::new(name).file_stem().unwrap().to_str().unwrap();
    let asm = dir.join(format!("{stem}-{}.s", optimize as u8));
    let exe = dir.join(format!(
        "{stem}-{}{}",
        optimize as u8,
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::write(&asm, &unit.asm).unwrap();
    let st = Command::new(&cc)
        .arg("-o")
        .arg(&exe)
        .arg(&asm)
        .args(if cfg!(target_os = "linux") {
            vec!["-no-pie"]
        } else {
            vec![]
        })
        .status()
        .unwrap();
    assert!(st.success(), "{name}: assembling/linking failed");
    let out = Command::new(&exe).output().unwrap();
    assert!(
        out.status.success(),
        "{name}: native run failed with {}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8(out.stdout).unwrap().replace("\r\n", "\n"))
}

fn examples() -> Vec<(String, String)> {
    let mut v: Vec<_> = std::fs::read_dir("examples")
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "ash"))
        .map(|e| {
            (
                e.path().display().to_string(),
                std::fs::read_to_string(e.path()).unwrap(),
            )
        })
        .collect();
    v.sort();
    v
}

#[test]
fn examples_match_expectations_interpreted() {
    for (name, src) in examples() {
        let want = expected(&src);
        assert!(!want.is_empty(), "{name} has no // expect: lines");
        assert_eq!(interpret(&src, &name, false), want, "{name} at -O0");
        assert_eq!(interpret(&src, &name, true), want, "{name} at -O1");
    }
}

#[test]
fn examples_match_expectations_native() {
    let mut ran = 0;
    for (name, src) in examples() {
        let want = expected(&src);
        for opt in [false, true] {
            if let Some(got) = native(&src, &name, opt) {
                assert_eq!(got, want, "{name} native at -O{}", opt as u8);
                ran += 1;
            }
        }
    }
    if ran == 0 {
        eprintln!("no C compiler / unsupported host: native golden tests skipped");
    }
}

#[test]
fn runtime_traps_agree_between_interpreter_and_native() {
    let cases = [
        (
            "fn main() { let z = 0; print(10 / z); }",
            "division by zero",
        ),
        (
            "fn main() { let z = 0; print(10 % z); }",
            "division by zero",
        ),
        (
            "fn big(x: i64) -> i64 { return x * 4611686018427387904; } fn main() { print(big(3)); }",
            "integer overflow",
        ),
        (
            "fn neg(x: i64) -> i64 { return -x; } fn main() { print(neg(-9223372036854775807 - 1)); }",
            "integer overflow",
        ),
        (
            "fn m(x: i64, y: i64) -> i64 { return x / y; } fn main() { print(m(-9223372036854775807 - 1, -1)); }",
            "integer overflow",
        ),
    ];
    for (src, want) in cases {
        let unit = compile(src, "trap.ash", Options::default()).unwrap();
        let mut out = Vec::new();
        let err = bellows::ir::Interp::new(&unit.ir, &mut out)
            .run()
            .unwrap_err();
        assert_eq!(err.to_string(), want, "interpreter: {src}");
        if let Some(cc) = cc().filter(|_| !cfg!(target_os = "macos")) {
            let dir = std::env::temp_dir().join(format!("bellows-trap-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let asm = dir.join("t.s");
            let exe = dir.join(format!("t{}", std::env::consts::EXE_SUFFIX));
            std::fs::write(&asm, &unit.asm).unwrap();
            assert!(
                Command::new(&cc)
                    .arg("-o")
                    .arg(&exe)
                    .arg(&asm)
                    .args(if cfg!(target_os = "linux") {
                        vec!["-no-pie"]
                    } else {
                        vec![]
                    })
                    .status()
                    .unwrap()
                    .success()
            );
            let out = Command::new(&exe).output().unwrap();
            assert_eq!(out.status.code(), Some(101), "native exit code: {src}");
            assert!(
                String::from_utf8_lossy(&out.stdout).contains(want),
                "native message: {src}"
            );
        }
    }
}
