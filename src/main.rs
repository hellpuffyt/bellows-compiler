//! `bellows` command line.
//!
//! ```text
//! bellows build  prog.ash [-o out] [-O0] [-g] [--target linux|windows] [--emit tokens|ast|ir|asm] [--cc gcc]
//! bellows run    prog.ash [args...]     compile, assemble+link with the system C compiler, execute
//! bellows interp prog.ash               run through the IR interpreter (no assembler needed)
//! bellows check  prog.ash               diagnostics only
//! bellows stats  prog.ash               optimizer statistics
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use bellows::codegen::Target;
use bellows::{Options, compile, render};

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2)
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  bellows build  FILE.ash [-o OUT] [-O0] [-g] [--target linux|windows] [--emit tokens|ast|ir|asm] [--cc CC]\n  bellows run    FILE.ash [-O0] [-g] [--cc CC]\n  bellows interp FILE.ash\n  bellows check  FILE.ash\n  bellows stats  FILE.ash"
    );
    std::process::exit(2)
}

struct Args {
    file: String,
    out: Option<PathBuf>,
    opts: Options,
    emit: Option<String>,
    cc: String,
    color: bool,
}

fn parse_args(rest: &[String]) -> Args {
    let mut a = Args {
        file: String::new(),
        out: None,
        opts: Options::default(),
        emit: None,
        cc: std::env::var("CC").unwrap_or_else(|_| "gcc".into()),
        color: std::env::var_os("NO_COLOR").is_none(),
    };
    let mut i = 0;
    while i < rest.len() {
        let next = |i: &mut usize| -> String {
            *i += 1;
            rest.get(*i)
                .cloned()
                .unwrap_or_else(|| die(format!("{} needs a value", rest[*i - 1])))
        };
        match rest[i].as_str() {
            "-o" => a.out = Some(PathBuf::from(next(&mut i))),
            "-O0" => a.opts.optimize = false,
            "-O1" | "-O" => a.opts.optimize = true,
            "-g" => a.opts.debug = true,
            "--target" => {
                let t = next(&mut i);
                a.opts.target = Target::parse(&t)
                    .unwrap_or_else(|| die(format!("unknown target {t} (linux or windows)")));
            }
            "--emit" => a.emit = Some(next(&mut i)),
            "--cc" => a.cc = next(&mut i),
            "--no-color" => a.color = false,
            s if s.starts_with('-') => die(format!("unknown option {s}")),
            s => {
                if !a.file.is_empty() {
                    die("only one source file is accepted");
                }
                a.file = s.to_string();
            }
        }
        i += 1;
    }
    if a.file.is_empty() {
        usage();
    }
    a
}

fn read(file: &str) -> String {
    std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {e}")))
}

fn compile_or_exit(a: &Args, src: &str) -> bellows::Unit {
    match compile(src, &a.file, a.opts.clone()) {
        Ok(u) => u,
        Err(d) => {
            eprint!("{}", render(src, &a.file, &d, a.color));
            std::process::exit(1);
        }
    }
}

/// Assembles and links `asm` into `out` with the system C compiler.
fn link(a: &Args, asm: &str, out: &Path) -> Result<(), String> {
    let asm_path = out.with_extension("s");
    std::fs::write(&asm_path, asm).map_err(|e| format!("{}: {e}", asm_path.display()))?;
    let mut cmd = Command::new(&a.cc);
    cmd.arg("-o").arg(out).arg(&asm_path);
    if a.opts.debug {
        cmd.arg("-g");
    }
    if a.opts.target == Target::Linux {
        cmd.arg("-no-pie").arg("-Wl,--no-warn-execstack");
    }
    let status = cmd
        .status()
        .map_err(|e| format!("cannot run `{}`: {e} (install gcc or pass --cc)", a.cc))?;
    if !status.success() {
        return Err(format!("`{}` failed with {status}", a.cc));
    }
    Ok(())
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first() else { usage() };
    let a = parse_args(&argv[1..]);
    let src = read(&a.file);
    match cmd.as_str() {
        "check" => {
            compile_or_exit(&a, &src);
            eprintln!("{}: ok", a.file);
            ExitCode::SUCCESS
        }
        "stats" => {
            let u = compile_or_exit(&a, &src);
            let s = u.stats;
            println!(
                "functions {}  blocks {}  instructions {}\nfolded {}  forwarded {}  dead temps {}  dead stores {}  branches folded {}  jumps threaded {}  blocks removed {}",
                u.ir.funcs.len(),
                u.ir.funcs.iter().map(|f| f.blocks.len()).sum::<usize>(),
                u.ir.funcs
                    .iter()
                    .flat_map(|f| &f.blocks)
                    .map(|b| b.instrs.len() + 1)
                    .sum::<usize>(),
                s.folded,
                s.forwarded,
                s.dead_temps,
                s.dead_stores,
                s.branches_folded,
                s.jumps_threaded,
                s.blocks_removed
            );
            ExitCode::SUCCESS
        }
        "interp" => {
            let u = compile_or_exit(&a, &src);
            let stdout = std::io::stdout();
            let mut interp =
                bellows::ir::Interp::new(&u.ir, std::io::BufWriter::new(stdout.lock()));
            match interp.run() {
                Ok(()) => ExitCode::SUCCESS,
                Err(t) => {
                    eprintln!("runtime error: {t}");
                    ExitCode::from(101)
                }
            }
        }
        "build" | "run" => {
            let u = compile_or_exit(&a, &src);
            if let Some(stage) = &a.emit {
                match stage.as_str() {
                    "tokens" => {
                        for t in &u.tokens {
                            println!("{:>5}..{:<5} {:?}", t.span.start, t.span.end, t.tok);
                        }
                    }
                    "ast" => print!("{}", bellows::parser::dump(&u.ast)),
                    "ir" => print!("{}", bellows::ir::dump(&u.ir)),
                    "asm" => print!("{}", u.asm),
                    other => die(format!(
                        "unknown --emit stage {other} (tokens, ast, ir, asm)"
                    )),
                }
                return ExitCode::SUCCESS;
            }
            let default_out = {
                let stem = Path::new(&a.file)
                    .file_stem()
                    .map_or("a", |s| s.to_str().unwrap_or("a"));
                let mut p = PathBuf::from(stem);
                if a.opts.target == Target::Windows {
                    p.set_extension("exe");
                }
                if cmd == "run" {
                    p = std::env::temp_dir().join(format!(
                        "bellows-{}-{}",
                        std::process::id(),
                        p.display()
                    ));
                }
                p
            };
            let out = a.out.clone().unwrap_or(default_out);
            if let Err(e) = link(&a, &u.asm, &out) {
                die(e);
            }
            if cmd == "build" {
                eprintln!("wrote {}", out.display());
                return ExitCode::SUCCESS;
            }
            let status = Command::new(&out)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .unwrap_or_else(|e| die(format!("cannot run {}: {e}", out.display())));
            let _ = std::fs::remove_file(&out);
            let _ = std::fs::remove_file(out.with_extension("s"));
            ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8)
        }
        other => die(format!("unknown command {other}")),
    }
}
