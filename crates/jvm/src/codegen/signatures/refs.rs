// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Signatures of class references, arrays and method types (dotty's `classSig`, `arraySig`,
//! `jsig` of `MethodOrPoly`).

use super::applied::Applied;
use super::{Boxing, Sig};
use crate::codegen::internal;
use tasty::{Binder, Erased, Kind, SymbolId, Type, TypeId};

impl Sig<'_> {
    /// A class, type parameter or type member reference, applied or not.
    pub fn reference(&mut self, t: TypeId, boxing: Boxing) {
        if let Some(p) = self.type_param(t) {
            let name = tasty::encode(&self.u.symbol(p).name);
            self.out.push_str(&format!("T{name};"));
            return;
        }
        let (tycon, args) = match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) => (tycon, args),
            _ => (t, Vec::new()),
        };
        match self.applied_param(tycon, &args) {
            Some(Applied::Resolved(r)) => return self.jsig(r, false, boxing),
            Some(Applied::Param(name)) => return self.out.push_str(&format!("T{name};")),
            Some(Applied::Erased) => return self.erased(t, boxing),
            None => {}
        }
        let s = match *self.u.types().get(tycon) {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(..) => self.u.resolve_ref(tycon),
            _ => None,
        };
        let Some(c) = s.filter(|&s| self.u.symbol(s).kind == Kind::Class) else {
            return self.erased(t, boxing);
        };
        let full = self.u.full_name(c);
        if full == "scala.Array" && args.len() == 1 {
            return self.array(args[0]);
        }
        let plain = self.u.intern_type(Type::Sym(None, c));
        let erased = self.u.erase(plain);
        let own = internal(&self.u.binary_name(c));
        match erased {
            Erased::Prim(_) | Erased::Unit | Erased::Nothing | Erased::Null => {
                self.erased_sig(&erased, boxing)
            }
            Erased::Class(ref e) if internal(e) != own => {
                let e = internal(e);
                let function = e.starts_with("scala/Function") && full.contains("Function");
                if function && !e.ends_with("XXL") {
                    self.class_sig(&e, &args)
                } else {
                    self.erased_sig(&erased, boxing)
                }
            }
            _ if boxing == Boxing::Unbox && self.u.underlying(c).is_some() => {
                let e = self.u.erase_value(t).full();
                self.erased_sig(&e, boxing);
            }
            _ if let Some((outer, outer_args)) = self.generic_outer(c, tycon) => {
                let outer = internal(&self.u.binary_name(outer));
                self.class_sig(&outer, &outer_args);
                self.out.pop();
                self.out.push('.');
                let name = tasty::encode(&self.u.symbol(c).name);
                self.class_sig_tail(&name, &args);
            }
            _ => self.class_sig(&own, &args),
        }
    }

    /// The class `c` is a member of and its type arguments, if the prefix of reference `tycon`
    /// applies it to some (dotty's `classSig` of a `pre` with arguments).
    fn generic_outer(&mut self, c: SymbolId, tycon: TypeId) -> Option<(SymbolId, Vec<TypeId>)> {
        let outer = self.u.symbol(c).owner.filter(|&o| self.u.is_class(o))?;
        let prefix = match *self.u.types().get(tycon) {
            Type::Sym(Some(p), _) | Type::TypeRef(p, _) => p,
            _ => return None,
        };
        let prefix = self.u.widen(prefix);
        let prefix = self.u.dealias(prefix);
        let parts = match *self.u.types().get(prefix) {
            Type::And(a, b) => vec![a, b],
            _ => vec![prefix],
        };
        parts.into_iter().find_map(|p| match self.u.types().get(p).clone() {
            Type::Applied(t, args) if !args.is_empty() && self.u.class_symbol(t) == Some(outer) => {
                Some((outer, args))
            }
            _ => None,
        })
    }

    pub fn class_sig(&mut self, name: &str, args: &[TypeId]) {
        self.out.push('L');
        self.class_sig_tail(name, args);
    }

    fn class_sig_tail(&mut self, name: &str, args: &[TypeId]) {
        self.out.push_str(name);
        if !args.is_empty() {
            self.out.push('<');
            for &a in args {
                self.arg(a);
            }
            self.out.push('>');
        }
        self.out.push(';');
    }

    fn arg(&mut self, a: TypeId) {
        let a = self.u.dealias(a);
        match *self.u.types().get(a) {
            Type::Bounds(lo, hi) => {
                if !self.class_named(hi, "scala.Any") {
                    self.out.push('+');
                    self.boxed(hi);
                } else if !self.class_named(lo, "scala.Nothing") {
                    self.out.push('-');
                    self.boxed(lo);
                } else {
                    self.out.push('*');
                }
            }
            Type::Lambda { .. } => self.out.push('*'),
            _ => self.boxed(a),
        }
    }

    /// The name and bounds of method type parameter `i` of `binder`, if in scope.
    pub fn poly_param(&self, binder: Binder, i: u32) -> Option<(String, TypeId)> {
        let (_, params) = self.poly.iter().find(|p| p.0 == binder)?;
        params.get(i as usize).cloned()
    }

    fn array(&mut self, elem: TypeId) {
        let checked = match *self.u.types().get(elem) {
            Type::Param(b, i) => self.poly_param(b, i).map_or(elem, |(_, bounds)| bounds),
            _ => elem,
        };
        if self.u.is_generic(checked) {
            return self.out.push_str("Ljava/lang/Object;");
        }
        self.out.push('[');
        let elem = self.u.dealias(elem);
        match *self.u.types().get(elem) {
            Type::Bounds(_, hi) => {
                let hi = self.u.widen(hi);
                self.jsig(hi, false, Boxing::Unbox)
            }
            _ => self.jsig(elem, false, Boxing::UnboxOnlyPrimitives),
        }
    }

    /// A method type: type parameters at the top, parameters, result.
    pub fn method(&mut self, mut t: TypeId, top: bool) {
        let mut tparams = Vec::new();
        let mut vparams = Vec::new();
        loop {
            match self.u.types().get(t).clone() {
                Type::Poly { binder, params, result } => {
                    self.poly.push((binder, params.iter().map(|(n, b)| (tasty::encode(n), *b)).collect()));
                    if top && !self.constructor {
                        tparams.extend(params.into_iter().map(|(n, b)| (tasty::encode(&n), b)));
                    }
                    t = result;
                }
                Type::Method { params, result, .. } => {
                    let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
                    let java_enum = self.constructor && names.ends_with(&["_$name", "_$ordinal"]);
                    let keep = params.len() - if java_enum { 2 } else { 0 };
                    let outer = usize::from(self.constructor && names.first() == Some(&"$outer"));
                    vparams.extend(params.into_iter().take(keep).skip(outer).map(|(_, p)| p));
                    t = self.context_result(result, &mut vparams);
                }
                _ => break,
            }
        }
        vparams.retain(|&p| !self.u.is_erased_param(p));
        self.type_params(&tparams);
        self.out.push('(');
        for p in vparams {
            self.jsig(p, false, Boxing::Unbox);
        }
        self.out.push(')');
        self.result(t);
    }

    /// The result type of context function type `t` with its parameter types added to `vparams`,
    /// else `t` (dotty's `collectMethodParams`).
    fn context_result(&mut self, t: TypeId, vparams: &mut Vec<TypeId>) -> TypeId {
        let d = self.u.dealias(t);
        let Type::Applied(tycon, mut args) = self.u.types().get(d).clone() else { return t };
        let name = self.u.class_symbol(tycon).map(|c| self.u.symbol(c).name.clone());
        let Some(arity) = name.as_deref().and_then(|n| n.strip_prefix("ContextFunction")) else { return t };
        if arity.is_empty() || !arity.bytes().all(|b| b.is_ascii_digit()) {
            return t;
        }
        let Some(result) = args.pop() else { return t };
        vparams.extend(args);
        result
    }
}
