//! Pickling quotes: typed trees pickled read back equal; dotty's quotes read and pickled again
//! give their bytes.

use std::path::Path;
use syntax::ast::Span;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::write::text_line_sizes;
use tasty::{Ast, Kind, SymbolId, Tasty, TastySpan, Universe, decode_positions, dump, line_sizes};
use typer::dump::library_jars;
use typer::pickle::{QuoteSource, pickle_positioned_quote, pickle_quote};
use typer::tpd::print::Printer;
use typer::tpd::{TNodeId, Tpd, Tree};
use typer::type_unit;
use typer::unpickle::{Unpickled, quote};

/// The library with the jars `extra` of the repository.
fn universe(extra: &[&str]) -> Option<Universe> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut jars = library_jars(&root.join("ref/libs"));
    jars.extend(extra.iter().map(|j| root.join(j)));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    Some(Universe::open(&jars).expect("the library"))
}

/// The name table shown and the `ASTs` section of TASTy `bytes`.
fn parts(bytes: &[u8]) -> (Vec<String>, Vec<u8>) {
    let t = Tasty::parse(bytes).unwrap();
    let names = t.names.iter().map(|(r, _)| format!("{:?}", t.names.get(r))).collect();
    (names, t.section("ASTs").unwrap().bytes.to_vec())
}

/// The `ASTs` section of TASTy `bytes`, printed.
fn shown_ast(bytes: &[u8]) -> String {
    let t = Tasty::parse(bytes).unwrap();
    match Ast::parse(t.section("ASTs").unwrap().bytes) {
        Ok(ast) => dump(&ast, &t.names),
        Err(e) => format!("unparsed: {e:?}"),
    }
}

/// `printed` without the span after each tree's class name.
fn without_spans(printed: &str) -> String {
    let mut out = String::new();
    for (i, part) in printed.split('(').enumerate() {
        if i > 0 {
            out.push('(');
        }
        match part.split_once(' ').filter(|_| i > 0) {
            Some((class, rest)) => {
                out.push_str(class);
                out.push_str(rest.find([' ', ')']).map_or("", |at| &rest[at..]));
            }
            None => out.push_str(part),
        }
    }
    out
}

/// The right-hand sides of the vals named `vals` in unit `text`, typed.
fn typed_rhss(u: &mut Universe, text: &str, vals: &[&str]) -> (Tpd, Names, Vec<TNodeId>) {
    let source = SourceFile::new("t.scala", text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::from_options(&[]));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &source, cfg).unit(root);
    let typed = type_unit(u, &mut p.ast, (p.s.names, &mut p.fresh), (root, cfg), None);
    let Ok(typed) = typed else { panic!("{text} untyped") };
    let mut rhss = Vec::new();
    let mut todo = vec![typed.root];
    while let Some(t) = todo.pop() {
        if let Tree::ValDef { name, rhs: Some(rhs), .. } = *typed.tpd.tree(t)
            && vals.contains(&names.text(name.name))
        {
            rhss.push(rhs);
        }
        todo.extend(typed.tpd.children(t).iter());
    }
    (typed.tpd, names, rhss)
}

#[test]
fn typed_expressions_read_back_equal() {
    let Some(mut u) = universe(&[]) else { return };
    let text = "object T {
        val a = if \"x\".length < 2 then 1 else Math.max(3, 4)
        val b = new java.lang.StringBuilder(\"y\").append(1.5f).toString
        val c = List[Long](1L, 'c'.toLong).apply(0)
        val d = while false do println('d')
        val e = (scala.Predef.identity[String](null): CharSequence)
        val f = { -1.0; Nil.isEmpty }
    }";
    let vals = ["a", "b", "c", "d", "e", "f"];
    let (mut tpd, mut names, rhss) = typed_rhss(&mut u, text, &vals);
    assert_eq!(rhss.len(), vals.len());
    for rhs in rhss {
        let bindings = tpd.list(&[]);
        let tree = Tree::Inlined { call: None, bindings, expansion: rhs };
        let root = tpd.node(tree, Span::NONE, tpd.get(rhs).tpe, None);
        let bytes = pickle_quote((&mut u, &tpd, &names), root, &[]).unwrap();
        let unit = u.root();
        let Ok(Unpickled::Quote(q)) = quote((&mut u, &mut tpd, &mut names), bytes, unit, &[]) else {
            panic!("unread");
        };
        let Tree::Inlined { expansion, .. } = *tpd.tree(q.root) else { panic!("not inlined") };
        let [ours, read] = [rhs, expansion].map(|t| {
            let mut p = Printer { tpd: &tpd, names: &names, u: &mut u, out: String::new(), source_len: 0 };
            p.tree(t);
            without_spans(&p.out)
        });
        assert_eq!(read, ours.replace("(InferredTypeTree", "(TypeTree"));
    }
}

#[test]
fn spans_of_typed_trees_read_back_in_utf16_units() {
    let Some(mut u) = universe(&[]) else { return };
    let text = "object T { val é = \"ü\"; val a = é.length + 1 }";
    let (mut tpd, mut names, rhss) = typed_rhss(&mut u, text, &["a"]);
    let source = SourceFile::new("t.scala", text);
    let bindings = tpd.list(&[]);
    let span = tpd.span(rhss[0]);
    let root = tpd.node(
        Tree::Inlined { call: None, bindings, expansion: rhss[0] },
        span,
        tpd.get(rhss[0]).tpe,
        None,
    );
    let quote_source = QuoteSource::of("t.scala".into(), &source, 0);
    let bytes = pickle_positioned_quote((&mut u, &tpd, &names), root, &[], &quote_source).unwrap();
    let unit = u.root();
    let Ok(Unpickled::Quote(q)) = quote((&mut u, &mut tpd, &mut names), bytes, unit, &[]) else {
        panic!("unread")
    };
    let Tree::Inlined { expansion, .. } = *tpd.tree(q.root) else { panic!("not inlined") };
    let read = tpd.span(expansion).in_own_source();
    let start = text.find("é.length").unwrap() as u32 - 2;
    assert_eq!((read.start(), read.end()), (start, start + 12));
}

/// The module class `name` of package `pkg`.
fn module_class(u: &mut Universe, pkg: &str, name: &str) -> SymbolId {
    let pkg = u.package(pkg).unwrap();
    let decls = u.decl(pkg, name).unwrap();
    *decls.iter().find(|&&c| u.symbol(c).kind == Kind::Class).unwrap()
}

/// The source of quote `bytes`: its path, last in the name table, and its line sizes, of the
/// source in the repository or Scala Native's checkout if there, else of the quote.
fn quote_source(bytes: &[u8]) -> QuoteSource<'static> {
    let t = Tasty::parse(bytes).unwrap();
    let last = t.names.iter().last().unwrap().0;
    let path = t.names.show(last);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = [root.join(&path), root.join("ref/scala-native").join(&path)]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok());
    let line_sizes = match text {
        Some(text) => text_line_sizes(&text),
        None => line_sizes(t.section("Positions").unwrap().bytes).unwrap(),
    };
    QuoteSource { path, line_sizes, file: None }
}

/// Reads quote `file` of `tests/pickle` owned by `owner` and pickles it again: how the names or
/// the bytes differ from the file's, or with `exact` unset the trees printed without addresses,
/// if they do.
fn repickled(u: &mut Universe, owner: SymbolId, file: &str, exact: bool) -> Result<(), String> {
    let path = format!("{}/../../tests/pickle/{file}.tasty", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(path).unwrap();
    let (mut tpd, mut names) = (Tpd::default(), Names::default());
    let read = quote((&mut *u, &mut tpd, &mut names), bytes.clone(), owner, &[]);
    let Ok(Unpickled::Quote(q)) = read else { return Err(format!("{file} unread: {:?}", read.err())) };
    let source = quote_source(&bytes);
    let ours = pickle_positioned_quote((u, &tpd, &names), q.root, &q.holes, &source);
    let ours = ours.map_err(|e| format!("{file}: {e}"))?;
    let ((our_names, our_ast), (names, ast)) = (parts(&ours), parts(&bytes));
    if our_names != names {
        return Err(format!("{file}: names {our_names:?}, not {names:?}"));
    }
    let (printed, expected) = (shown_ast(&ours), shown_ast(&bytes));
    let same_trees =
        if exact { our_ast == ast } else { without_addresses(&printed) == without_addresses(&expected) };
    if !same_trees {
        return Err(format!("{file}: ASTs differ:\n{printed}\n{expected}"));
    }
    match if exact { ours == bytes } else { same_spans(&ours, &bytes) } {
        true => Ok(()),
        false => Err(format!("{file}: positions differ:\n{:?}\n{:?}", spans(&ours), spans(&bytes))),
    }
}

/// The spans of the Positions section of TASTy `bytes`, by address.
fn spans(bytes: &[u8]) -> Vec<(u32, TastySpan)> {
    let t = Tasty::parse(bytes).unwrap();
    let mut spans: Vec<_> =
        decode_positions(t.section("Positions").unwrap().bytes).unwrap().into_iter().collect();
    spans.sort_by_key(|&(a, _)| a);
    spans
}

/// Whether TASTy `ours` and `theirs` give the same spans in the same order, addresses aside.
fn same_spans(ours: &[u8], theirs: &[u8]) -> bool {
    let [a, b] = [ours, theirs].map(|t| spans(t).into_iter().map(|(_, s)| s).collect::<Vec<_>>());
    a == b
}

/// `printed` without the addresses of definitions and binders.
fn without_addresses(printed: &str) -> String {
    let mut out = String::new();
    let mut chars = printed.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '@' || c == '#' {
            while chars.next_if(char::is_ascii_digit).is_some() {}
        }
    }
    out
}

/// Whether each quote of `files` pickles to its bytes, or with `exact` unset to the same trees.
fn all_repickled(u: &mut Universe, owner: SymbolId, files: &[&str], exact: bool) {
    let failed: Vec<String> = files.iter().filter_map(|f| repickled(u, owner, f, exact).err()).collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn quotes_of_unsafe_package_compat_pickle_to_their_bytes() {
    let Some(mut u) = universe(&[]) else { return };
    let owner = module_class(&mut u, "scala.scalanative.unsafe", "UnsafePackageCompat$");
    all_repickled(&mut u, owner, &["unsafe-package-compat-1", "unsafe-package-compat-2"], true);
}

#[test]
fn quotes_with_definitions_pickle_to_their_bytes() {
    let Some(mut u) = universe(&["tests/pickle/probe.jar"]) else { return };
    let owner = module_class(&mut u, "probe", "Probe$");
    let quotes = [
        "probe-vals",
        "probe-defs",
        "probe-folded",
        "probe-matching",
        "probe-options",
        "probe-alternatives",
        "probe-dollar",
    ];
    all_repickled(&mut u, owner, &quotes, true);
}

/// Quotes whose types dotty shares apart where they refer to a package as its `this` and as a
/// reference, which TASTy writes alike: pickled to the same trees, the types shared otherwise.
#[test]
fn quotes_with_package_prefixes_pickle_to_their_trees() {
    let Some(mut u) = universe(&["tests/pickle/probe.jar"]) else { return };
    let owner = module_class(&mut u, "probe", "Probe$");
    let quotes = [
        "probe-lambda",
        "probe-function",
        "probe-fold",
        "probe-higher",
        "probe-alias",
        "probe-vars",
        "probe-tries",
        "probe-clauses",
    ];
    all_repickled(&mut u, owner, &quotes, false);
}
