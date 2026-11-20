// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ExpandPrivate.scala
//! The private underlying values of value classes (`class C(private val x: Int) extends AnyVal`).

use super::super::builtin::synthetic;
use super::super::encoded_names::decode;
use super::super::{Kind, SymbolId, Universe};
use super::{Unpickler, flags as f};
use crate::tags::PARAMsetter;
use names::name_kinds;

impl Unpickler {
    /// Enters the private getter of each value class's class parameter, once the classes'
    /// parents are read, with the expanded name `p$C$$x` of its class file as run-time name
    /// (dotty's `ExpandPrivate` of `isVCPrivateParamAccessor`); those of other classes stay
    /// skipped.
    pub fn private_values(&mut self, u: &mut Universe) -> Result<(), String> {
        for (i, class) in std::mem::take(&mut self.private_values) {
            if !is_value_class(u, class) || has_value(u, class) {
                continue;
            }
            let h = self.header(i)?;
            let name = decode(&h.name);
            let mut tags = f::visibility_tags(h.flags, h.within.is_some());
            tags.extend(f::member_tags(h.flags, true));
            let s = synthetic(u, class, &name, Kind::Val, &tags);
            let expanded = name_kinds::expanded(&u.expanded_prefix(class), &name);
            u.set_target(s, &expanded);
            self.enter(u, i, &h, s)?;
        }
        Ok(())
    }
}

/// Whether class `c` extends `AnyVal`.
fn is_value_class(u: &mut Universe, c: SymbolId) -> bool {
    let any_val = u.defn().any_val;
    any_val.is_some() && u.parents(c).into_iter().any(|p| u.class_symbol(p) == any_val)
}

/// Whether class `c` has a class parameter's getter entered.
fn has_value(u: &Universe, c: SymbolId) -> bool {
    u.symbol(c).decls.iter().any(|&d| u.symbol(d).kind == Kind::Val && u.symbol(d).flags.has(PARAMsetter))
}
