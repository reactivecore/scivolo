// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! Dropping the bindings of an inline expansion that nothing uses, and inlining the elideable
//! methods used once (dotty's `Inliner.dropUnusedDefs`, without type bindings, and
//! `isElideableExpr`).

use super::copy::TreeTypeMap;
use super::purity::PURE_PATH;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, map_children};
use rustc_hash::FxHashMap;
use tasty::tags::{CASE, INLINE, MUTABLE, SYNTHETIC, TRAIT};
use tasty::{Kind, SymbolId, Type};

impl Inlining<'_> {
    /// `bindings` without the unused elideable ones and the inline ones, and `expansion`, owned by
    /// `owner`, with the elideable methods used once inlined.
    pub fn drop_unused(
        &mut self,
        bindings: Vec<TNodeId>,
        expansion: TNodeId,
        owner: SymbolId,
    ) -> Result<(Vec<TNodeId>, TNodeId), Unsupported> {
        let mut count: FxHashMap<SymbolId, u32> = FxHashMap::default();
        let mut rhs_of: FxHashMap<SymbolId, TNodeId> = FxHashMap::default();
        for &b in &bindings {
            let Some(s) = self.tpd.sym(b) else { continue };
            let elideable = match *self.tpd.tree(b) {
                T::DefDef { paramss, rhs: Some(r), .. } if paramss.is_empty() => {
                    self.is_elideable(r).then_some(r)
                }
                T::ValDef { rhs: Some(r), .. } => self.is_elideable(r).then_some(r),
                _ => None,
            };
            if let Some(r) = elideable {
                count.insert(s, 0);
                rhs_of.insert(s, r);
            }
        }
        self.count_refs(expansion, &mut count);
        for &b in &bindings {
            self.count_refs(b, &mut count);
        }
        let retain = |i: &Self, s: SymbolId| {
            let kept = match count.get(&s) {
                Some(&x) => x > 1 || x == 1 && i.u.symbol(s).kind != Kind::Def,
                None => true,
            };
            kept && !i.u.symbol(s).flags.has(INLINE)
        };
        let retained: Vec<TNodeId> =
            bindings.iter().copied().filter(|&b| self.tpd.sym(b).is_none_or(|s| retain(self, s))).collect();
        if retained.len() == bindings.len() {
            return Ok((bindings, expansion));
        }
        let once: FxHashMap<SymbolId, TNodeId> =
            rhs_of.into_iter().filter(|(s, _)| count.get(s) == Some(&1)).collect();
        let expansion = self.inline_once(expansion, &once, owner)?;
        self.drop_unused(retained, expansion, owner)
    }

    /// Adds to `count` the references in `t`: one for each reference tree, two for each term
    /// referred to in a type.
    fn count_refs(&mut self, t: TNodeId, count: &mut FxHashMap<SymbolId, u32>) {
        let refs = matches!(self.tpd.tree(t), T::Ident { .. } | T::Select { .. });
        if refs && let Some(s) = self.tpd.sym(t) {
            count.entry(s).and_modify(|x| *x += 1);
        }
        if (refs || matches!(self.tpd.tree(t), T::New { .. } | T::TypeTree { .. }))
            && let Some(tpe) = self.tpd.get(t).tpe
        {
            self.u.map_type(tpe, &mut |u, part| {
                if let Type::Sym(_, s) = *u.types().get(part)
                    && u.symbol(s).kind.is_term()
                {
                    count.entry(s).and_modify(|x| *x += 2);
                }
                None
            });
        }
        for c in self.tpd.children(t) {
            self.count_refs(c, count);
        }
    }

    /// `t`, owned by `owner`, with each reference to a binding of `once` replaced by a copy of its
    /// right-hand side and blocks without statements by their expressions.
    fn inline_once(
        &mut self,
        t: TNodeId,
        once: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let t = match *self.tpd.tree(t) {
            T::Ident { .. } | T::Select { .. } => match self
                .tpd
                .sym(t)
                .and_then(|s| once.get(&s).map(|&r| (s, r)))
            {
                Some((s, rhs)) => {
                    let mut m =
                        TreeTypeMap { old_owner: Some(s), new_owner: Some(owner), ..TreeTypeMap::default() };
                    self.copy(&mut m, rhs, &mut |_, _, _| None)?
                }
                None => t,
            },
            T::Block { stats, expr } if stats.is_empty() => expr,
            _ => t,
        };
        let mut failed = None;
        let changed = map_children(self, t, &mut |i, c| match i.inline_once(c, once, owner) {
            Ok(n) => n,
            Err(e) => {
                failed.get_or_insert(e);
                c
            }
        });
        if let Some(e) = failed {
            return Err(e);
        }
        if changed && matches!(self.tpd.tree(t), T::Apply { .. }) {
            self.beta_reduce(t, owner)
        } else {
            Ok(t)
        }
    }

    /// Whether `t` can be dropped or duplicated by the inliner (dotty's `isElideableExpr`).
    pub fn is_elideable(&mut self, t: TNodeId) -> bool {
        match self.tpd.tree(t).clone() {
            T::This { .. } | T::Super { .. } | T::Literal { .. } | T::New { .. } | T::Closure { .. } => true,
            T::Ident { .. } => {
                let inline_param = self.tpd.sym(t).is_some_and(|s| {
                    let s = self.u.symbol(s);
                    s.kind == Kind::Param && s.flags.has(INLINE)
                });
                self.ref_purity(t) == PURE_PATH || inline_param
            }
            T::Select { qualifier, .. } => self.ref_purity(t) == PURE_PATH && self.is_elideable(qualifier),
            T::TypeApply { fun, .. } => self.is_elideable(fun),
            T::Apply { fun, args } => {
                let callee = self.method_part(fun).and_then(|f| self.tpd.sym(f));
                let args = self.tpd.items(args).to_vec();
                if self.is_pure_apply(t, callee) {
                    self.is_elideable(fun) && args.into_iter().all(|a| self.is_elideable(a))
                } else {
                    self.is_case_class_apply(t, fun) && args.into_iter().all(|a| self.is_elideable(a))
                }
            }
            T::Typed { expr, .. } | T::NamedArg { arg: expr, .. } => self.is_elideable(expr),
            T::Block { stats: defs, expr } | T::Inlined { bindings: defs, expansion: expr, .. } => {
                let defs = self.tpd.items(defs).to_vec();
                self.is_elideable(expr) && defs.into_iter().all(|d| self.is_stat_elideable(d))
            }
            _ => false,
        }
    }

    /// Whether `t` applies `fun`, the synthetic `apply` of a case class without initialization
    /// code, through its synthetic companion.
    fn is_case_class_apply(&mut self, t: TNodeId, fun: TNodeId) -> bool {
        let Some(f) = self.method_part(fun) else { return false };
        let Some(meth) = self.tpd.sym(f) else { return false };
        let s = self.u.symbol(meth);
        if s.name != "apply" || !s.flags.has(SYNTHETIC) {
            return false;
        }
        let owner = s.owner;
        let case_companion =
            owner.and_then(|o| self.u.companion_class(o)).is_some_and(|c| self.u.symbol(c).flags.has(CASE));
        let tpe = self.tpd.tpe(t);
        let real = self
            .u
            .class_symbol(tpe)
            .is_some_and(|c| !self.u.symbol(c).flags.has(TRAIT) && self.is_no_inits_class(c));
        let through = match *self.tpd.tree(f) {
            T::Select { qualifier, .. } => self.tpd.sym(qualifier),
            _ => Some(meth),
        };
        let synthetic = through.is_some_and(|q| self.u.symbol(q).flags.has(SYNTHETIC));
        case_companion && real && synthetic
    }

    fn is_stat_elideable(&mut self, s: TNodeId) -> bool {
        match *self.tpd.tree(s) {
            T::TypeDef { .. } | T::Import { .. } | T::DefDef { .. } => true,
            T::ValDef { rhs, .. } => {
                let mutable = self.tpd.sym(s).is_some_and(|v| self.u.symbol(v).flags.has(MUTABLE));
                !mutable && rhs.is_none_or(|r| self.is_elideable(r))
            }
            _ => false,
        }
    }
}
