// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The types of `quotes.reflect`.

use super::Fallible;
use super::constants::encoded;
use crate::host::Host;
use tasty::tags::{CLASSconst, GIVEN, IMPLICIT, MUTABLE, OPAQUE};
use tasty::{Kind, NoVars, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree as T, constant_of};
use vm::Reflected;

/// How many aliases `dealias_keep_opaques` follows at most.
const DEALIAS_STEPS: u32 = 100;

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

    /// `t` dealiased up to a reference to an opaque type.
    pub fn dealias_keep_opaques(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..DEALIAS_STEPS {
            let reference = match *self.u.types().get(t) {
                Type::Applied(tycon, _) => tycon,
                _ => t,
            };
            if self.u.referenced(reference).is_some_and(|s| self.u.symbol(s).flags.has(OPAQUE)) {
                break;
            }
            match self.u.dealias_step(t) {
                next if next == t => break,
                next => t = next,
            }
        }
        t
    }

    pub fn is_subtype(&mut self, a: TypeId, b: TypeId) -> Fallible<bool> {
        let holds = self.u.is_subtype_in(a, b, &mut NoVars);
        self.decided(holds, [a, b])
    }

    pub fn is_same_type(&mut self, a: TypeId, b: TypeId) -> Fallible<bool> {
        let holds = self.u.is_same_type_in(a, b, &mut NoVars);
        self.decided(holds, [a, b])
    }

    /// `t` with each reference to a symbol of `pairs` but `this` replaced by its type.
    pub fn substitute_types(&mut self, t: TypeId, pairs: Vec<(SymbolId, TypeId)>) -> TypeId {
        self.u.map_type(t, &mut |u, part| {
            let s = match *u.types().get(part) {
                Type::Sym(_, s) => s,
                Type::TypeRef(..) | Type::TermRef(..) => u.resolve_ref(part)?,
                Type::This(_) => return Some(part),
                _ => return None,
            };
            pairs.iter().find(|&&(from, _)| from == s).map(|&(_, to)| to)
        })
    }

    /// Whether `t` is `PolyFunction { def apply(params): r }` with an erased parameter.
    pub fn is_erased_function_type(&mut self, t: TypeId) -> bool {
        self.u.poly_function_apply(t).is_some_and(|info| self.u.has_erased_params(info))
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

    /// `Refined(tpt, refinements, cls)`: `tpt`'s type refined by each type alias and value in
    /// turn (dotty's `assignType` of a `RefinedTypeTree`); refused for a refinement referring to
    /// `cls`'s `this`, one of a member class, or a value not narrowing the member it refines.
    pub fn refined(&mut self, tpt: TNodeId, cls: SymbolId, defs: Vec<TNodeId>) -> Fallible<TNodeId> {
        let refused = "Refined.apply of a refinement other than a type alias or a narrowing value";
        let parent = self.tpd.tpe(tpt);
        let parent_class = self.u.class_symbol(parent).filter(|&c| self.u.is_class(c));
        let (Some(parent_class), true) = (parent_class, self.u.is_class(cls)) else {
            return Err("Refined.apply of a type other than a class reference");
        };
        let mut tpe = parent;
        for &d in &defs {
            let s = self.tpd.sym(d).ok_or(refused)?;
            let (name, info) = (self.u.symbol(s).name.clone(), self.u.info(s));
            let existing = self.u.member(parent_class, &name);
            let flags = self.u.symbol(s).flags;
            let narrows = match (self.tpd.tree(d), self.u.symbol(s).kind) {
                (T::TypeDef { .. }, Kind::Type) => {
                    let lambda = match *self.u.types().get(info) {
                        Type::Alias(a) => matches!(self.u.types().get(a), Type::Lambda { .. }),
                        _ => true,
                    };
                    !lambda && existing.iter().all(|&m| self.u.symbol(m).kind != Kind::Class)
                }
                (T::ValDef { .. }, Kind::Val)
                    if ![MUTABLE, GIVEN, IMPLICIT].iter().any(|&f| flags.has(f)) =>
                {
                    existing.into_iter().all(|m| {
                        let member = self.u.member_info(parent, m);
                        self.u.symbol(m).kind == Kind::Val && self.u.is_subtype(info, member)
                    })
                }
                _ => false,
            };
            let mut this = false;
            self.u.map_type(info, &mut |u, part| {
                if let Type::This(c) = *u.types().get(part) {
                    this |= u.class_symbol(c) == Some(cls);
                }
                None
            });
            if !narrows || this {
                return Err(refused);
            }
            tpe = self.u.intern_type(Type::Refined(tpe, name, info));
        }
        let refinements = self.tpd.list(&defs);
        Ok(self.tpd.node(T::RefinedTypeTree { tpt, refinements }, self.expansion, Some(tpe), None))
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
