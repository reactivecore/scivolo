// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! Where a lifted local class is instantiated and named: its constructor calls pass the values
//! it captures, and it is named `Outer$Name$N`.

use super::local_free::{Local, within};
use super::walk::{children, is_class_def};
use crate::Lower;
use names::name_kinds;
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::SymbolId;
use tasty::tags::OBJECT;
use typer::tpd::{TNodeId, Tree};

/// A constructor call `new C(...)` with the innermost method and class definition around it.
pub struct ConstructorCall {
    apply: TNodeId,
    def: Option<SymbolId>,
    class: Option<SymbolId>,
}

/// Finds the constructor calls of unit `root` by the class they instantiate, each class's in
/// `subtrees` order.
pub fn constructor_calls(l: &mut Lower, root: TNodeId) -> FxHashMap<SymbolId, Vec<ConstructorCall>> {
    let mut around = FxHashMap::default();
    let mut todo = vec![(root, None, None)];
    while let Some((n, def, class)) = todo.pop() {
        if matches!(l.tpd.tree(n), Tree::Apply { .. }) {
            around.entry(n).or_insert((def, class));
        }
        let def = if matches!(l.tpd.tree(n), Tree::DefDef { .. }) { l.tpd.sym(n) } else { def };
        let class = if is_class_def(l.tpd, n) { l.tpd.sym(n) } else { class };
        todo.extend(children(l.tpd, n).into_iter().map(|c| (c, def, class)));
    }
    let mut calls: FxHashMap<SymbolId, Vec<ConstructorCall>> = FxHashMap::default();
    for apply in super::walk::subtrees(l.tpd, root) {
        let Tree::Apply { fun, .. } = *l.tpd.tree(apply) else { continue };
        let ctor = super::walk::without_type_args(l.tpd, fun);
        let Tree::Select { qualifier, .. } = *l.tpd.tree(ctor) else { continue };
        let Tree::New { tpt } = *l.tpd.tree(qualifier) else { continue };
        let Some(c) = l.u.class_symbol(l.tpd.tpe(tpt)) else { continue };
        let (def, class) = around.get(&apply).copied().unwrap_or_default();
        calls.entry(c).or_default().push(ConstructorCall { apply, def, class });
    }
    calls
}

/// Passes the values of local class `c`'s free values, or in its class itself its `fields`, first
/// to each of its constructor calls `calls`; records the values passed outside the class in
/// `passed`. In classes nested in the class the values stay for `nested_reads` to read from the
/// fields.
pub fn pass_values(
    l: &mut Lower,
    calls: &[ConstructorCall],
    c: &Local,
    fields: &[SymbolId],
    passed: &mut FxHashSet<TNodeId>,
) {
    let (class, free) = (c.class, &c.free);
    for &ConstructorCall { apply: t, def, class: innermost } in calls {
        let Tree::Apply { fun, args } = *l.tpd.tree(t) else { continue };
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
                _ => l.ident(v, l.tpd.span(fun).end_pos()),
            };
            values.push(value);
        }
        values.extend(l.tpd.items(args).iter().copied());
        let args = l.tpd.list(&values);
        l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
    }
}

/// Names local class `class` in class `outer` `Outer$Name$N`, an object's class `Outer$Name$N$`
/// and no longer an object's.
pub fn name_class(l: &mut Lower, class: SymbolId, outer: SymbolId) {
    let class_name = l.u.symbol(class).name.clone();
    l.local_names.insert(class, class_name.clone());
    let (base, module) = match tasty::module_name(&class_name) {
        Some(m) if l.u.is_module_class(class) => (m, "$"),
        _ => (class_name.as_str(), ""),
    };
    let flags = l.u.symbol(class).flags.without(OBJECT);
    l.u.set_flags(class, flags);
    let local = l.fresh_name(outer, &name_kinds::unique_prefix(base));
    let local = [local.as_str(), module].concat();
    let outer_name = l.u.binary_name(outer);
    let outer_name = outer_name.rsplit('.').next().unwrap_or(&outer_name).to_owned();
    let name = match outer_name.strip_suffix('$') {
        Some(module_class) => name_kinds::flat(module_class, &local),
        None => name_kinds::flat(&outer_name, &local),
    };
    l.u.rename(class, &name);
}
