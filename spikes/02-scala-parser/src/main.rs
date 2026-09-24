mod corpus;
mod lex;
mod nesting;
mod ts;

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

use corpus::{Corpus, mb_per_s};
use lex::{Lexer, Tok};

struct Args {
    mode: String,
    iters: u32,
    dump: Option<PathBuf>,
    roots: Vec<PathBuf>,
}

fn args() -> Args {
    let mut it = std::env::args().skip(1);
    let mode = it.next().unwrap_or_default();
    let mut args = Args { mode, iters: 5, dump: None, roots: Vec::new() };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--iters" => args.iters = it.next().and_then(|n| n.parse().ok()).expect("--iters N"),
            "--dump" => args.dump = it.next().map(PathBuf::from),
            _ => args.roots.push(arg.into()),
        }
    }
    args
}

fn main() -> io::Result<()> {
    let args = args();
    let mut dump: Box<dyn Write> = match &args.dump {
        Some(path) => Box::new(BufWriter::new(File::create(path)?)),
        None => Box::new(io::sink()),
    };
    for root in &args.roots {
        let corpus = Corpus::load(root);
        let (files, bytes) = (corpus.files.len(), corpus.bytes());
        print!("{}: {files} files, {:.2} MB", corpus.name, bytes as f64 / 1e6);
        match args.mode.as_str() {
            "ts" => {
                let s = ts::check(&corpus, &mut dump);
                let parse = ts::bench(&corpus, args.iters, false);
                let walk = ts::bench(&corpus, args.iters, true);
                print!(", {} failed ({:.1}%)", s.failed, 100.0 * s.failed as f64 / files as f64);
                print!(", {} ERROR, {} MISSING, {} nodes", s.errors, s.missing, s.nodes);
                print!(", parse {:.1} MB/s", mb_per_s(bytes, args.iters, parse));
                println!(", parse+walk {:.1} MB/s", mb_per_s(bytes, args.iters, walk));
            }
            "lex" => {
                let (tokens, failed) = lex_check(&corpus, &mut dump);
                let secs = lex_bench(&corpus, args.iters);
                print!(", {tokens} tokens, {failed} files with lex errors");
                println!(", {:.1} MB/s", mb_per_s(bytes, args.iters, secs));
            }
            "sexp" => {
                println!();
                let mut parser = ts::parser();
                for src in &corpus.files {
                    let tree = parser.parse(&src.text, None).expect("parse");
                    writeln!(dump, "{}\n{}", src.path.display(), tree.root_node().to_sexp())?;
                }
            }
            "tokens" => {
                println!();
                for src in &corpus.files {
                    for tok in Lexer::new(&src.text) {
                        let text = &src.text[tok.start as usize..tok.end as usize];
                        writeln!(dump, "{:?}\t{text}", tok.kind)?;
                    }
                }
            }
            "nest" => {
                let (files, defs) = nesting::check(&corpus, &mut dump);
                println!(", {files} files with {defs} suspicious definition nestings");
            }
            _ => panic!("usage: scala-parser-spike <ts|lex|nest> [--iters N] [--dump FILE] <root>..."),
        }
    }
    dump.flush()
}

fn lex_check(corpus: &Corpus, dump: &mut impl Write) -> (usize, usize) {
    let (mut tokens, mut failed) = (0, 0);
    for src in &corpus.files {
        let mut lexer = Lexer::new(&src.text);
        let mut first_error = None;
        for tok in lexer.by_ref() {
            tokens += 1;
            if tok.kind == Tok::Error && first_error.is_none() {
                first_error = Some(tok.start as usize);
            }
        }
        if lexer.errors > 0 {
            failed += 1;
            let at = first_error.unwrap_or(src.text.len());
            let line = src.text[..at].matches('\n').count() + 1;
            writeln!(dump, "{}\t{line}\t{} errors", src.path.display(), lexer.errors).ok();
        }
    }
    (tokens, failed)
}

fn lex_bench(corpus: &Corpus, iters: u32) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        for src in &corpus.files {
            std::hint::black_box(Lexer::new(&src.text).count());
        }
    }
    start.elapsed().as_secs_f64()
}
