// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The typing pass over a copied inline body (dotty's `InlineTyper`, a `ReTyper`): inline
//! arguments in place of their proxies, operations on constants folded into literals, `if`s on
//! constants and inline matches reduced, the types of `if`s recomputed.

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
        self.retype_part(t, args, owner, true)
    }

    /// `retype`, a reference to or an application of a method whose next clause is a using clause
    /// applied to its implicit arguments if `adapt`.
    fn retype_part(
        &mut self,
        t: TNodeId,
        args: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
        adapt: bool,
    ) -> Result<TNodeId, Unsupported> {
        let t = self.retype_node(t, args, owner)?;
        if let Some(expanded) = self.expand_splice(t, owner)? {
            return Ok(expanded);
        }
        let adaptable = matches!(
            self.tpd.tree(t),
            T::Ident { .. } | T::Select { .. } | T::TypeApply { .. } | T::Apply { .. }
        );
        let t = if adapt && adaptable && self.in_macro_result { self.with_implicit_args(t)? } else { t };
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
                None if self.is_local_inline_val(t) => Ok(self.const_to_literal(t)),
                None => self.ident_as_select(t),
            },
            T::Literal { .. } | T::ClassLiteral { .. } | T::This { .. } | T::Super { .. } => Ok(t),
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
                    if self.pure().is_idempotent_expr(cond) {
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
                self.reselect(t, qualifier, self.names.text(name.name).to_owned())?;
                if let Some(prefix) = before.and_then(|b| self.local_prefix(b))
                    && let Some(tpe) = self.tpd.get(t).tpe
                    && let Type::Sym(Some(_), s) = *self.u.types().get(tpe)
                {
                    self.tpd.get_mut(t).tpe = Some(self.u.intern_type(Type::Sym(Some(prefix), s)));
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
            | T::UnApply { .. }
            | T::TypeDef { .. }
            | T::Template { .. } => {
                let call = if let T::Inlined { call, .. } = *self.tpd.tree(t) { call } else { None };
                let (inner, within) = self.owners_within(t, owner);
                let outer_class = std::mem::replace(&mut self.enclosing_class, within);
                let mut failed = None;
                let mark = self.enter_block_implicits(t);
                let unadapted = match *self.tpd.tree(t) {
                    T::Apply { fun, .. } | T::TypeApply { fun, .. } => Some(fun),
                    T::Closure { meth, .. } => Some(meth),
                    _ => None,
                };
                map_children(self, t, &mut |i, c| match Some(c) == call {
                    true => c,
                    false => i.retype_part(c, args, inner, Some(c) != unadapted).unwrap_or_else(|e| {
                        failed.get_or_insert(e);
                        c
                    }),
                });
                self.block_implicits.truncate(mark);
                self.enclosing_class = outer_class;
                failed.map_or(Ok(()), Err)?;
                self.retyped(t, owner)
            }
            T::Return { expr, from } => {
                let expr = self.retype(expr, args, owner)?;
                self.tpd.get_mut(t).tree = T::Return { expr, from };
                Ok(t)
            }
            _ => Err((span, "inline body with a label")),
        }
    }

    /// Whether `t` refers to an inline value local to a method.
    pub fn is_local_inline_val(&self, t: TNodeId) -> bool {
        self.tpd.sym(t).is_some_and(|s| {
            let sym = self.u.symbol(s);
            let local = sym.owner.is_some_and(|o| self.u.symbol(o).kind.is_term());
            sym.kind == tasty::Kind::Val && sym.flags.has(tasty::tags::INLINE) && local
        })
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
}
