//! Where a lifted local class is instantiated and defined: its constructor calls pass the values
//! it captures, and its definition moves from its block to its package as `Outer$Name$N`.

use super::local_free::{Free, within};
use super::walk::{children, is_class_def};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::OBJECT;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Passes the values of `free`, or in `class` itself its `fields`, first to each constructor call
/// of `class` in unit `root`; records the values passed outside `class` in `passed`. In classes
/// nested in `class` the values stay for `nested_reads` to read from the fields.
pub fn pass_values(
    l: &mut Lower,
    root: TNodeId,
    class: SymbolId,
    free: &Free,
    fields: &[SymbolId],
    passed: &mut FxHashSet<TNodeId>,
) -> Result<()> {
    for t in super::walk::subtrees(l.tpd, root) {
        let Tree::Apply { fun, args } = *l.tpd.tree(t) else { continue };
        let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { continue };
        let Tree::New { tpt } = *l.tpd.tree(qualifier) else { continue };
        if l.u.class_symbol(l.tpd.tpe(tpt)) != Some(class) {
            continue;
        }
        let (def, innermost) = enclosing(l, root, t);
        let inside = def.is_some_and(|d| within(l, d, class));
        let span = l.tpd.span(t);
        let mut values = Vec::new();
        for &v in &free.values {
            let value = match free.in_methods.iter().position(|&w| w == v) {
                _ if !inside => {
                    let value = l.ident(v, l.tpd.span(fun).end_pos());
                    passed.insert(value);
                    value
                }
                Some(i) if innermost == Some(class) => {
                    let this = l.this(class, span);
                    l.select(this, fields[i], span)
                }
                Some(_) => l.ident(v, l.tpd.span(fun).end_pos()),
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

/// The innermost method definition and class definition around `t` in `root`.
fn enclosing(l: &Lower, root: TNodeId, t: TNodeId) -> (Option<SymbolId>, Option<SymbolId>) {
    let mut todo = vec![(root, None, None)];
    while let Some((n, def, class)) = todo.pop() {
        if n == t {
            return (def, class);
        }
        let def = if matches!(l.tpd.tree(n), Tree::DefDef { .. }) { l.tpd.sym(n) } else { def };
        let class = if is_class_def(l.tpd, n) { l.tpd.sym(n) } else { class };
        todo.extend(children(l.tpd, n).into_iter().map(|c| (c, def, class)));
    }
    (None, None)
}

/// Names local class `class` in class `outer` `Outer$Name$N`, an object's class `Outer$Name$N$`
/// and no longer an object's.
pub fn name_class(l: &mut Lower, class: SymbolId, outer: SymbolId) {
    let class_name = l.u.symbol(class).name.clone();
    let (base, module) = match tasty::module_name(&class_name) {
        Some(m) if l.u.is_module_class(class) => (m, "$"),
        _ => (class_name.as_str(), ""),
    };
    let flags = l.u.symbol(class).flags.without(OBJECT);
    l.u.set_flags(class, flags);
    let local = l.fresh.next(l.names, &format!("{base}$"));
    let local = format!("{}{module}", l.names.text(local));
    let outer_name = l.u.binary_name(outer);
    let outer_name = outer_name.rsplit('.').next().unwrap_or(&outer_name).to_owned();
    let name = if outer_name.ends_with('$') {
        format!("{outer_name}{local}")
    } else {
        format!("{outer_name}${local}")
    };
    l.u.rename(class, &name);
}

/// Moves class definition `td` of `class` from its block to its package; the block and the index
/// of the statement it was before.
pub fn lift(l: &mut Lower, root: TNodeId, td: TNodeId, class: SymbolId) -> Option<(TNodeId, usize)> {
    let mut package = class;
    while l.u.symbol(package).kind != Kind::Package {
        package = l.u.symbol(package).owner.expect("a class's owner");
    }
    let owner = l.u.symbol(class).owner.expect("a class's owner");
    l.u.set_owner(class, package);
    l.u.enter(class);
    l.local_classes.insert(class, owner);
    let mut from = None;
    for t in super::walk::subtrees(l.tpd, root) {
        match *l.tpd.tree(t) {
            Tree::Block { stats, expr } if l.tpd.items(stats).contains(&td) => {
                let mut kept = l.tpd.items(stats).to_vec();
                let index = kept.iter().position(|&s| s == td).expect("the class definition");
                kept.remove(index);
                from = Some((t, index));
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
    from
}
