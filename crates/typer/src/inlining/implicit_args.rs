// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Implicit arguments of a macro-built reference to a method with a using clause, applied as
//! the retyper's `adapt` does (dotty's `adaptNoArgsImplicitMethod`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Binder, Type, TypeId};

impl Inlining<'_> {
    /// `t` applied to the implicit arguments its type's using clauses take, if it is not the
    /// function of an application.
    pub fn with_implicit_args(&mut self, mut t: TNodeId) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(t);
        loop {
            let Some(tpe) = self.tpd.get(t).tpe else { return Ok(t) };
            let widened = self.u.widen(tpe);
            let Type::Method { binder, params, result, implicit, contextual } =
                self.u.types().get(widened).clone()
            else {
                return Ok(t);
            };
            if !implicit && !contextual {
                return Ok(t);
            }
            if params.iter().any(|&(_, p)| self.refers_to(p, binder)) || self.refers_to(result, binder) {
                return Err((span, "implicit arguments of a dependent method type in a macro expansion"));
            }
            let mut args = Vec::new();
            for (_, p) in params {
                args.push(self.summon(p, span)?.ok_or((span, "no implicit argument"))?);
            }
            let args = self.tpd.list(&args);
            t = self.tpd.node(T::Apply { fun: t, args }, span, Some(result), None);
        }
    }

    fn refers_to(&mut self, t: TypeId, binder: Binder) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(*u.types().get(part), Type::Param(b, _) if b == binder);
            None
        });
        found
    }
}
