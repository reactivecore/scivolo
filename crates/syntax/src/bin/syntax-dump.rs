//! Prints what the scanner makes of Scala files, in the format of `scripts/syntax-oracle`.
//! Usage: syntax-dump tokens --list LIST, one `path<TAB>options` per line.

use std::fmt::Write as _;
use std::io::Write as _;
use syntax::config::Config;
use syntax::diag::{Diagnostic, Level};
use syntax::names::Names;
use syntax::scan::{Scanner, TokenData};
use syntax::source::SourceFile;
use syntax::token::Token;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [mode, flag, list] = args.as_slice() else { return usage() };
    if mode != "tokens" || flag != "--list" {
        return usage();
    }
    let jobs = std::fs::read_to_string(list).expect("reading the list");
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in jobs.lines().filter(|l| !l.is_empty()) {
        let (path, options) = line.split_once('\t').unwrap_or((line, ""));
        let options: Vec<&str> = options.split(' ').filter(|o| !o.is_empty()).collect();
        let mut b = format!("== file {path}\n");
        match std::fs::read(path) {
            Ok(bytes) => tokens(SourceFile::new(path, String::from_utf8_lossy(&bytes)), &options, &mut b),
            Err(e) => writeln!(b, "== crash {e}").unwrap(),
        }
        out.write_all(b.as_bytes()).unwrap();
    }
}

fn usage() {
    eprintln!("usage: syntax-dump tokens --list LIST");
    std::process::exit(2);
}

fn tokens(source: SourceFile, options: &[&str], b: &mut String) {
    let mut names = Names::default();
    let mut s = Scanner::new(&source, &mut names, Config::from_options(options));
    loop {
        line(&s, &s.cur, b);
        if s.token() == Token::Eof {
            break;
        }
        s.next_token();
    }
    let mut shown: Vec<Diagnostic> = Vec::new();
    for d in s.diags.items.iter() {
        if !shown
            .iter()
            .any(|e| e.offset == d.offset && (e.level == Level::Error || d.level == Level::Warning))
        {
            shown.push(d.clone());
        }
    }
    for d in shown {
        let level = if d.level == Level::Error { "E" } else { "W" };
        let message: Vec<u16> = d.message.encode_utf16().collect();
        writeln!(b, "== {level} {} {}", d.offset, quote(&message)).unwrap();
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
