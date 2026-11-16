// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Calls of the signature polymorphic methods of `MethodHandle` and `VarHandle`, typed by the
//! types at the call (dotty's `simpleApply` on a signature polymorphic method).

use super::type_ops::TypeOps;
use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Tree;
use tasty::{ANY, Kind, NOTHING, Type, TypeId, Universe};

impl Typer<'_> {
    /// Retypes `f`, the method of an application to the arguments of `proto`, if it is
    /// signature polymorphic, as `mh.invokeExact(x, 1)`: as an instance of the method whose
    /// parameter types are the widened types of the arguments, typed without an expected type,
    /// and whose result type is the method's unless it is `Object`, else the expected type of
    /// the application, `Any` without one. Other methods are left.
    ///
    /// Refuses named and sequence arguments, an expected type whose wildcards `avoid_wildcards`
    /// refuses or whose type variables cannot be instantiated, and a call applied again or
    /// eta-expanded.
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

    /// Retypes call `t` as the qualifier of a selection, as `mh.invoke(x).eq(y)`: a result of
    /// `Any`, given for no expected type, becomes `Object`.
    pub fn signature_polymorphic_selected(&mut self, t: TNodeId) {
        let T::Apply { fun, .. } = *self.tpd.tree(t) else { return };
        let Some(instance) = self.tpd.sym(fun) else { return };
        let info = self.u.info(instance);
        let Some(m) = self.u.method(info) else { return };
        if m.result != ANY {
            return;
        }
        let params = m.params.to_vec();
        let object = self.u.intern_type(Type::Sym(None, self.u.defn().object));
        let binder = self.u.fresh_binder();
        let info = self.u.intern_method(binder, &params, object, false, false);
        self.u.set_info(instance, info);
        self.tpd.get_mut(t).tpe = Some(object);
    }

    /// The result type of an instance of signature polymorphic method `m` at a call expected to
    /// be of `pt`.
    fn signature_polymorphic_result(&mut self, m: tasty::SymbolId, pt: Pt) -> Result<TypeId, &'static str> {
        let info = self.u.info(m);
        let result = self.final_result(info);
        let result = self.strip_null(result);
        let object = self.u.defn().object;
        let dealiased = self.u.dealias(result);
        if self.u.applied(dealiased).is_some() || self.u.class_symbol(dealiased) != Some(object) {
            return Ok(result);
        }
        let expected = match pt {
            Pt::Any => return Ok(ANY),
            Pt::Type(expected) => self.types().avoid_wildcards(expected, 1)?,
            _ => return Err("signature polymorphic call applied or eta-expanded"),
        };
        self.fully_defined_as(expected, true)
    }

    /// Retypes call `t` as the qualifier of a cast whose expected type is `pt`, as
    /// `val s: String = mh.invoke(x).asInstanceOf[String]`: a result of `Any`, given for no
    /// expected type, becomes `pt` with its wildcards avoided, without instantiating its type
    /// variables (dotty's `SelectionProto(asInstanceOf, PolyProto(_, resTp))`).
    ///
    /// Refuses a cast that is applied or eta-expanded, or expected to be of a type that mentions
    /// type variables not yet inferred, as `id(mh.invoke(x).asInstanceOf[String])`.
    pub fn signature_polymorphic_cast(&mut self, t: TNodeId, pt: Pt) -> Result<(), &'static str> {
        let T::Apply { fun, .. } = *self.tpd.tree(t) else { return Ok(()) };
        let Some(instance) = self.tpd.sym(fun) else { return Ok(()) };
        let info = self.u.info(instance);
        let Some(m) = self.u.method(info) else { return Ok(()) };
        if m.result != ANY {
            return Ok(());
        }
        let params = m.params.to_vec();
        let result = match pt {
            Pt::Any => return Ok(()),
            Pt::PolyFun(i) => match self.poly_protos[i].pt {
                Pt::Any => return Ok(()),
                Pt::Type(expected) => self.types().avoid_wildcards(expected, 1)?,
                _ => return Err("cast of a signature polymorphic call applied or eta-expanded"),
            },
            _ => return Err("cast of a signature polymorphic call without type arguments"),
        };
        let result = self.zonk(result);
        if has_part(self.u, result, |u, part| u.var(part).is_some()) {
            return Err("cast of a signature polymorphic call expected to be of an inferred type");
        }
        let binder = self.u.fresh_binder();
        let info = self.u.intern_method(binder, &params, result, false, false);
        self.u.set_info(instance, info);
        self.tpd.get_mut(t).tpe = Some(result);
        Ok(())
    }
}

impl TypeOps<'_> {
    /// `t` at variance `v` with each wildcard replaced by its upper bound where it occurs
    /// covariantly, its lower bound contravariantly and a wildcard argument of both as an
    /// invariant type argument (dotty's `AvoidWildcardsMap`).
    ///
    /// Refuses a wildcard in a type other than an applied class type, an intersection or a union.
    pub fn avoid_wildcards(&mut self, t: TypeId, v: i8) -> Result<TypeId, &'static str> {
        if !self.has_wildcard(t) {
            return Ok(t);
        }
        Ok(match *self.u.ty(t) {
            Type::Wildcard(bounds) => {
                let (lo, hi) = bounds.and_then(|b| self.u.bounds(b)).unwrap_or((NOTHING, ANY));
                match v {
                    1 => self.avoid_wildcards(hi, 1)?,
                    -1 => self.avoid_wildcards(lo, -1)?,
                    _ => {
                        let (lo, hi) = (self.avoid_wildcards(lo, -1)?, self.avoid_wildcards(hi, 1)?);
                        self.u.intern_type(Type::Bounds(lo, hi))
                    }
                }
            }
            Type::Applied(tycon, args) => {
                let Some(c) = self.u.sym_of(tycon).filter(|&c| self.u.symbol(c).kind == Kind::Class) else {
                    return Err("signature polymorphic call expected to be of a type with a wildcard");
                };
                let variances = self.u.variances(c);
                let mut avoided = Vec::new();
                for (i, a) in self.u.types_of(args).to_vec().into_iter().enumerate() {
                    avoided.push(match variances.get(i).copied().unwrap_or(0) {
                        0 if self.has_wildcard(a) => {
                            let (lo, hi) = (self.avoid_wildcards(a, -1)?, self.avoid_wildcards(a, 1)?);
                            let lo = self.u.bounds(lo).map_or(lo, |(l, _)| l);
                            let hi = self.u.bounds(hi).map_or(hi, |(_, h)| h);
                            self.u.intern_type(Type::Bounds(lo, hi))
                        }
                        0 => a,
                        av => self.avoid_wildcards(a, av * v)?,
                    });
                }
                self.u.intern_applied(tycon, &avoided)
            }
            Type::And(a, b) => {
                let (a, b) = (self.avoid_wildcards(a, v)?, self.avoid_wildcards(b, v)?);
                self.u.intern_type(Type::And(a, b))
            }
            Type::Or(a, b, soft) => {
                let (a, b) = (self.avoid_wildcards(a, v)?, self.avoid_wildcards(b, v)?);
                self.u.intern_type(Type::Or(a, b, soft))
            }
            _ => return Err("signature polymorphic call expected to be of a type with a wildcard"),
        })
    }

    /// Whether `t` mentions a wildcard.
    fn has_wildcard(&mut self, t: TypeId) -> bool {
        has_part(self.u, t, |u, part| matches!(u.ty(part), Type::Wildcard(_)))
    }
}

/// Whether `t` has a part `is` holds of.
fn has_part(u: &mut Universe, t: TypeId, is: impl Fn(&Universe, TypeId) -> bool) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= is(u, part);
        None
    });
    found
}
