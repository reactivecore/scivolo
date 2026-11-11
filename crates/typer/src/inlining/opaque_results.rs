// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The type of an inline expansion with opaque proxies or of a method whose result refers to its
//! own opaque types, sealed by a cast (dotty's `InlineCall.expand`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::{OPAQUE, SYNTHETIC, TRANSPARENT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Inlining<'_> {
    /// `inlined`, the expansion of `call` with bindings `locals` and expansion type `expanded`,
    /// cast to the call's type if an opaque proxy or the call's own opaque types hide that it
    /// conforms; for a transparent method to the call's type intersected with the expansion's
    /// type, the proxies mapped back to their object (dotty's `unpackProxiesFromResultType`).
    pub fn sealed_opaques(
        &mut self,
        inlined: TNodeId,
        call: TNodeId,
        locals: &[SymbolId],
        expanded: TypeId,
    ) -> Result<TNodeId, Unsupported> {
        let proxied = locals.iter().copied().find_map(|s| {
            let mut module_ref = self.u.info(s);
            while let Type::Refined(parent, ..) = *self.u.ty(module_ref) {
                module_ref = parent;
            }
            let m = self.u.sym_of(module_ref)?;
            let class = match self.u.module_class(m) {
                Some(c) => c,
                None => {
                    let widened = self.u.widen(module_ref);
                    self.u.class_symbol(widened)?
                }
            };
            let refined = module_ref != self.u.info(s);
            (self.u.symbol(s).flags.has(SYNTHETIC) && refined && self.u.has_opaques(class))
                .then_some((s, module_ref))
        });
        let (proxies, module_ref) = match proxied {
            Some((opaque, module_ref)) => {
                let opaque_ref = self.u.intern_type(Type::Sym(None, opaque));
                let proxies = locals.iter().copied().filter(|&s| s == opaque || self.u.info(s) == opaque_ref);
                (proxies.collect(), module_ref)
            }
            None if self.result_has_own_opaques(call) => (Vec::new(), expanded),
            None => return Ok(inlined),
        };
        let call_tpe = self.tpd.tpe(call);
        let meth = self.tpd.sym(self.tpd.meth_part(call));
        let target = if meth.is_some_and(|m| self.u.symbol(m).flags.has(TRANSPARENT)) {
            let unpacked = self.u.map_type(expanded, &mut |u, t| match u.sym_ref(t) {
                Some((None, s)) if proxies.contains(&s) => Some(module_ref),
                _ => None,
            });
            self.u.glb(call_tpe, unpacked)
        } else {
            call_tpe
        };
        if self.u.is_subtype(self.tpd.tpe(inlined), target) {
            return Ok(inlined);
        }
        let unstable = match self.u.sym_of(target) {
            Some(s) => self.u.symbol(s).kind.is_term() && !self.pure().is_stable_member(s),
            None => false,
        };
        let to = if unstable { self.u.widen(target) } else { target };
        let cast = self.cast(inlined, to)?;
        if let T::TypeApply { args, .. } = *self.tpd.tree(cast)
            && let Some(&tpt) = self.tpd.items(args).first()
        {
            let end = self.tpd.span(inlined).end_pos();
            self.tpd.get_mut(tpt).span = end;
        }
        Ok(cast)
    }

    /// Whether the widened type of `call` refers to an opaque type of a class enclosing the
    /// called method (dotty's `hasOpaquesInResultFromCallWithTransparentContext`).
    fn result_has_own_opaques(&mut self, call: TNodeId) -> bool {
        let Some(meth) = self.tpd.sym(self.tpd.meth_part(call)) else { return false };
        let owners: Vec<SymbolId> = std::iter::successors(Some(meth), |&s| self.u.symbol(s).owner).collect();
        let call_tpe = self.tpd.tpe(call);
        let widened = self.u.widen(call_tpe);
        let mut found = false;
        self.u.map_type(widened, &mut |u, part| {
            if let Some(s) = u.sym_of(part) {
                let sym = u.symbol(s);
                found |= sym.kind == Kind::Type
                    && sym.flags.has(OPAQUE)
                    && sym.owner.is_some_and(|o| owners.contains(&o));
            }
            None
        });
        found
    }
}
