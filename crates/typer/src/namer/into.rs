// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! `Conversion.into[T]` in the types of parameters: `T @$into` in the parameter's own type,
//! `into[T]` again in its method's type (dotty's `suppressInto` and `revealInto`).

use crate::typer::Typer;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `t` with its covariant occurrences of `into[T]` as `T @$into`.
    pub fn suppress_into(&mut self, t: TypeId) -> TypeId {
        let Some((into, silent)) = self.into_symbols() else { return t };
        self.map_into(t, 1, &mut |u, t| match u.types().get(t).clone() {
            Type::Applied(c, args) if args.len() == 1 && u.referenced(c) == Some(into) => {
                let annot = crate::defn::type_ref(u, silent);
                Some(u.intern_type(Type::Annotated(args[0], annot)))
            }
            _ => None,
        })
    }

    /// `t` with its covariant occurrences of `T @$into` as `into[T]`.
    pub fn reveal_into(&mut self, t: TypeId) -> TypeId {
        let Some((into, silent)) = self.into_symbols() else { return t };
        let Some(module) = self.resolve_full_name("scala.Conversion", false) else { return t };
        let module = crate::defn::type_ref(self.u, module);
        let tycon = self.u.intern_type(Type::Sym(Some(module), into));
        self.map_into(t, 1, &mut |u, t| match u.types().get(t).clone() {
            Type::Annotated(arg, a) if u.class_symbol(a) == Some(silent) => {
                Some(u.intern_type(Type::Applied(tycon, vec![arg])))
            }
            _ => None,
        })
    }

    /// `scala.Conversion.into` and `scala.annotation.internal.$into`.
    fn into_symbols(&mut self) -> Option<(SymbolId, SymbolId)> {
        let into = self.resolve_full_name("scala.Conversion$.into", true)?;
        Some((into, self.resolve_full_name("scala.annotation.internal.$into", true)?))
    }

    /// `t` with `f` applied to its parts of non-negative variance, `t`'s being `v`.
    fn map_into(
        &mut self,
        t: TypeId,
        v: i8,
        f: &mut impl FnMut(&mut tasty::Universe, TypeId) -> Option<TypeId>,
    ) -> TypeId {
        if v < 0 {
            return t;
        }
        if let Some(mapped) = f(self.u, t) {
            return mapped;
        }
        match self.u.types().get(t).clone() {
            Type::ByName(r) => {
                let r = self.map_into(r, v, f);
                self.u.intern_type(Type::ByName(r))
            }
            Type::Annotated(r, a) => {
                let r = self.map_into(r, v, f);
                self.u.intern_type(Type::Annotated(r, a))
            }
            Type::Applied(c, args) => {
                let variances = self.tycon_variances(c);
                let args = (args.iter().enumerate())
                    .map(|(i, &a)| self.map_into(a, v * variances.get(i).copied().unwrap_or(0), f))
                    .collect();
                self.u.intern_type(Type::Applied(c, args))
            }
            _ => t,
        }
    }
}
