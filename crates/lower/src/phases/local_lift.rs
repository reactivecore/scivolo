//! Where a lifted local class is instantiated and defined: its constructor calls pass the values
//! it captures, and its definition moves from its block to its package as `Outer$Name$N`.

use super::local_free::{Free, within};
use super::walk::children;
use crate::{Lower, Result, unsupported};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Passes the values of `free`, or inside `class` its `fields`, first to each constructor call of
/// `class` in unit `root`.
pub fn pass_values(
    l: &mut Lower,
    root: TNodeId,
    class: SymbolId,
    free: &Free,
    fields: &[SymbolId],
) -> Result<()> {
    for t in super::walk::subtrees(l.tpd, root) {
        let Tree::Apply { fun, args } = *l.tpd.tree(t) else { continue };
        let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { continue };
        let Tree::New { tpt } = *l.tpd.tree(qualifier) else { continue };
        if l.u.class_symbol(l.tpd.tpe(tpt)) != Some(class) {
            continue;
        }
        let inside = enclosing_def(l, root, t).is_some_and(|d| within(l, d, class));
        let span = l.tpd.span(t);
        let mut values = Vec::new();
        for &v in &free.values {
            let value = match free.in_methods.iter().position(|&w| w == v) {
                _ if !inside => l.ident(v, span),
                Some(i) => {
                    let this = l.this(class, span);
                    l.select(this, fields[i], span)
                }
                None => return unsupported(span, "local classes making instances of themselves"),
            };
            values.push(value);
        }
        values.extend(l.tpd.items(args).iter().copied());
        let args = l.tpd.list(&values);
        l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
    }
    Ok(())
}

/// The innermost method definition around `t` in `root`.
fn enclosing_def(l: &Lower, root: TNodeId, t: TNodeId) -> Option<SymbolId> {
    let mut todo = vec![(root, None)];
    while let Some((n, def)) = todo.pop() {
        if n == t {
            return def;
        }
        let def = match l.tpd.tree(n) {
            Tree::DefDef { .. } => l.tpd.sym(n),
            _ => def,
        };
        todo.extend(children(l.tpd, n).into_iter().map(|c| (c, def)));
    }
    None
}

/// Moves class definition `td` of `class` from its block to the package of `outer`, named
/// `Outer$Name$N`.
pub fn lift(l: &mut Lower, root: TNodeId, td: TNodeId, class: SymbolId, outer: SymbolId) {
    let base = format!("{}$", l.u.symbol(class).name);
    let local = l.fresh.next(l.names, &base);
    let local = l.names.text(local).to_owned();
    let outer_name = l.u.binary_name(outer);
    let outer_name = outer_name.rsplit('.').next().unwrap_or(&outer_name).to_owned();
    let name = if outer_name.ends_with('$') {
        format!("{outer_name}{local}")
    } else {
        format!("{outer_name}${local}")
    };
    let mut package = outer;
    while l.u.symbol(package).kind != Kind::Package {
        package = l.u.symbol(package).owner.expect("a class's owner");
    }
    l.u.rename(class, &name);
    l.u.set_owner(class, package);
    l.u.enter(class);
    l.local_classes.insert(class);
    for t in super::walk::subtrees(l.tpd, root) {
        match *l.tpd.tree(t) {
            Tree::Block { stats, expr } if l.tpd.items(stats).contains(&td) => {
                let kept: Vec<TNodeId> = l.tpd.items(stats).iter().copied().filter(|&s| s != td).collect();
                let stats = l.tpd.list(&kept);
                l.tpd.get_mut(t).tree = Tree::Block { stats, expr };
            }
            Tree::PackageDef { pid, stats }
                if l.tpd.sym(t) == Some(package) || l.tpd.sym(pid) == Some(package) =>
            {
                let mut all = l.tpd.items(stats).to_vec();
                all.push(td);
                let stats = l.tpd.list(&all);
                l.tpd.get_mut(t).tree = Tree::PackageDef { pid, stats };
            }
            _ => {}
        }
    }
}
