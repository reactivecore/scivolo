//! An input's statements as a compilation unit, as dotty's `ReplPhase` wraps them: expressions
//! become `val res<N> = e`, the statements the body of `object rs$line$<N>` in the empty
//! package, after the imports of earlier lines; then desugared.

use crate::input::Parsed;
use crate::{report, show_methods};
use syntax::ast::{Ast, NodeId, Span, TName, Tree};
use syntax::desugar::Desugar;
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use syntax::parse::Parser;
use syntax::source::SourceFile;

/// The desugared unit of an input.
pub struct Wrapped {
    pub root: NodeId,
    /// The input's top-level imports as source text.
    pub imports: Vec<String>,
    /// The next free `res` index.
    pub next_res: u32,
}

/// `parsed` as the unit of `object`, after the import statements `prelude`, with `res` the next
/// free `res` index.
pub fn wrap(
    parsed: &mut Parsed,
    object: &str,
    res: u32,
    prelude: &str,
    names: &mut Names,
    fresh: &mut FreshNames,
) -> Result<Wrapped, Vec<String>> {
    let mut next_res = res;
    let ast = &mut parsed.ast;
    let stats = definitions(ast, names, &parsed.stats, &mut next_res);
    let imports = stats.iter().filter_map(|&s| import_text(ast, &parsed.source, s)).collect();
    let shows = show_methods::source(ast, names, &stats);
    let text = format!("{prelude}\nobject {object} {{\n{shows}}}\n");
    let mut outer = synthetic(ast, names, fresh, &text);
    let Some(&Tree::ModuleDef { imp, .. }) = outer.last().map(|&m| ast.tree(m)) else {
        unreachable!("the wrapper object")
    };
    let Tree::Template { body, .. } = *ast.tree(imp) else { unreachable!("the wrapper's template") };
    let all: Vec<NodeId> = stats.iter().chain(ast.items(body)).copied().collect();
    let all = ast.list(&all);
    if let Tree::Template { body, .. } = &mut ast.get_mut(imp).tree {
        *body = all;
    }
    let empty = TName { name: names.intern("<empty>"), is_type: false };
    let pid = ast.node(Tree::Ident { name: empty });
    let stats = ast.list(std::mem::take(&mut outer).as_slice());
    let package = ast.node(Tree::PackageDef { pid, stats });
    let mut diags = Diagnostics::default();
    let root = Desugar::new(ast, names, &mut diags, fresh, &parsed.source, parsed.cfg).unit(package);
    if diags.has_errors() {
        return Err(report::diagnostics(&parsed.source, &diags));
    }
    Ok(Wrapped { root, imports, next_res })
}

/// The statements of `stats` with each expression bound to the next `res` value and each
/// assignment `x = e` followed by `val x$assign = x` (dotty's `ReplPhase.definitions`).
fn definitions(ast: &mut Ast, names: &mut Names, stats: &[NodeId], res: &mut u32) -> Vec<NodeId> {
    let flat = match stats {
        [s] => match *ast.tree(*s) {
            Tree::Block { stats, expr } => ast.items(stats).iter().copied().chain(expr).collect(),
            _ => stats.to_vec(),
        },
        _ => stats.to_vec(),
    };
    let mut out = Vec::new();
    for s in flat {
        match *ast.tree(s) {
            Tree::Assign { lhs, .. } if matches!(ast.tree(lhs), Tree::Ident { .. }) => {
                let Tree::Ident { name } = *ast.tree(lhs) else { unreachable!("an identifier") };
                let assign = format!("{}$assign", names.text(name.name));
                let rhs = ast.node(Tree::Ident { name });
                out.extend([s, val(ast, names, &assign, rhs)]);
            }
            _ if is_expression(ast, s) => {
                out.push(val(ast, names, &format!("res{res}"), s));
                *res += 1;
            }
            _ => {
                bump(ast, names, s, res);
                out.push(s);
            }
        }
    }
    out
}

/// `val name = rhs`.
fn val(ast: &mut Ast, names: &mut Names, name: &str, rhs: NodeId) -> NodeId {
    let name = TName { name: names.intern(name), is_type: false };
    let tpt = ast.node(Tree::TypeTree);
    let mods = ast.mods(Default::default());
    let v = ast.node(Tree::ValDef { name, tpt, rhs: Some(rhs), mods });
    let span = ast.span(rhs);
    ast.with_span(v, span);
    v
}

fn is_expression(ast: &Ast, t: NodeId) -> bool {
    let statement = matches!(
        ast.tree(t),
        Tree::Import { .. }
            | Tree::Export { .. }
            | Tree::ExtMethods { .. }
            | Tree::Thicket { .. }
            | Tree::Empty
    );
    !statement && ast.is_term(t)
}

/// Moves `res` past the indices of the names `resN` that `t` defines (lampepfl/dotty#3536).
fn bump(ast: &Ast, names: &Names, t: NodeId, res: &mut u32) {
    let name = match ast.tree(t) {
        Tree::Apply { args: trees, .. } | Tree::Tuple { trees } | Tree::PatDef { pats: trees, .. } => {
            ast.items(*trees).iter().for_each(|&t| bump(ast, names, t, res));
            return;
        }
        Tree::Ident { name }
        | Tree::ValDef { name, .. }
        | Tree::DefDef { name, .. }
        | Tree::TypeDef { name, .. }
        | Tree::ModuleDef { name, .. }
        | Tree::Bind { name, .. } => name,
        _ => return,
    };
    let index = names.text(name.name).strip_prefix("res").and_then(|n| n.parse::<u32>().ok());
    if let Some(n) = index.filter(|&n| n >= *res) {
        *res = n + 1;
    }
}

/// The source text of import statement `t`, with `import` in front.
fn import_text(ast: &Ast, source: &SourceFile, t: NodeId) -> Option<String> {
    let Tree::Import { .. } = ast.tree(t) else { return None };
    let span = ast.span(t);
    let text = source.text.get(span.start() as usize..span.end() as usize)?;
    let first = text.strip_prefix("import").is_some_and(|r| r.starts_with(char::is_whitespace));
    Some(if first { text.to_owned() } else { format!("import {text}") })
}

/// The statements of synthetic source `text`, parsed into `ast` without positions.
fn synthetic(ast: &mut Ast, names: &mut Names, fresh: &mut FreshNames, text: &str) -> Vec<NodeId> {
    let source = SourceFile::new("<repl wrapper>", text);
    let mut p = Parser::new(&source, names, Default::default());
    p.ast = std::mem::take(ast);
    p.fresh = std::mem::take(fresh);
    let first = p.ast.node(Tree::Empty).index();
    let stats = p.parse_stats();
    assert!(!p.s.diags.has_errors(), "the wrapper of {text:?} parses");
    let end = p.ast.node(Tree::Empty).index();
    *ast = std::mem::take(&mut p.ast);
    *fresh = std::mem::take(&mut p.fresh);
    for i in first..end {
        ast.get_mut(NodeId::new(i)).span = Span::NONE;
    }
    stats
}
