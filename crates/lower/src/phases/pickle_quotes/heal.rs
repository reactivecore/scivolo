// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/staging/CrossStageSafety.scala
//   compiler/src/dotty/tools/dotc/staging/HealType.scala
//   compiler/src/dotty/tools/dotc/staging/QuoteTypeTags.scala
//! The types of a quote's body healed as dotty's `CrossStageSafety` heals them before the quote
//! is pickled: aliases dealiased, a type of level 0 made `t.Underlying` of its tag `t`.

use super::{SPLICE, staged};
use crate::Lower;
use crate::phases::literals::is_wildcard;
use crate::phases::walk::children;
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};
use typer::tpd::{TNodeId, Tree};

/// The name of the type member of `scala.quoted.Type` a tag's type is.
pub const UNDERLYING: &str = "Underlying";

/// The tags of a quote's types of level 0 and those the healed body uses.
struct Healer<'t> {
    tags: &'t [(SymbolId, TypeId)],
    /// The tags used, each with the tree order of its first use.
    used: FxHashMap<TypeId, usize>,
    at: usize,
}

/// Heals the trees of quote body `body` outside its splices, inner trees first: a type tree
/// whose type changes becomes a `TypeTree`, an applied type tree takes its parts' types, a
/// definition and a pattern variable get their symbol's info healed, a wildcard, an extractor
/// pattern and a splice their type, a splice's without the types of level 0; the other trees
/// keep theirs. Returns the tags `tags` of the types of level 0 the body uses, in the order
/// dotty records them.
pub fn heal(l: &mut Lower, body: TNodeId, tags: &[(SymbolId, TypeId)]) -> Vec<TypeId> {
    let (mut order, mut todo) = (Vec::new(), vec![body]);
    while let Some(t) = todo.pop() {
        order.push(t);
        if staged(l, t, SPLICE).is_none() {
            todo.extend(children(l.tpd, t).into_iter().rev());
        }
    }
    let mut h = Healer { tags, used: FxHashMap::default(), at: 0 };
    let mut changed = FxHashSet::default();
    for (at, &t) in order.iter().enumerate().rev() {
        h.at = at;
        if let Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::Bind { .. } = l.tpd.tree(t)
            && let Some(s) = l.tpd.sym(t)
        {
            let info = l.u.info(s);
            let healed = h.healed(l.u, info);
            l.u.set_info(s, healed);
            continue;
        }
        let Some(tpe) = l.tpd.get(t).tpe else { continue };
        let healed = match *l.tpd.tree(t) {
            _ if staged(l, t, SPLICE).is_some() => {
                let widened = l.u.widen(tpe);
                h.healed(l.u, widened);
                let healed = Healer { tags: &[], used: FxHashMap::default(), at }.healed(l.u, widened);
                avoided(l.u, healed)
            }
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::Bind { .. } => continue,
            Tree::Ident { .. } if is_wildcard(l, t) => h.healed(l.u, tpe),
            Tree::Ident { name } if !name.is_type => continue,
            Tree::Ident { .. } | Tree::SingletonTypeTree { .. } => h.retyped(l, t, tpe),
            Tree::Select { name, .. } if name.is_type => h.retyped(l, t, tpe),
            Tree::AppliedTypeTree { tpt, args } => {
                let parts = std::iter::once(tpt).chain(l.tpd.items(args).iter().copied());
                if !parts.clone().any(|p| changed.contains(&p)) {
                    continue;
                }
                let parts: Vec<TypeId> = parts.map(|p| l.tpd.tpe(p)).collect();
                let applied = l.u.intern_type(Type::Applied(parts[0], parts[1..].to_vec()));
                l.u.beta_reduced(applied)
            }
            Tree::TypeTree { .. } | Tree::UnApply { .. } => h.healed(l.u, tpe),
            _ => continue,
        };
        if healed != tpe {
            l.tpd.get_mut(t).tpe = Some(healed);
            changed.insert(t);
        }
    }
    let mut used: Vec<(TypeId, usize)> = h.used.into_iter().collect();
    used.sort_by_key(|&(_, at)| at);
    used.into_iter().map(|(tag, _)| tag).collect()
}

impl Healer<'_> {
    /// The healed type of type tree `t` of type `tpe`, `t` made a `TypeTree` if it changes.
    fn retyped(&mut self, l: &mut Lower, t: TNodeId, tpe: TypeId) -> TypeId {
        let healed = self.healed(l.u, tpe);
        if healed != tpe {
            let node = l.tpd.get_mut(t);
            (node.tree, node.sym) = (Tree::TypeTree { inferred: false }, None);
        }
        healed
    }

    /// `t` with the aliases among its parts dealiased and each type of level 0 with a tag `t`
    /// made `t.Underlying` (dotty's `HealType`).
    fn healed(&mut self, u: &mut Universe, t: TypeId) -> TypeId {
        u.map_type(t, &mut |u, x| match *u.types().get(x) {
            Type::Sym(None, s) if let Some(&(_, tag)) = self.tags.iter().find(|(t, _)| *t == s) => {
                let first = self.used.entry(tag).or_insert(self.at);
                *first = (*first).min(self.at);
                Some(u.intern_type(Type::TypeRef(tag, UNDERLYING.into())))
            }
            Type::Sym(..) | Type::TypeRef(..) => u.referenced_alias(x).map(|a| self.healed(u, a)),
            _ => None,
        })
    }
}

/// `t` with each abstract type local to a term made its upper bound (dotty's `AvoidMap` of
/// `getPicklableHoleType`).
pub fn avoided(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, x| {
        let Type::Sym(None, s) = *u.types().get(x) else { return None };
        let owner = u.symbol(s).owner?;
        if !matches!(u.symbol(s).kind, Kind::Type | Kind::TypeParam) || !u.symbol(owner).kind.is_term() {
            return None;
        }
        let info = u.info(s);
        match *u.types().get(info) {
            Type::Bounds(_, hi) => Some(avoided(u, hi)),
            _ => None,
        }
    })
}
