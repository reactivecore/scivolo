//! A method `<name>$show: Any` per value of an input, whose result the REPL prints.

use syntax::ast::flags::LAZY;
use syntax::ast::{Ast, NodeId, Tree};
use syntax::names::{Name, Names, is_var_pattern_name};

/// The suffix of the show methods' names.
pub const SUFFIX: &str = "$show";

/// The definitions of the show methods of the non-lazy values `stats` define, as source text.
pub fn source(ast: &Ast, names: &Names, stats: &[NodeId]) -> String {
    let mut values = Vec::new();
    for &s in stats {
        match ast.tree(s) {
            Tree::ValDef { name, mods, .. } if !ast.modifiers(*mods).is(LAZY) => values.push(name.name),
            Tree::PatDef { mods, pats, .. } if !ast.modifiers(*mods).is(LAZY) => {
                ast.items(*pats).iter().for_each(|&p| bound(ast, names, p, &mut values));
            }
            _ => {}
        }
    }
    values
        .into_iter()
        .map(|n| {
            let n = names.text(n);
            format!("def `{n}{SUFFIX}`: _root_.scala.Any = `{n}`\n")
        })
        .collect()
}

/// The variables pattern `p` binds.
fn bound(ast: &Ast, names: &Names, p: NodeId, out: &mut Vec<Name>) {
    match ast.tree(p) {
        Tree::Ident { name } if !name.is_type && is_variable(names.text(name.name)) => out.push(name.name),
        Tree::Bind { name, body, .. } => {
            out.push(name.name);
            bound(ast, names, *body, out);
        }
        Tree::Typed { expr, .. } => bound(ast, names, *expr, out),
        Tree::Parens { t } => bound(ast, names, *t, out),
        Tree::Tuple { trees } | Tree::Apply { args: trees, .. } => {
            ast.items(*trees).iter().for_each(|&t| bound(ast, names, t, out));
        }
        _ => {}
    }
}

fn is_variable(name: &str) -> bool {
    name != "_" && is_var_pattern_name(name)
}
