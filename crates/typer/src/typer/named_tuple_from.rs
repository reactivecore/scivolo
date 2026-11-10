// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeEval.scala
//! The reduction of `NamedTuple.From[C]` to the named tuple of case class `C`'s fields.

use super::Typer;
use tasty::{NameId, Type, TypeId};

impl Typer<'_> {
    /// `t` with each `NamedTuple.From[C]` of a case class `C` reduced to the named tuple of its
    /// fields (dotty's `From` reduction).
    pub fn with_case_fields(&mut self, mut t: TypeId) -> TypeId {
        let Some(from) = self.resolve_full_name("scala.NamedTuple$.From", true) else { return t };
        let mut applied = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Some((tycon, &[arg])) = u.applied(part) {
                applied.push((part, tycon, arg));
            }
            None
        });
        for (part, tycon, arg) in applied {
            if self.referenced(tycon) != Some(from) {
                continue;
            }
            let Some(fields) = self.case_fields_tuple(arg) else { continue };
            t = self.u.map_type(t, &mut |_, p| (p == part).then_some(fields));
        }
        t
    }

    /// The named tuple of the fields of case class type `c`; `c` widened if a named tuple.
    fn case_fields_tuple(&mut self, c: TypeId) -> Option<TypeId> {
        use tasty::tags::{CASE, CASEaccessor};
        if self.stripped_named_tuple(c).is_some() {
            return Some(self.u.widen(c));
        }
        let class = self.u.class_symbol(c).filter(|&k| self.u.symbol(k).flags.has(CASE))?;
        let decls = self.u.symbol(class).decls.clone();
        let fields: Vec<_> = decls
            .into_iter()
            .filter(|&d| {
                self.u.symbol(d).kind == tasty::Kind::Val && self.u.symbol(d).flags.has(CASEaccessor)
            })
            .collect();
        let names: Vec<NameId> = fields.iter().map(|&f| self.u.interned_name(f)).collect();
        let values: Vec<TypeId> = fields.iter().map(|&f| self.u.member_info(c, f)).collect();
        let names: Vec<TypeId> = names.into_iter().map(|n| self.u.intern_type(Type::Str(n))).collect();
        let nt = self.named_tuple_ref()?;
        let (names, values) = (self.nested_pairs(&names).ok()?, self.nested_pairs(&values).ok()?);
        Some(self.u.intern_applied(nt, &[names, values]))
    }
}
