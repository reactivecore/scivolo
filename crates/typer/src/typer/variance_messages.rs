// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala
//! The messages of variance errors (dotty's `Traverser.checkVariance`).

use super::Typer;
use super::variance_types::{declared, label};
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

const ENUM_EXAMPLE: &str = "See an example at https://docs.scala-lang.org/scala3/reference/enums/adts.html#parameter-variance-of-enums";

impl Typer<'_> {
    /// The error for type parameter `tvar` occurring in the type of `sym` where variance
    /// `required` is; of a class, `unchecked` are its parents annotated `@uncheckedVariance`.
    pub fn variance_message(
        &mut self,
        sym: SymbolId,
        tvar: SymbolId,
        required: u8,
        unchecked: &[TypeId],
    ) -> String {
        let info = self.u.info(sym);
        let shown = match self.u.symbol(sym).kind {
            Kind::Class => {
                let parents: Vec<String> = (self.u.parents(sym).into_iter())
                    .map(|p| match unchecked.contains(&p) {
                        true => format!("{} @uncheckedVariance", self.u.show_user(p)),
                        false => self.u.show_user(p),
                    })
                    .collect();
                format!("{} {{...}}", parents.join(" with "))
            }
            Kind::Def if !matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) => {
                format!("=> {}", self.u.show_user(info))
            }
            _ => self.u.show_user(info),
        };
        let (own, at) = (label(declared(self.u, tvar)), label(required));
        let (tname, described) = (self.u.symbol(tvar).name.clone(), self.described(sym));
        let mut msg = format!("{own} type {tname} occurs in {at} position in type {shown} of {described}");
        let towner = self.u.symbol(tvar).owner;
        let enum_case = towner.filter(|&o| self.flags_of(o).is_all_of(f::ENUM_CASE) && self.u.is_class(o));
        if let Some(case) = enum_case
            && self.flags_of(tvar).is(f::SYNTHETIC)
        {
            let case = &self.u.symbol(case).name;
            msg.push_str(&format!(
                "\nenum case {case} requires explicit declaration of type {tname} to resolve this issue.\n{ENUM_EXAMPLE}"
            ));
        }
        let flags = self.flags_of(sym);
        if flags.is(f::PARAM_ACCESSOR) && flags.is(f::PRIVATE) {
            let (kw, fix) = match flags.is(f::MUTABLE) {
                true => ("var", " and add\na field inside the class instead"),
                false => ("val", ""),
            };
            let name = &self.u.symbol(sym).name;
            msg.push_str(&format!(
                "\n\nImplementation limitation: private {kw} parameters cannot be inferred to be local\nand therefore are always variance-checked.\n\nPotential fix: remove the private {kw} modifiers on the parameter {name}{fix}."
            ));
        }
        msg
    }

    /// `sym` as dotty's messages name it: its kind and name.
    fn described(&self, sym: SymbolId) -> String {
        let s = self.u.symbol(sym);
        let flags = self.flags_of(sym);
        let kind = match s.kind {
            Kind::Class if flags.is(f::TRAIT) => "trait",
            Kind::Class => "class",
            Kind::Type | Kind::TypeParam => "type",
            Kind::Def => "method",
            Kind::Param if s.owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Def) => "parameter",
            _ if flags.is(f::MUTABLE) => "variable",
            _ if flags.is(f::LAZY) => "lazy value",
            _ => "value",
        };
        format!("{kind} {}", s.name)
    }
}
