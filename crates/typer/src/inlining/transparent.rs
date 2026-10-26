// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//! `Inlined` trees of transparent inline calls: expanded while typing, their calls reduced to a
//! trace after it (dotty's `Inlines.inlineCall` in the typer, `PostTyper`'s `inlineCallTrace`).

use super::{Inlining, Unsupported};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::TName;
use tasty::{Kind, SymbolId};

impl Inlining<'_> {
    /// The expansion of `call` in `ctx` of a transparent inline method defined in `bodies`, with
    /// the transparent calls in it expanded too; `bodies` holds the definition of every
    /// transparent inline method it may reach, `with_inits` the classes of the unit that may run
    /// code when an instance is made; the method may be one that is not transparent if `force`.
    pub fn inline_transparent(
        &mut self,
        (bodies, with_inits): (FxHashMap<SymbolId, TNodeId>, FxHashSet<SymbolId>),
        (call, force): (TNodeId, bool),
        ctx: Ctx,
    ) -> Result<TNodeId, Unsupported> {
        (self.bodies, self.with_inits, self.ctx) = (bodies, with_inits, Some(ctx));
        self.force_inline = force;
        let def = self.inline_method(call);
        self.force_inline = false;
        match def? {
            Some(def) => self.inline_call(call, def, ctx.owner),
            None => Ok(call),
        }
    }

    /// The context of the call of `Inlined` tree `t` from the typer, if it recorded one, and `t`.
    pub fn typer_inlined_ctx(&self, t: TNodeId) -> Option<(Ctx, TNodeId)> {
        let T::Inlined { call: Some(call), .. } = *self.tpd.tree(t) else { return None };
        Some((self.host.call_ctx(call)?, t))
    }

    /// The call of `Inlined` tree `t`, made a reference to the top-level class of the method it
    /// calls unless it is one already, the `this` qualifiers in its bindings then typed.
    pub fn traced_call(&mut self, t: TNodeId) -> Option<TNodeId> {
        let T::Inlined { call: Some(call), bindings, expansion } = *self.tpd.tree(t) else { return None };
        let meth = self.tpd.sym(self.tpd.meth_part(call))?;
        if self.u.symbol(meth).kind == Kind::Class {
            return Some(call);
        }
        let mut cls = meth;
        while let Some(o) = self.u.symbol(cls).owner.filter(|&o| self.u.symbol(o).kind != Kind::Package) {
            cls = o;
        }
        let name = TName { name: self.names.intern(&self.u.symbol(cls).name.clone()), is_type: true };
        let tpe = crate::defn::type_ref(self.u, cls);
        let span = self.tpd.span(call);
        let trace = self.tpd.node(T::Ident { name }, span, Some(tpe), Some(cls));
        self.tpd.get_mut(t).tree = T::Inlined { call: Some(trace), bindings, expansion };
        for b in self.tpd.items(bindings).to_vec() {
            self.type_this_qualifiers(b);
        }
        self.type_this_qualifiers(expansion);
        Some(trace)
    }
}
