// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The types of `quotes.reflect`.

use super::Fallible;
use super::constants::encoded;
use crate::host::Host;
use tasty::tags::CLASSconst;
use tasty::{NoVars, SymbolId, Type, TypeId};
use typer::tpd::constant_of;
use vm::Reflected;

impl Host {
    pub fn type_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        self.reflect().type_symbol(t)
    }

    pub fn term_symbol(&mut self, t: TypeId) -> Fallible<Option<SymbolId>> {
        self.reflect().term_symbol(t)
    }

    pub fn type_args(&mut self, t: TypeId) -> Vec<TypeId> {
        self.reflect().type_args(t)
    }

    pub fn widen_term_ref_by_name(&mut self, t: TypeId) -> TypeId {
        self.reflect().widen_term_ref_by_name(t)
    }

    pub fn class_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        let t = self.u.widen(t);
        self.u.class_symbol(t).filter(|&c| self.u.is_class(c))
    }

    pub fn base_type(&mut self, t: TypeId, base: SymbolId) -> Fallible<TypeId> {
        self.u.base_type(t, base).ok_or("TypeRepr.baseType of a class the type does not derive from")
    }

    pub fn select_type(&mut self, t: TypeId, s: SymbolId) -> TypeId {
        self.reflect().select_type(t, s)
    }

    pub fn widen(&mut self, t: TypeId) -> TypeId {
        self.u.widen(t)
    }

    pub fn is_subtype(&mut self, a: TypeId, b: TypeId) -> Fallible<bool> {
        let holds = self.u.is_subtype_in(a, b, &mut NoVars);
        self.decided(holds, [a, b])
    }

    pub fn is_same_type(&mut self, a: TypeId, b: TypeId) -> Fallible<bool> {
        let holds = self.u.is_same_type_in(a, b, &mut NoVars);
        self.decided(holds, [a, b])
    }

    /// `t` with each reference to a symbol of `pairs` replaced by its type.
    pub fn substitute_types(&mut self, t: TypeId, pairs: Vec<(SymbolId, TypeId)>) -> TypeId {
        self.u.map_type(t, &mut |u, part| {
            let s = match *u.types().get(part) {
                Type::Sym(_, s) => s,
                Type::TypeRef(..) | Type::TermRef(..) => u.resolve_ref(part)?,
                _ => return None,
            };
            pairs.iter().find(|&&(from, _)| from == s).map(|&(_, to)| to)
        })
    }

    /// The constant of constant type `t`, nothing for another type.
    pub fn constant_type(&mut self, t: TypeId) -> Fallible<Reflected> {
        if matches!(*self.u.types().get(t), Type::Constant(tag, _) if tag == CLASSconst) {
            return Err("ConstantType of a class constant");
        }
        Ok(constant_of(&self.u, t).map_or((Vec::new(), None), encoded))
    }

    pub fn type_constructor_of(&mut self, name: String) -> Fallible<TypeId> {
        self.class_named(&name).ok_or("TypeRepr.typeConstructorOf of a class the frontend lacks")
    }

    /// `holds` of a type check of `types`; a `false` is refused if a type has a part the universe
    /// may not decide: a type variable, a wildcard, an unresolved reference, a match type, a
    /// lambda's type parameter or a type not represented.
    pub fn decided(&mut self, holds: bool, types: [TypeId; 2]) -> Fallible<bool> {
        let mut undecided = false;
        for t in types {
            self.u.map_type(t, &mut |u, part| {
                undecided |= match *u.types().get(part) {
                    Type::Var(_) | Type::Wildcard(_) | Type::Match { .. } | Type::Local(..) => true,
                    Type::Other(_) | Type::PackageName(_) => true,
                    Type::TypeRef(..) | Type::TermRef(..) => u.resolve_ref(part).is_none(),
                    _ => false,
                };
                None
            });
        }
        match !holds && undecided {
            true => Err("a type check the frontend cannot decide"),
            false => Ok(holds),
        }
    }

    /// The type of the class of Java name `name`, a primitive type for a primitive's name; `None`
    /// for nested classes.
    fn class_named(&mut self, name: &str) -> Option<TypeId> {
        let primitive = ["boolean", "byte", "short", "char", "int", "long", "float", "double", "void"];
        let full = match primitive.iter().position(|&p| p == name) {
            Some(i) => format!(
                "scala.{}",
                ["Boolean", "Byte", "Short", "Char", "Int", "Long", "Float", "Double", "Unit"][i]
            ),
            None if name.contains('$') => return None,
            None => name.to_owned(),
        };
        let t = self.u.class_ref(&full);
        matches!(self.u.types().get(t), Type::Sym(..)).then_some(t)
    }
}
