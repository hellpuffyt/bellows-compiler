//! Bellows — an optimizing compiler for Ash.
//!
//! Pipeline: [`lexer`] → [`parser`] → [`sema`] → [`ir`] (lowering) →
//! [`opt`] → [`codegen`] (x86-64 assembly). The IR also has an interpreter,
//! used for tests, benchmarks and platforms without an assembler.
//!
//! ```
//! let src = "fn main() { let mut i = 0; while i < 3 { print(i * i); i = i + 1; } }";
//! let unit = bellows::compile(src, "demo.ash", bellows::Options::default()).unwrap();
//! let mut out = Vec::new();
//! bellows::ir::Interp::new(&unit.ir, &mut out).run().unwrap();
//! assert_eq!(String::from_utf8(out).unwrap(), "0\n1\n4\n");
//! ```

pub mod codegen;
pub mod diag;
pub mod ir;
pub mod lexer;
pub mod opt;
pub mod parser;
pub mod sema;

use diag::{Diagnostic, Source};

#[derive(Debug, Clone)]
pub struct Options {
    pub optimize: bool,
    pub target: codegen::Target,
    /// Emit DWARF line info (`.loc`) naming the source file.
    pub debug: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            optimize: true,
            target: codegen::Target::host(),
            debug: false,
        }
    }
}

/// Everything the pipeline produced for one source file.
#[derive(Debug)]
pub struct Unit {
    pub tokens: Vec<lexer::Token>,
    pub ast: parser::Program,
    pub ir: ir::Program,
    pub stats: opt::Stats,
    pub asm: String,
}

/// Runs the whole pipeline. Returns the first diagnostic on failure.
pub fn compile(src: &str, name: &str, opts: Options) -> Result<Unit, Diagnostic> {
    let source = Source::new(name, src);
    let tokens = lexer::lex(src)?;
    let ast = parser::parse(&tokens)?;
    let typed = sema::check(&ast)?;
    let line_of = |off: usize| source.position(off).0 as u32;
    let mut ir = ir::lower(&ast, &typed, &line_of);
    let stats = if opts.optimize {
        opt::optimize(&mut ir)
    } else {
        opt::Stats::default()
    };
    let asm = codegen::generate(&ir, opts.target, opts.debug.then_some(name));
    Ok(Unit {
        tokens,
        ast,
        ir,
        stats,
        asm,
    })
}

/// Renders a diagnostic against its source, rustc-style.
pub fn render(src: &str, name: &str, d: &Diagnostic, color: bool) -> String {
    Source::new(name, src).render(d, color)
}
