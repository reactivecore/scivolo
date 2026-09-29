//! Prints what the scanner, parser or desugaring makes of Scala files, in the format of
//! `scripts/syntax-oracle`.
//! Usage: syntax-dump tokens|trees|desugar --list LIST, one `path<TAB>options` per line.

use std::fmt::Write as _;
use std::io::Write as _;
use syntax::ast::print::Printer;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::diag::{Diagnostics, Level};
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::scan::{Scanner, TokenData};
use syntax::source::SourceFile;
use syntax::token::Token;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [mode, flag, list] = args.as_slice() else { return usage() };
    let run: fn(&SourceFile, &[&str], &mut String) = match mode.as_str() {
        "tokens" => tokens,
        "trees" => trees,
        "desugar" => desugared,
        _ => return usage(),
    };
    if flag != "--list" {
        return usage();
    }
    let jobs = std::fs::read_to_string(list).expect("reading the list");
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in jobs.lines().filter(|l| !l.is_empty()) {
        let (path, options) = line.split_once('\t').unwrap_or((line, ""));
        let options: Vec<&str> = options.split(' ').filter(|o| !o.is_empty()).collect();
        let mut b = format!("== file {path}\n");
        match std::fs::read(path) {
            Ok(bytes) => run(&SourceFile::new(path, String::from_utf8_lossy(&bytes)), &options, &mut b),
            Err(e) => writeln!(b, "== crash {e}").unwrap(),
        }
        out.write_all(b.as_bytes()).unwrap();
    }
}

fn usage() {
    eprintln!("usage: syntax-dump tokens|trees|desugar --list LIST");
    std::process::exit(2);
}

fn tokens(source: &SourceFile, options: &[&str], b: &mut String) {
    let mut names = Names::default();
    let mut s = Scanner::new(source, &mut names, Config::from_options(options));
    loop {
        line(&s, &s.cur, b);
        if s.token() == Token::Eof {
            break;
        }
        s.next_token();
    }
    diagnostics(&s.diags, b);
}

fn trees(source: &SourceFile, options: &[&str], b: &mut String) {
    parse_and_print(source, options, false, b);
}

fn desugared(source: &SourceFile, options: &[&str], b: &mut String) {
    parse_and_print(source, options, true, b);
}

fn parse_and_print(source: &SourceFile, options: &[&str], desugar: bool, b: &mut String) {
    let mut names = Names::default();
    let mut p = Parser::new(source, &mut names, Config::from_options(options));
    let mut root = p.parse();
    if desugar {
        let cfg = p.s.cfg;
        let mut d = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, source, cfg);
        root = d.unit(root);
    }
    let mut printer = Printer { ast: &p.ast, names: p.s.names, out: String::new() };
    printer.tree(root);
    b.push_str(&printer.out);
    b.push('\n');
    diagnostics(&p.s.diags, b);
}

fn diagnostics(diags: &Diagnostics, b: &mut String) {
    for d in diags.unique() {
        let level = if d.level == Level::Error { "E" } else { "W" };
        let message = syntax::names::to_utf16(&d.message);
        writeln!(b, "== {level} {} {}", d.pos(), quote(&message)).unwrap();
    }
}

fn line(s: &Scanner, td: &TokenData, b: &mut String) {
    let t = td.token;
    write!(b, "{} {} {} ", t as u8, td.offset, td.last_offset).unwrap();
    match td.line_offset {
        Some(o) => write!(b, "{o}").unwrap(),
        None => b.push('-'),
    }
    if matches!(t, Token::Identifier | Token::BackquotedIdent | Token::InterpolationId) {
        let name = syntax::names::to_utf16(td.name.map_or("null", |n| s.names.text(n)));
        write!(b, " {}", quote(&name)).unwrap();
    } else if (Token::CharLit..=Token::StringPart).contains(&t) || t == Token::QuoteId {
        write!(b, " {}", quote(s.str_val(td).unwrap_or(&[]))).unwrap();
    }
    if matches!(t, Token::IntLit | Token::LongLit) {
        write!(b, " base={}", td.base).unwrap();
    }
    b.push('\n');
}

/// `units` quoted as the oracle does: escapes for controls and lone surrogates.
fn quote(units: &[u16]) -> String {
    let mut q = String::from("\"");
    for c in char::decode_utf16(units.iter().copied()) {
        match c {
            Ok('"') => q.push_str("\\\""),
            Ok('\\') => q.push_str("\\\\"),
            Ok('\n') => q.push_str("\\n"),
            Ok('\r') => q.push_str("\\r"),
            Ok('\t') => q.push_str("\\t"),
            Ok(c) if c < ' ' => write!(q, "\\u{:04x}", c as u32).unwrap(),
            Ok(c) => q.push(c),
            Err(e) => write!(q, "\\u{:04x}", e.unpaired_surrogate()).unwrap(),
        }
    }
    q.push('"');
    q
}
