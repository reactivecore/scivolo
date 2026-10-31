// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! One inline call replaced by its expansion: the operations of `scala.compiletime` reduced, any
//! other call by an `Inlined` tree of its body (dotty's `Inlines.inlineCall`, `InlineCall.expand`).

use super::{Inlining, MAX_INLINES, Unsupported, expand};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::tags::{INLINE, TRANSPARENT};
use tasty::{Kind, SymbolId};

impl Inlining<'_> {
    /// Call `call`, owned by `owner`, of the inline method defined by `def`, expanded; what is
    /// outside the subset in another source reported at the call.
    pub fn inline_call(
        &mut self,
        call: TNodeId,
        def: TNodeId,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(call);
        let mut lifted = Vec::new();
        self.lift_bindings(call, &mut lifted, None);
        let expanded = self
            .expand_call(call, def, owner)
            .map_err(|(at, what)| (if at.is_in_other_source() { span } else { at }, what))?;
        if lifted.is_empty() {
            return Ok(expanded);
        }
        let (stats, tpe) = (self.tpd.list(&lifted), self.tpd.get(expanded).tpe);
        Ok(self.tpd.node(T::Block { stats, expr: expanded }, span, tpe, None))
    }

    /// Moves the statements of the blocks and the bindings of the inlined trees in the function
    /// part of `t` to `lifted`, each placed at `at` if given (dotty's `liftBindings`).
    fn lift_bindings(&mut self, t: TNodeId, lifted: &mut Vec<TNodeId>, at: Option<Span>) -> TNodeId {
        match self.tpd.tree(t).clone() {
            T::Block { stats, expr } => {
                for s in self.tpd.items(stats).to_vec() {
                    lifted.push(self.lifted_at(s, at));
                }
                self.lift_bindings(expr, lifted, at)
            }
            T::Inlined { call, bindings, expansion } => {
                for b in self.tpd.items(bindings).to_vec() {
                    lifted.push(self.lifted_at(b, at));
                }
                let inner = call.map(|c| self.tpd.span(c)).filter(|s| s.exists());
                let expansion = self.lift_bindings(expansion, lifted, inner);
                let bindings = self.tpd.list(&[]);
                self.tpd.get_mut(t).tree = T::Inlined { call, bindings, expansion };
                t
            }
            T::Apply { fun, .. } | T::TypeApply { fun, .. } => {
                let lifted_fun = self.lift_bindings(fun, lifted, at);
                if let T::Apply { fun, .. } | T::TypeApply { fun, .. } = &mut self.tpd.get_mut(t).tree {
                    *fun = lifted_fun;
                }
                t
            }
            T::Select { qualifier, name } => {
                let qualifier = self.lift_bindings(qualifier, lifted, at);
                self.tpd.get_mut(t).tree = T::Select { qualifier, name };
                t
            }
            _ => t,
        }
    }

    /// `t` and its subtrees placed at `at`, without the arguments inlined from outside (dotty's
    /// `liftFromInlined`).
    fn lifted_at(&mut self, t: TNodeId, at: Option<Span>) -> TNodeId {
        let Some(at) = at else { return t };
        if let T::Inlined { call: None, bindings, expansion } = *self.tpd.tree(t)
            && bindings.is_empty()
        {
            return expansion;
        }
        let node = self.tpd.get(t);
        let (tree, tpe, sym) = (node.tree.clone(), node.tpe, node.sym);
        let copy = self.tpd.node(tree, at, tpe, sym);
        crate::tpd::map_children(self, copy, &mut |i, c| i.lifted_at(c, Some(at)));
        copy
    }

    fn expand_call(&mut self, call: TNodeId, def: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(call);
        if self.depth >= MAX_INLINES {
            return Err((span, "maximal number of successive inlines exceeded"));
        }
        if self.depth == 0 {
            self.outermost_span = self.typer_inlined.unwrap_or(span);
            self.outer_implicits.clear();
            if !self.transparent_only {
                self.ctx = self.host.call_ctx(call);
                if self.ctx.is_none()
                    && let Some((ctx, around)) = self.typer_ctx
                {
                    self.ctx = Some(ctx);
                    let mut implicits = Vec::new();
                    if !self.implicit_defs_at(around, call, &mut implicits) {
                        self.implicit_defs(around, &mut implicits);
                    }
                    self.outer_implicits = implicits;
                }
            }
        }
        if self.depth > 0 && self.tpd.sym(def).is_some_and(|m| self.u.within_opaques(m)) {
            return Err((span, "inline call of an object with opaque types in an inline expansion"));
        }
        let op = self.compiletime_op(call);
        if let Some(reduced) = self.intrinsic(op.as_deref(), call, owner)? {
            return Ok(reduced);
        }
        self.depth += 1;
        let mut o = Some(owner);
        while let Some(x) = o.filter(|&x| self.u.symbol(x).kind != Kind::Class) {
            o = self.u.symbol(x).owner;
        }
        self.enclosing_class = o;
        if !self.transparent_only {
            self.type_this_qualifiers(call);
        }
        let outer_call = std::mem::replace(&mut self.call_span, span);
        let outer_implicits = self.local_implicits;
        self.local_implicits |= self.defines_implicit(def);
        let outer_macro = std::mem::replace(&mut self.in_macro_result, false);
        let expanded = expand::Expansion::new(self, call, owner).inlined(def);
        (self.call_span, self.local_implicits, self.in_macro_result) =
            (outer_call, outer_implicits, outer_macro);
        self.depth -= 1;
        let (bindings, expansion) = expanded?;
        if op.as_deref() == Some("error") {
            self.issue_error(call)?;
        }
        if let Some(inner) = self.inline_ref_in(expansion) {
            return Err((self.tpd.span(inner), "inline call in an inline expansion"));
        }
        let locals: Vec<SymbolId> = bindings.iter().filter_map(|&b| self.tpd.sym(b)).collect();
        let tpe = self.tpd.tpe(expansion);
        let transparent = self.tpd.sym(def).is_some_and(|m| self.u.symbol(m).flags.has(TRANSPARENT));
        let declared = Some(self.tpd.tpe(call))
            .filter(|&t| !transparent && !locals.iter().any(|&s| self.mentions(t, s)));
        let tpe = self
            .avoid(tpe, &locals)
            .or(declared)
            .ok_or((span, "inline expansion of a type referring to a binding"))?;
        let tpe = super::prefix_seen::class_params_seen(self.u, tpe);
        let expanded = self.tpd.tpe(expansion);
        let expansion = self.sealed(&locals, expansion, tpe)?;
        let bindings = self.tpd.list(&bindings);
        let inlined =
            self.tpd.node(T::Inlined { call: Some(call), bindings, expansion }, span, Some(tpe), None);
        self.sealed_opaques(inlined, call, &locals, expanded)
    }

    /// A reference to an inline method in `t`, outside the arguments inlined from outside.
    fn inline_ref_in(&self, t: TNodeId) -> Option<TNodeId> {
        match *self.tpd.tree(t) {
            T::Inlined { call: None, .. } => return None,
            T::Inlined { bindings, expansion, .. } => {
                let parts = [self.tpd.items(bindings), &[expansion]].concat();
                return parts.into_iter().find_map(|c| self.inline_ref_in(c));
            }
            T::Ident { .. } | T::Select { .. } => {
                let inline = self.tpd.sym(t).is_some_and(|s| {
                    let s = self.u.symbol(s);
                    s.kind == Kind::Def
                        && s.flags.has(INLINE)
                        && (!self.transparent_only || s.flags.has(TRANSPARENT))
                });
                if inline {
                    return Some(t);
                }
            }
            _ => {}
        }
        self.tpd.children(t).into_iter().find_map(|c| self.inline_ref_in(c))
    }
}
