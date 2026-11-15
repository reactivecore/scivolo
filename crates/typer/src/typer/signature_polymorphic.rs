// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Calls of the signature polymorphic methods of `MethodHandle` and `VarHandle`, typed by the
//! types at the call (dotty's `simpleApply` on a signature polymorphic method).

use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Tree;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Retypes `f`, the method of an application to the arguments of `proto`, if it is
    /// signature polymorphic, as `mh.invokeExact(x, 1)`: as an instance of the method whose
    /// parameter types are the widened types of the arguments, typed without an expected type,
    /// and whose result type is the method's unless it is `Object`, else the expected type of
    /// the application if fully defined, else `Object`. Other methods are left.
    ///
    /// Refuses named and sequence arguments, an expected type with a wildcard in it, and one
    /// whose type variables cannot be instantiated.
    pub fn signature_polymorphic_call(&mut self, f: TNodeId, proto: usize) -> Result<TNodeId, &'static str> {
        let Some(m) = self.tpd.sym(f).filter(|&m| self.u.is_signature_polymorphic(m)) else { return Ok(f) };
        let prefix = match *self.u.ty(self.tpd.tpe(f)) {
            Type::Sym(prefix, _) => prefix,
            Type::TermRef(prefix, _) => Some(prefix),
            _ => return Err("signature polymorphic method of an unexpected reference"),
        };

        let (args, ctx) = (self.protos[proto].args.clone(), self.protos[proto].ctx);
        let pretyped = self.protos[proto].typed.take().unwrap_or_default();
        let mut typed = Vec::new();
        let mut params = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            if self.is_sequence_arg(a) || matches!(self.ast.tree(a), Tree::NamedArg { .. }) {
                return Err("named or sequence argument of a signature polymorphic method");
            }
            let arg = match pretyped.get(i) {
                Some(&Some(arg)) => arg,
                _ => self.typed(a, Pt::Any, ctx),
            };
            let tpe = self.tpd.tpe(arg);
            let tpe = self.zonk(tpe);
            let tpe = self.u.widen(tpe);
            let tpe = self.u.by_name(tpe).unwrap_or(tpe);
            params.push((self.u.intern_name(&format!("x${i}")), tpe));
            typed.push(Some(arg));
        }
        self.protos[proto].typed = Some(typed);

        let result = self.signature_polymorphic_result(m, self.protos[proto].pt)?;
        let binder = self.u.fresh_binder();
        let info = self.u.intern_method(binder, &params, result, false, false);
        let instance = self.u.signature_polymorphic_instance(m, info);
        let tpe = self.ref_type(prefix, instance);
        let node = self.tpd.get_mut(f);
        (node.sym, node.tpe) = (Some(instance), Some(tpe));
        Ok(f)
    }

    /// Whether `t` is a call of an instance of a signature polymorphic method.
    pub fn is_signature_polymorphic_call(&self, t: TNodeId) -> bool {
        let T::Apply { fun, .. } = *self.tpd.tree(t) else { return false };
        self.tpd.sym(fun).is_some_and(|s| self.u.original_signature_polymorphic(s).is_some())
    }

    /// The result type of an instance of signature polymorphic method `m` at a call expected to
    /// be of `pt`.
    fn signature_polymorphic_result(&mut self, m: tasty::SymbolId, pt: Pt) -> Result<TypeId, &'static str> {
        let info = self.u.info(m);
        let result = self.final_result(info);
        let object = self.u.defn().object;
        let dealiased = self.u.dealias(result);
        if self.u.applied(dealiased).is_some() || self.u.class_symbol(dealiased) != Some(object) {
            return Ok(result);
        }
        let Pt::Type(expected) = pt else { return Ok(result) };
        if matches!(self.u.ty(expected), Type::Wildcard(_)) {
            return Ok(result);
        }
        let mut wildcard = false;
        self.u.map_type(expected, &mut |u, part| {
            wildcard |= matches!(u.ty(part), Type::Wildcard(_));
            None
        });
        if wildcard {
            return Err("signature polymorphic call expected to be of a type with a wildcard");
        }
        self.fully_defined_as(expected, true)
    }
}
