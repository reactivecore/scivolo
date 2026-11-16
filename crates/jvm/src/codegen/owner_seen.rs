// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The type of a member seen from its class, which its generic signature describes.

use super::Gen;
use super::descriptors::internal;
use names::name_kinds;
use tasty::{SymbolId, TypeId};

impl Gen<'_, '_> {
    /// The type of member `sym` seen from its class (dotty's `memberInfo` of the owner's
    /// `thisType`), of a mixin accessor or forwarder that of the trait member.
    pub fn seen_from_owner(&mut self, sym: SymbolId) -> TypeId {
        let owner = self.l.u.symbol(sym).owner.filter(|&o| self.l.u.is_class(o));
        let name = &self.l.u.symbol(sym).name;
        if name_kinds::is_outer(name) {
            let (_, result) = self.l.u.erased_method(sym);
            if let tasty::Erased::Class(outer) = result
                && let Some(Some(c)) = self.classes.get(&internal(&outer)).copied()
            {
                let info = self.l.u.info(sym);
                let declared = self.l.u.result_type(info);
                let tycon = self.l.u.applied(declared).map(|(t, _)| t);
                return match tycon.and_then(|t| self.l.u.class_symbol(t)) {
                    Some(d) if d == c => declared,
                    _ => self.l.u.this_type(c),
                };
            }
        }
        match owner {
            Some(o) => {
                let member = self.mixed_origin(sym).unwrap_or(sym);
                let this = match member == sym {
                    true => self.l.u.this_type(o),
                    false => {
                        let class = self.l.u.intern_type(tasty::Type::Sym(None, o));
                        self.l.u.intern_type(tasty::Type::This(class))
                    }
                };
                self.l.u.member_info(this, member)
            }
            None => self.l.u.info(sym),
        }
    }
}
