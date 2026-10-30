// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! `Term.etaExpand`: a method term as a closure, curried over its later clauses.

use super::Fallible;
use super::decode::Expr;
use crate::host::Host;
use tasty::{Binder, Kind, SymbolId, Type, TypeId};
use typer::tpd::TNodeId;

impl Host {
    /// The expression number of `e` eta-expanded into a closure owned by `owner`, `e`'s own if it
    /// is no method.
    pub fn eta_expand(&mut self, e: Expr, owner: SymbolId) -> Fallible<i32> {
        let owner = self.made_owner(owner);
        match self.eta(e.tree, owner)? {
            Some(closure) => Ok(self.add_tree(closure) as i32),
            None => Ok(e.number),
        }
    }

    /// The closure of method term `t` owned by `owner`, its result eta-expanded in turn; `None`
    /// if `t` is no method or its parameters depend on each other.
    fn eta(&mut self, t: TNodeId, owner: SymbolId) -> Fallible<Option<TNodeId>> {
        let info = self.tpd.get(t).tpe.ok_or("Term.etaExpand of a term without a type")?;
        let info = self.u.widen(info);
        let Type::Method { binder, params, result, .. } = self.u.types().get(info).clone() else {
            return Ok(None);
        };
        if params.iter().any(|&(_, p)| self.refers_to(p, binder)) {
            return Ok(None);
        }
        let result = match self.u.types().get(result) {
            Type::Method { .. } => self.function_type(result)?,
            _ => result,
        };
        let info =
            self.u.intern_type(Type::Method { binder, params, result, implicit: false, contextual: false });
        let synthetic = 1 << tasty::tags::SYNTHETIC;
        let meth = self.reflect().new_term((owner, "$anonfun", Kind::Def), info, synthetic)?;
        let params = self.method_params(meth)?;
        let mut refs = Vec::new();
        for p in params.iter().flatten() {
            let s = self.tpd.sym(*p).ok_or("a parameter without a symbol")?;
            refs.push(self.reflect().ref_(s)?);
        }
        let applied = self.reflect().apply(t, &refs)?;
        let body = self.eta(applied, meth)?.unwrap_or(applied);
        let def = self.reflect().def_def(meth, &params, Some(body))?;
        Ok(Some(self.reflect().closure(def)?))
    }

    /// The function type of method type `m`, curried for its later clauses (dotty's
    /// `toFunctionType`).
    fn function_type(&mut self, m: TypeId) -> Fallible<TypeId> {
        let Type::Method { binder, params, result, contextual, .. } = self.u.types().get(m).clone() else {
            return Ok(m);
        };
        if self.refers_to(result, binder) {
            return Err("Term.etaExpand of a method with a dependent later clause");
        }
        let result = self.function_type(result)?;
        let context = if contextual { "Context" } else { "" };
        let class = self.u.class_ref(&format!("scala.{context}Function{}", params.len()));
        let mut args: Vec<TypeId> =
            params.into_iter().map(|(_, p)| self.translated_from_repeated(p)).collect();
        args.push(result);
        Ok(self.u.intern_type(Type::Applied(class, args)))
    }

    /// Whether `t` refers to a parameter of method type binder `binder`.
    fn refers_to(&mut self, t: TypeId, binder: Binder) -> bool {
        let mut refers = false;
        self.u.map_type(t, &mut |u, part| {
            refers |= matches!(*u.types().get(part), Type::Param(b, _) if b == binder);
            None
        });
        refers
    }

    /// Repeated parameter type `p` as a `Seq` (dotty's `translateFromRepeated`), else `p`.
    fn translated_from_repeated(&mut self, p: TypeId) -> TypeId {
        let class_name = |h: &mut Host, t: TypeId| h.u.class_symbol(t).map(|c| h.u.full_name(c));
        match self.u.types().get(p).clone() {
            Type::Annotated(seq, annot)
                if class_name(self, annot).as_deref() == Some("scala.annotation.internal.Repeated") =>
            {
                seq
            }
            Type::Applied(c, args) if class_name(self, c).as_deref() == Some("scala.<repeated>") => {
                let seq = self.u.class_ref("scala.collection.immutable.Seq");
                self.u.intern_type(Type::Applied(seq, args))
            }
            _ => p,
        }
    }
}
