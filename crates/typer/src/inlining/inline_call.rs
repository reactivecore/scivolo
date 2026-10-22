// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! One inline call replaced by its expansion: the operations of `scala.compiletime` reduced, any
//! other call by an `Inlined` tree of its body (dotty's `Inlines.inlineCall`, `InlineCall.expand`).

use super::{Inlining, MAX_INLINES, Unsupported, expand};
use crate::tpd::{TNodeId, Tree as T};
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
        self.expand_call(call, def, owner)
            .map_err(|(at, what)| (if at.is_in_other_source() { span } else { at }, what))
    }

    fn expand_call(&mut self, call: TNodeId, def: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(call);
        if self.depth >= MAX_INLINES {
            return Err((span, "maximal number of successive inlines exceeded"));
        }
        if self.depth == 0 {
            self.outermost_span = self.typer_inlined.unwrap_or(span);
            if !self.transparent_only {
                self.ctx = self.host.call_ctx(call);
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
        let expanded = expand::Expansion::new(self, call, owner).inlined(def);
        (self.call_span, self.local_implicits) = (outer_call, outer_implicits);
        self.depth -= 1;
        let (bindings, expansion) = expanded?;
        if op.as_deref() == Some("error") {
            self.issue_error(call)?;
        }
        if let Some(inner) = self.inline_ref_in(expansion) {
            return Err((self.tpd.span(inner), "inline call in an inline expansion"));
        }
        if let Some(r) = bindings.iter().chain([&expansion]).find_map(|&b| self.erased_value_in(b)) {
            return Err((self.tpd.span(r), "erasedValue in an inline expansion"));
        }
        let locals: Vec<SymbolId> = bindings.iter().filter_map(|&b| self.tpd.sym(b)).collect();
        let tpe = self.tpd.tpe(expansion);
        let tpe =
            self.avoid(tpe, &locals).ok_or((span, "inline expansion of a type referring to a binding"))?;
        let expanded = self.tpd.tpe(expansion);
        let expansion = self.sealed(&locals, expansion, tpe)?;
        let bindings = self.tpd.list(&bindings);
        let inlined =
            self.tpd.node(T::Inlined { call: Some(call), bindings, expansion }, span, Some(tpe), None);
        self.sealed_opaques(inlined, call, &locals, expanded)
    }
}
