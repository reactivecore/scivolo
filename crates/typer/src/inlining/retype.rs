// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The typing pass over a copied inline body (dotty's `InlineTyper`, a `ReTyper`): inline
//! arguments in place of their proxies, operations on constants folded into literals, `if`s on
//! constants and inline matches reduced, and the types of `if`s, blocks and inlined arguments
//! recomputed.

use super::copy::TreeTypeMap;
use super::{Inlining, Unsupported};
use crate::tpd::{MatchKind, TNodeId, Tree as T, map_children};
use rustc_hash::FxHashMap;
use syntax::ast::constant::Constant as C;
use tasty::{SymbolId, Type};

impl Inlining<'_> {
    /// `t`, owned by `owner`, retyped, the proxies of inline arguments in `args` replaced by copies
    /// of them and the inline calls in it expanded.
    pub fn retype(
        &mut self,
        t: TNodeId,
        args: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let t = self.retype_node(t, args, owner)?;
        if let Some(expanded) = self.expand_splice(t, owner)? {
            return Ok(expanded);
        }
        let Some(def) = self.inline_method(t)? else { return Ok(t) };
        self.inline_call(t, def, owner)
    }

    /// `t` retyped by its tree class, an `if` and an inline match as `InlineTyper.typedIf` and
    /// `typedMatch` do.
    fn retype_node(
        &mut self,
        t: TNodeId,
        args: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(t);
        match self.tpd.tree(t).clone() {
            T::Ident { .. } => match self.tpd.sym(t).and_then(|s| args.get(&s).map(|&r| (s, r))) {
                Some((proxy, rhs)) => {
                    let mut m = TreeTypeMap {
                        old_owner: Some(proxy),
                        new_owner: Some(owner),
                        ..TreeTypeMap::default()
                    };
                    self.copy(&mut m, rhs, &mut |_, _, _| None)
                }
                None => self.ident_as_select(t),
            },
            T::Literal { .. } | T::ClassLiteral { .. } | T::This { .. } => Ok(t),
            T::TypeTree { .. }
            | T::SingletonTypeTree { .. }
            | T::ByNameTypeTree { .. }
            | T::AppliedTypeTree { .. }
            | T::TypeBoundsTree { .. }
            | T::LambdaTypeTree { .. }
            | T::RefinedTypeTree { .. }
            | T::Annotated { .. } => Ok(t),
            T::If { cond, thenp, elsep, inline } => {
                let cond = self.retype_forced(inline, cond, args, owner)?;
                if let Some(C::Boolean(b)) = self.constant_of(cond) {
                    let selected = self.retype(if b { thenp } else { elsep }, args, owner)?;
                    if self.is_idempotent_expr(cond) {
                        return Ok(selected);
                    }
                    let span = self.envelope(&[cond], selected);
                    return self.block(&[cond], selected, span);
                }
                if inline {
                    return Err((span, "inline if whose condition is not a constant value"));
                }
                let (thenp, elsep) = (self.retype(thenp, args, owner)?, self.retype(elsep, args, owner)?);
                let tpe = self.u.lub(self.tpd.tpe(thenp), self.tpd.tpe(elsep));
                Ok(self.tpd.node(T::If { cond, thenp, elsep, inline }, span, Some(tpe), None))
            }
            T::Match { selector, cases, kind: MatchKind::Inline } if !self.is_inline_method(owner) => {
                let selector = selector.map(|s| self.retype_forced(true, s, args, owner)).transpose()?;
                self.reduce_inline_match(t, selector, cases, args, owner)
            }
            T::Select { qualifier, name } => {
                let before = self.tpd.get(qualifier).tpe;
                let qualifier = self.retype(qualifier, args, owner)?;
                if let T::Select { qualifier: q, .. } = &mut self.tpd.get_mut(t).tree {
                    *q = qualifier;
                }
                if self.reselects_override(t, qualifier, self.names.text(name.name).to_owned()) {
                    return Err((span, "inline expansion selecting an overriding member"));
                }
                if let Some(b) = before
                    && self.is_stable_ref(b)
                    && let Some(tpe) = self.tpd.get(t).tpe
                    && let Type::Sym(Some(_), s) = *self.u.types().get(tpe)
                {
                    self.tpd.get_mut(t).tpe = Some(self.u.intern_type(Type::Sym(Some(b), s)));
                }
                self.retyped(t, owner)
            }
            T::Apply { .. }
            | T::TypeApply { .. }
            | T::Typed { .. }
            | T::NamedArg { .. }
            | T::New { .. }
            | T::Block { .. }
            | T::ValDef { .. }
            | T::DefDef { .. }
            | T::Inlined { .. }
            | T::Closure { .. }
            | T::SeqLiteral { .. }
            | T::WhileDo { .. }
            | T::Assign { .. }
            | T::Match { .. }
            | T::CaseDef { .. }
            | T::Try { .. }
            | T::Bind { .. }
            | T::Alternative { .. }
            | T::UnApply { .. } => {
                let (inner, call) = match *self.tpd.tree(t) {
                    T::ValDef { .. } | T::DefDef { .. } => (self.tpd.sym(t).unwrap_or(owner), None),
                    T::Inlined { call, .. } => (owner, call),
                    _ => (owner, None),
                };
                let mut failed = None;
                map_children(self, t, &mut |i, c| match Some(c) == call {
                    true => c,
                    false => i.retype(c, args, inner).unwrap_or_else(|e| {
                        failed.get_or_insert(e);
                        c
                    }),
                });
                failed.map_or(Ok(()), Err)?;
                self.retyped(t, owner)
            }
            T::Super { .. } => Err((span, "super in an inline body")),
            _ => Err((span, "inline body with a return or label")),
        }
    }

    /// `t` retyped, with every inline call in it expanded if `force`.
    fn retype_forced(
        &mut self,
        force: bool,
        t: TNodeId,
        args: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let outer = self.force_inline;
        self.force_inline |= force;
        let t = self.retype(t, args, owner);
        self.force_inline = outer;
        t
    }

    /// Node `t`, whose children are retyped, retyped itself.
    fn retyped(&mut self, t: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let applied = matches!(self.tpd.tree(t), T::Apply { .. });
        let t = self.beta_reduce(t, owner)?;
        match *self.tpd.tree(t) {
            T::Select { .. } | T::Apply { .. } => {
                self.fold_constants(t);
                Ok(self.const_to_literal(t))
            }
            _ if applied => Ok(self.const_to_literal(t)),
            T::Block { stats, expr } => {
                let locals: Vec<SymbolId> =
                    self.tpd.items(stats).iter().filter_map(|&s| self.tpd.sym(s)).collect();
                let tpe = self.tpd.tpe(expr);
                if !locals.iter().any(|&l| self.mentions(tpe, l)) {
                    self.tpd.get_mut(t).tpe = Some(tpe);
                } else if let Some(avoided) = self.avoid(tpe, &locals) {
                    let span = self.tpd.span(expr);
                    let tpt =
                        self.tpd.node(T::TypeTree { inferred: true }, span.end_pos(), Some(avoided), None);
                    let expr =
                        self.tpd.node(T::Typed { expr, tpt }, span.to_synthetic(), Some(avoided), None);
                    self.tpd.get_mut(t).tree = T::Block { stats, expr };
                    self.tpd.get_mut(t).tpe = Some(avoided);
                }
                Ok(t)
            }
            T::Inlined { expansion, .. } => {
                self.tpd.get_mut(t).tpe = Some(self.tpd.tpe(expansion));
                Ok(t)
            }
            T::Match { cases: parts, .. } | T::Alternative { trees: parts } => {
                let parts = self.tpd.items(parts).to_vec();
                self.tpd.get_mut(t).tpe = Some(crate::tpd::lub_of(self.tpd, self.u, &parts));
                Ok(t)
            }
            T::Try { expr, cases, .. } => {
                let parts = [&[expr][..], self.tpd.items(cases)].concat();
                self.tpd.get_mut(t).tpe = Some(crate::tpd::lub_of(self.tpd, self.u, &parts));
                Ok(t)
            }
            _ => Ok(t),
        }
    }
}
