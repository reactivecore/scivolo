// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The type arguments a constructor gets from the class type of its `new` or `this`.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The type arguments of `C[Ts]` if `f` selects the constructor of `new C[Ts]`, with the
    /// position after the `new` tree, or the class's own type parameters if `f` selects the
    /// constructor of `this` (dotty's `argTypesLo` of a constructor's qualifier).
    pub fn new_type_args(&mut self, f: TNodeId) -> Option<(Vec<TypeId>, syntax::ast::Span)> {
        let T::Select { qualifier, .. } = *self.tpd.tree(f) else { return None };
        let tpe = match self.tpd.tree(qualifier) {
            T::New { .. } => self.tpd.tpe(qualifier),
            T::This { .. } if self.tpd.sym(f).is_some_and(|s| self.u.is_constructor(s)) => {
                let this = self.tpd.tpe(qualifier);
                self.u.widen(this)
            }
            _ => return None,
        };
        let tpe = self.u.dealias(tpe);
        match self.u.types().get(tpe).clone() {
            Type::Applied(_, args) => {
                let args = args.into_iter().map(|a| self.lower_bound_of_arg(a)).collect();
                Some((args, self.tpd.span(qualifier).end_pos()))
            }
            _ => None,
        }
    }

    /// Type argument `a`, or its lower bound if it is a wildcard.
    fn lower_bound_of_arg(&mut self, a: TypeId) -> TypeId {
        let bounds = match *self.u.types().get(a) {
            Type::Wildcard(b) => b,
            Type::Bounds(..) => Some(a),
            _ => return a,
        };
        match bounds.map(|b| self.u.types().get(b).clone()) {
            Some(Type::Bounds(lo, _)) => lo,
            _ => crate::defn::type_ref(self.u, self.u.builtins().nothing),
        }
    }
}
