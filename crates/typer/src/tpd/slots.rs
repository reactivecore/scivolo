//! The subtree fields of typed trees as values, read and written back without cloning the tree.

use super::{Clauses, Field, List, TNodeId, Tree};
use std::ops::{Deref, DerefMut};

/// A subtree field of a tree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Node(TNodeId),
    Opt(Option<TNodeId>),
    List(List),
    Clauses(Clauses),
}

/// The at most four subtree fields of a tree, in the order of `Tree::fields`.
#[derive(Clone, Copy, Debug)]
pub struct Slots {
    items: [Slot; 4],
    len: usize,
}

impl Slots {
    fn of(slots: &[Slot]) -> Slots {
        let mut items = [Slot::Opt(None); 4];
        items[..slots.len()].copy_from_slice(slots);
        Slots { items, len: slots.len() }
    }
}

impl Deref for Slots {
    type Target = [Slot];

    fn deref(&self) -> &[Slot] {
        &self.items[..self.len]
    }
}

impl DerefMut for Slots {
    fn deref_mut(&mut self) -> &mut [Slot] {
        &mut self.items[..self.len]
    }
}

impl Tree {
    /// The subtree fields of the tree, in the order of `Tree::fields`.
    pub fn slots(&self) -> Slots {
        use Slot::{List as L, Node as N, Opt as O};
        let s = Slots::of;
        match *self {
            Tree::Ident { name: _ }
            | Tree::Literal { value: _ }
            | Tree::ClassLiteral { tpe: _ }
            | Tree::TypeTree { inferred: _ } => s(&[]),
            Tree::Select { qualifier: a, name: _ }
            | Tree::This { qual: a }
            | Tree::New { tpt: a }
            | Tree::NamedArg { arg: a, name: _ }
            | Tree::Labeled { expr: a }
            | Tree::SingletonTypeTree { reference: a }
            | Tree::ByNameTypeTree { result: a }
            | Tree::Bind { body: a, name: _ }
            | Tree::TypeDef { rhs: a, name: _ } => s(&[N(a)]),
            Tree::Super { qual: a, mix: b }
            | Tree::Typed { expr: a, tpt: b }
            | Tree::Assign { lhs: a, rhs: b }
            | Tree::Return { expr: a, from: b }
            | Tree::Annotated { arg: a, annot: b }
            | Tree::WhileDo { cond: a, body: b } => s(&[N(a), N(b)]),
            Tree::Apply { fun: a, args: l }
            | Tree::TypeApply { fun: a, args: l }
            | Tree::AppliedTypeTree { tpt: a, args: l }
            | Tree::Import { expr: a, selectors: l }
            | Tree::PackageDef { pid: a, stats: l } => s(&[N(a), L(l)]),
            Tree::SeqLiteral { elems: l, elemtpt: a }
            | Tree::Block { stats: l, expr: a }
            | Tree::LambdaTypeTree { tparams: l, body: a } => s(&[L(l), N(a)]),
            Tree::Closure { env, meth, tpt } => s(&[L(env), N(meth), O(tpt)]),
            Tree::If { cond, thenp, elsep, inline: _ } => s(&[N(cond), N(thenp), N(elsep)]),
            Tree::CaseDef { pat, guard, body } => s(&[N(pat), O(guard), N(body)]),
            Tree::Try { expr, cases, finalizer } => s(&[N(expr), L(cases), O(finalizer)]),
            Tree::Match { selector, cases, kind: _ } => s(&[O(selector), L(cases)]),
            Tree::Alternative { trees } => s(&[L(trees)]),
            Tree::UnApply { fun, implicits, patterns } => s(&[N(fun), L(implicits), L(patterns)]),
            Tree::TypeBoundsTree { lo, hi, alias } => s(&[N(lo), N(hi), O(alias)]),
            Tree::ValDef { tpt, rhs, name: _ } => s(&[N(tpt), O(rhs)]),
            Tree::DefDef { paramss, tpt, rhs, name: _ } => s(&[Slot::Clauses(paramss), N(tpt), O(rhs)]),
            Tree::Template { constr, parents, self_, body } => s(&[N(constr), L(parents), O(self_), L(body)]),
            Tree::ImportSelector { imported, renamed, bound } => s(&[N(imported), O(renamed), O(bound)]),
            Tree::Inlined { call, bindings, expansion } => s(&[O(call), L(bindings), N(expansion)]),
        }
    }

    /// Sets the subtree fields of the tree to `slots`, which `slots` gave for a tree of its class.
    pub fn set_slots(&mut self, slots: &[Slot]) {
        let mut next = slots.iter().copied();
        self.fields(&mut |field| {
            let slot = match field {
                Field::Name(_) | Field::Constant(_) | Field::Class(_) => return,
                _ => next.next(),
            };
            match (field, slot) {
                (Field::Node(c), Some(Slot::Node(n))) => *c = n,
                (Field::Opt(c), Some(Slot::Opt(n))) => *c = n,
                (Field::List(c), Some(Slot::List(n))) => *c = n,
                (Field::Clauses(c), Some(Slot::Clauses(n))) => *c = n,
                _ => unreachable!("the slots of a tree of another class"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tpd::MatchKind;
    use syntax::ast::TName;
    use syntax::names::Names;

    /// The subtree fields of `tree` as `Tree::fields` hands them out.
    fn by_fields(mut tree: Tree) -> Vec<Slot> {
        let mut out = Vec::new();
        tree.fields(&mut |field| match field {
            Field::Node(c) => out.push(Slot::Node(*c)),
            Field::Opt(c) => out.push(Slot::Opt(*c)),
            Field::List(l) => out.push(Slot::List(*l)),
            Field::Clauses(c) => out.push(Slot::Clauses(*c)),
            Field::Name(_) | Field::Constant(_) | Field::Class(_) => {}
        });
        out
    }

    #[test]
    fn slots_are_the_subtree_fields_in_order() {
        let name = TName { name: Names::default().intern("x"), is_type: false };
        let [a, b, c, d] = [1, 2, 3, 4].map(TNodeId::new);
        let [l, m, k] = [1, 2, 3].map(|i| List::new(i, 1));
        let paramss = Clauses::new(5, 1);
        let trees = [
            Tree::Super { qual: a, mix: b },
            Tree::Apply { fun: a, args: l },
            Tree::TypeApply { fun: a, args: l },
            Tree::Typed { expr: a, tpt: b },
            Tree::NamedArg { name, arg: a },
            Tree::Closure { env: l, meth: a, tpt: Some(b) },
            Tree::SeqLiteral { elems: l, elemtpt: a },
            Tree::Assign { lhs: a, rhs: b },
            Tree::Block { stats: l, expr: a },
            Tree::If { cond: a, thenp: b, elsep: c, inline: false },
            Tree::CaseDef { pat: a, guard: Some(b), body: c },
            Tree::Return { expr: a, from: b },
            Tree::WhileDo { cond: a, body: b },
            Tree::Try { expr: a, cases: l, finalizer: Some(b) },
            Tree::Match { selector: Some(a), cases: l, kind: MatchKind::Plain },
            Tree::UnApply { fun: a, implicits: l, patterns: m },
            Tree::Annotated { arg: a, annot: b },
            Tree::AppliedTypeTree { tpt: a, args: l },
            Tree::TypeBoundsTree { lo: a, hi: b, alias: Some(c) },
            Tree::LambdaTypeTree { tparams: l, body: a },
            Tree::ValDef { name, tpt: a, rhs: Some(b) },
            Tree::DefDef { name, paramss, tpt: a, rhs: Some(b) },
            Tree::Template { constr: a, parents: l, self_: Some(d), body: m },
            Tree::Import { expr: a, selectors: k },
            Tree::ImportSelector { imported: a, renamed: Some(b), bound: Some(c) },
            Tree::PackageDef { pid: a, stats: l },
            Tree::Inlined { call: Some(a), bindings: l, expansion: b },
        ];
        for tree in trees {
            assert_eq!(*tree.slots(), by_fields(tree.clone())[..], "{tree:?}");
        }
    }
}
