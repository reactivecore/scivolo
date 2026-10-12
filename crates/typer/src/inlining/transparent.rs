//! `Inlined` trees of transparent inline calls: expanded while typing, their calls reduced to a
//! trace after it (dotty's `Inlines.inlineCall` in the typer, `PostTyper`'s `inlineCallTrace`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::TName;
use syntax::names::{FreshNames, Names};
use tasty::{Kind, SymbolId, Universe};

/// The expansion of `call`, owned by `owner`, of a transparent inline method defined in `bodies`,
/// with the transparent calls in it expanded too; `bodies` holds the definition of every
/// transparent inline method it may reach, `with_inits` the classes of the unit that may run
/// code when an instance is made.
pub fn inline_transparent(
    (u, tpd): (&mut Universe, &mut Tpd),
    (names, fresh): (&mut Names, &mut FreshNames),
    (bodies, with_inits): (FxHashMap<SymbolId, TNodeId>, FxHashSet<SymbolId>),
    call: TNodeId,
    owner: SymbolId,
) -> Result<TNodeId, Unsupported> {
    let mut i = Inlining::new((u, tpd), (names, fresh), true);
    (i.bodies, i.with_inits) = (bodies, with_inits);
    match i.inline_method(call)? {
        Some(def) => i.inline_call(call, def, owner),
        None => Ok(call),
    }
}

impl Inlining<'_> {
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
        Some(trace)
    }
}
