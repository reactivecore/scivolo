// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Polymorphic types instantiated with fresh types (dotty's `constrained`, `newTypeParams`).

use super::Typer;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The result of polymorphic type `info` of `owner`, its type parameters replaced by fresh
    /// abstract types within their bounds if `rigid`, else by fresh type variables within them;
    /// `info` itself if not polymorphic.
    pub fn poly_instance(&mut self, info: TypeId, owner: SymbolId, rigid: bool) -> TypeId {
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else { return info };
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        if let Some(args) = self.mapped_targs_of(owner, names.len()) {
            return self.instantiate_poly(result, binder, &names, &args, Some(owner));
        }
        let mut args = Vec::new();
        for n in &names {
            args.push(if rigid {
                let s = self.syms.define(self.u, n, Kind::TypeParam, owner, syntax::ast::flags::TYPE_PARAM);
                self.u.unenter(s);
                self.u.intern_type(Type::Sym(None, s))
            } else {
                self.constraint.fresh(self.u)
            });
        }
        for (&(_, bounds), &a) in params.iter().zip(&args) {
            let bounds = self.instantiate_poly(bounds, binder, &names, &args, Some(owner));
            if let Type::Sym(_, s) = *self.u.types().get(a) {
                self.u.set_info(s, bounds);
            } else if let Type::Bounds(lo, hi) = *self.u.types().get(bounds) {
                self.conforms(lo, a);
                self.conforms(a, hi);
            }
        }
        self.instantiate_poly(result, binder, &names, &args, Some(owner))
    }
}
