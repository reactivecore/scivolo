// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Signatures of class references and arrays (dotty's `classSig`, `arraySig`).

use super::applied::Applied;
use super::{Boxing, Sig};
use crate::codegen::descriptors::{internal, primitive_box};
use tasty::{Binder, Erased, Kind, ParamList, SymbolId, Type, TypeId};

impl Sig<'_> {
    /// A class, type parameter or type member reference, applied or not.
    pub fn reference(&mut self, t: TypeId, boxing: Boxing) {
        if let Some(p) = self.type_param(t) {
            let name = tasty::encode(&self.u.symbol(p).name);
            self.out.push_str(&format!("T{name};"));
            return;
        }
        let (tycon, args) = self.u.applied(t).map_or((t, Vec::new()), |(c, a)| (c, a.to_vec()));
        match self.applied_param(tycon, &args) {
            Some(Applied::Resolved(r)) => return self.jsig(r, false, boxing),
            Some(Applied::Param(name)) => return self.out.push_str(&format!("T{name};")),
            Some(Applied::Erased) => return self.erased(t, boxing),
            None => {}
        }
        let s = match *self.u.ty(tycon) {
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
        if boxing == Boxing::Unbox
            && let Some(value) = self.u.underlying(c)
        {
            return self.value_class(t, c, value);
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
        let prefix = match *self.u.ty(tycon) {
            Type::Sym(Some(p), _) | Type::TypeRef(p, _) => p,
            _ => return None,
        };
        let prefix = self.u.widen(prefix);
        let prefix = self.u.dealias(prefix);
        let parts = match *self.u.ty(prefix) {
            Type::And(a, b) => vec![a, b],
            _ => vec![prefix],
        };
        parts.into_iter().find_map(|p| {
            let (t, args) = self.u.applied(p).filter(|(_, args)| !args.is_empty())?;
            let args = args.to_vec();
            (self.u.class_symbol(t) == Some(outer)).then_some((outer, args))
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
        match *self.u.ty(a) {
            Type::Bounds(lo, hi) => {
                let java_top =
                    matches!(*self.u.ty(hi), Type::Sym(_, s) if s == self.u.defn().from_java_object);
                if !java_top && !self.class_named(hi, "scala.Any") {
                    self.out.push('+');
                    self.boxed(hi);
                } else if !self.class_named(lo, "scala.Nothing") {
                    self.out.push('-');
                    self.boxed(lo);
                } else {
                    self.out.push('*');
                }
            }
            Type::Lambda { params, result, .. } => self.lambda_arg(params, result),
            _ => self.boxed(a),
        }
    }

    /// A type lambda argument with parameters `params` and result `result`: an unbounded one
    /// applying a class as that class, applying another type as `*`, a bounded one as its
    /// constructor's erasure, any other as its result.
    fn lambda_arg(&mut self, params: ParamList, result: TypeId) {
        let Some((tycon, _)) = self.u.applied(result) else {
            return match self.u.erase(result) {
                Erased::Unit => self.out.push_str("Lscala/runtime/BoxedUnit;"),
                e @ Erased::Prim(_) => self.class_sig(primitive_box(&e).unwrap_or_default(), &[]),
                _ => self.jsig(result, false, Boxing::Unbox),
            };
        };
        let bounds: Vec<TypeId> = self.u.params_of(params).iter().map(|p| p.1).collect();
        let unbounded = bounds.into_iter().all(|b| {
            self.u.bounds(b).is_some_and(|(lo, hi)| {
                self.class_named(lo, "scala.Nothing") && self.class_named(hi, "scala.Any")
            })
        });
        match unbounded {
            true if self.class_ref(tycon).is_some() => self.jsig(tycon, false, Boxing::Unbox),
            true => self.out.push('*'),
            false => {
                let e = self.u.erase(tycon);
                self.erased_sig(&e, Boxing::Box);
            }
        }
    }

    /// Whether `t` refers to the class of full name `name`.
    pub fn class_named(&mut self, t: TypeId, name: &str) -> bool {
        self.class_ref(t).is_some_and(|c| self.u.full_name(c) == name)
    }

    /// The class `t` refers to after dealiasing, none for a type parameter or member.
    pub fn class_ref(&mut self, t: TypeId) -> Option<SymbolId> {
        let t = self.u.dealias(t);
        let s = match *self.u.ty(t) {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(..) => self.u.resolve_ref(t),
            _ => None,
        };
        s.filter(|&s| self.u.symbol(s).kind == Kind::Class)
    }

    /// The name and bounds of method type parameter `i` of `binder`, if in scope.
    pub fn poly_param(&self, binder: Binder, i: u32) -> Option<(String, TypeId)> {
        let (_, params) = self.poly.iter().find(|p| p.0 == binder)?;
        params.get(i as usize).cloned()
    }

    fn array(&mut self, elem: TypeId) {
        let param = self.u.param_ref(elem).and_then(|(b, i)| self.poly_param(b, i));
        let checked = param.map_or(elem, |(_, bounds)| bounds);
        if self.u.is_generic(checked) {
            return self.out.push_str("Ljava/lang/Object;");
        }
        self.out.push('[');
        let elem = self.u.dealias(elem);
        match self.u.bounds(elem) {
            Some((_, hi)) => {
                let hi = self.u.widen(hi);
                self.jsig(hi, false, Boxing::Unbox)
            }
            None => self.jsig(elem, false, Boxing::UnboxOnlyPrimitives),
        }
    }
}
