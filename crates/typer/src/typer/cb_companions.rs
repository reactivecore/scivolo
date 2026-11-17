// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Context bound companions (`experimental.modularity`): the name of a type with context bounds
//! as a term standing for their witnesses, and selections from it.

use super::find_ref::Name;
use super::implicits::candidates::Candidate;
use super::implicits::ranking::compare;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Level, LevelId};
use crate::tpd::TNodeId;
use syntax::ast::tree::ModsId;
use syntax::ast::{List, NodeId, TName, Tree};
use syntax::desugar::witness_names;
use tasty::{Kind, SymbolId, Type};

/// The refusal of a context bound companion used other than as a qualifier, and how a lookup
/// that finds one reports it.
pub const CB_COMPANION: &str = "context bound companion";

impl Typer<'_> {
    /// Records the witness names of type `sym` defined with modifiers `mods`, if it has any.
    pub fn record_witness_names(&mut self, sym: SymbolId, mods: ModsId) {
        let annots = self.ast.items(self.ast.modifiers(mods).annotations);
        if let Some(names) = annots.iter().find_map(|&a| witness_names(self.ast, self.names, a)) {
            self.syms.witnessed.insert(sym, names);
        }
    }

    /// Refuses term `name` at level `l` if it is a context bound companion there.
    pub fn check_no_cb_companion(&mut self, l: LevelId, name: Name, ctx: Ctx) -> Result<(), &'static str> {
        if !name.is_type && !self.cb_types_at(l, name.text, ctx).is_empty() {
            return Err(CB_COMPANION);
        }
        Ok(())
    }

    /// Determines the types with context bounds whose companions level `l` defines as term
    /// `name` (dotty's `addContextBoundCompanionFor`): a type parameter, or a type member of the
    /// class of a members level or of its base classes, in linearization order.
    pub fn cb_types_at(&mut self, l: LevelId, name: &str, ctx: Ctx) -> Vec<SymbolId> {
        if self.syms.witnessed.is_empty() {
            return Vec::new();
        }
        match *self.levels.get(l) {
            Level::Scope(_) => {
                let found = self.defined(l, Name { text: name, is_type: true }, ctx);
                found
                    .map_or_else(Vec::new, |f| f.syms)
                    .into_iter()
                    .filter(|s| {
                        self.syms.witnessed.contains_key(s) && self.u.symbol(*s).kind == Kind::TypeParam
                    })
                    .collect()
            }
            Level::Members(owner) if self.u.is_class(owner) => {
                let lin = self.u.linearization(owner);
                let mut types: Vec<SymbolId> = (self.syms.witnessed.keys().copied())
                    .filter(|&s| self.u.symbol(s).name == name)
                    .filter(|&s| self.u.symbol(s).owner.is_some_and(|o| lin.contains(&o)))
                    .collect();
                types.sort_by_key(|&s| lin.iter().position(|&c| Some(c) == self.u.symbol(s).owner));
                types
            }
            _ => Vec::new(),
        }
    }

    /// Types selection `id` of `name` from identifier `qualifier` if it names a context bound
    /// companion in `ctx`; `None` if it does not.
    pub fn cb_select(
        &mut self,
        id: NodeId,
        qualifier: NodeId,
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        if self.syms.witnessed.is_empty() {
            return None;
        }
        let Tree::Ident { name: companion } = *self.ast.tree(qualifier) else { return None };
        let text = self.text(companion.name).to_owned();
        if companion.is_type || !matches!(self.find_ref(&text, false, ctx), Err(CB_COMPANION)) {
            return None;
        }
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            let types = self.cb_types_at(l, &text, ctx);
            if !types.is_empty() {
                let witnesses = self.witness_refs(qualifier, &types, Ctx { level: l, ..ctx });
                return Some(self.typed_cb_select(id, &witnesses, name, pt, ctx));
            }
            level = self.levels.outer(l);
        }
        None
    }

    /// The references to the witnesses of `types` at `qualifier`, typed in `ctx`.
    fn witness_refs(&mut self, qualifier: NodeId, types: &[SymbolId], ctx: Ctx) -> Vec<TNodeId> {
        let mut names: Vec<String> = Vec::new();
        for s in types {
            for n in &self.syms.witnessed[s] {
                if !names.contains(n) {
                    names.push(n.clone());
                }
            }
        }
        let span = self.ast.span(qualifier);
        names
            .iter()
            .map(|n| {
                let name = TName { name: self.name(n), is_type: false };
                let ident = self.ast.node(Tree::Ident { name });
                self.ast.with_span(ident, span);
                self.typed(ident, Pt::Any, ctx)
            })
            .collect()
    }

    /// Selects `name` from the best of `witnesses` that have such a member (dotty's
    /// `typedCBSelect`): one whose type ranks above the others' as the more general given.
    /// Refuses a choice between witnesses of equal rank, which dotty makes by the selected
    /// members, and a selection no witness has as a member.
    fn typed_cb_select(
        &mut self,
        id: NodeId,
        witnesses: &[TNodeId],
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let text = self.text(name.name).to_owned();
        let mut best: Vec<TNodeId> = Vec::new();
        for &w in witnesses {
            let tpe = self.tpd.tpe(w);
            if self.members_found_in(tpe, &text, name.is_type, ctx).is_none() {
                continue;
            }
            let mut comparisons = Vec::new();
            for &prev in &best {
                match self.compare_witnesses(prev, w, ctx) {
                    Some(c) if c != 0 => comparisons.push(c),
                    _ => return self.unsupported(id, "context bound companion witnesses of equal rank"),
                }
            }
            if comparisons.contains(&1) {
                continue;
            }
            let kept = best.iter().zip(&comparisons).filter(|&(_, &c)| c != -1).map(|(&p, _)| p);
            best = std::iter::once(w).chain(kept).collect();
        }
        let &[chosen] = &best[..] else {
            return self.unsupported(id, "context bound companion selection no single witness has");
        };
        let qualifier = self.ast.node(Tree::TypeTree);
        self.ast.with_span(qualifier, self.tpd.span(chosen));
        self.splices.insert(qualifier, chosen);
        let select = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(select, self.ast.span(id));
        self.typed(select, pt, ctx)
    }

    /// Whether `qual` is a term parameter of a method with type parameters whose type refines
    /// type member `name` as an alias, as `tc.Self` in `def f[T: TC as tc](y: tc.Self)`, which
    /// dotty reduces to the alias in the parameter types of a later clause when the method is
    /// type-applied.
    pub fn is_aliased_param_member(&mut self, qual: TNodeId, name: &str) -> bool {
        let Type::Sym(_, param) = *self.u.ty(self.tpd.tpe(qual)) else { return false };
        let method = self.u.symbol(param).owner;
        let tree = method.and_then(|m| self.syms.def_trees.get(&m).copied());
        let Some(&Tree::DefDef { paramss, .. }) = tree.map(|t| self.ast.tree(t)) else { return false };
        let clauses = self.ast.clause_items(paramss);
        let first = |&c: &List| self.ast.items(c).first().map(|&p| self.ast.tree(p));
        if self.u.symbol(param).kind != Kind::Param
            || !clauses.iter().any(|c| matches!(first(c), Some(Tree::TypeDef { .. })))
        {
            return false;
        }
        let mut t = self.u.info(param);
        while let Type::Refined(parent, refined, info) = *self.u.ty(t) {
            if self.u.name(refined) == name && matches!(self.u.ty(info), Type::Alias(_)) {
                return true;
            }
            t = parent;
        }
        false
    }

    /// 1 if witness `a` ranks above witness `b` as the more general given, -1 if below, 0 if
    /// neither (dotty's `compare` with `preferGeneral`); `None` if undecided.
    fn compare_witnesses(&mut self, a: TNodeId, b: TNodeId, ctx: Ctx) -> Option<i32> {
        let candidate = |t: &Self, w: TNodeId| match *t.u.ty(t.tpd.tpe(w)) {
            Type::Sym(prefix, sym) => Some(Candidate { sym, prefix }),
            _ => None,
        };
        let (a, b) = (candidate(self, a)?, candidate(self, b)?);
        compare(self, a, b, ctx)
    }
}
