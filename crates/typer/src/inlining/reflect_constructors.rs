// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The constructors of the classes `Symbol.newClass` makes, of the type its `conMethodType`
//! gives.

use super::Reflect;
use crate::defn::type_ref;
use crate::tpd::TNodeId;
use tasty::tags::{LOCAL, PARAMsetter, PRIVATE};
use tasty::{Binder, Flags, Kind, SymbolId, Type, TypeId, Universe};

impl Reflect<'_> {
    /// Gives `class` a constructor of type `info`, its type parameters also the class's and its
    /// value parameters also fields; answers the constructor and the definitions of its
    /// parameters by clause.
    pub fn class_constructor(
        &mut self,
        class: SymbolId,
        info: TypeId,
    ) -> Result<(SymbolId, Vec<Vec<TNodeId>>), &'static str> {
        let span = self.span;
        let ctor = self.constructor(class)?;
        let (mut t, mut clauses, mut binder) = (info, Vec::new(), None);
        let (mut class_refs, mut ctor_refs) = (Vec::new(), Vec::new());
        if let Some((b, params, result)) = self.u.poly(info).map(|p| (p.binder, p.params.to_vec(), p.result))
        {
            let mut defs = Vec::new();
            for (name, bounds) in params {
                if refers_to(self.u, bounds, b) {
                    return Err("Symbol.newClass with type parameter bounds referring to type parameters");
                }
                let p =
                    self.define(&name, Kind::TypeParam, class, Flags::default().with(PRIVATE).with(LOCAL));
                self.u.set_info(p, bounds);
                self.u.enter(p);
                class_refs.push(type_ref(self.u, p));
                let q = self.define(&name, Kind::TypeParam, ctor, Flags::default());
                self.u.set_info(q, bounds);
                ctor_refs.push(self.u.intern_type(Type::Sym(None, q)));
                defs.push(self.type_param_def(q));
            }
            clauses.push(defs);
            (t, binder) = (result, Some(b));
        }
        while let Some((params, result)) = self.u.method(t).map(|m| (m.params.to_vec(), m.result)) {
            let mut defs = Vec::new();
            for (name, tpe) in params {
                let field = self.define(&name, Kind::Val, class, Flags::default().with(PARAMsetter));
                let field_type = substituted(self.u, tpe, binder, &class_refs);
                self.u.set_info(field, field_type);
                self.u.enter(field);
                let p = self.define(&name, Kind::Param, ctor, Flags::default());
                let param_type = substituted(self.u, tpe, binder, &ctor_refs);
                self.u.set_info(p, param_type);
                let tpt = self.type_tree(param_type);
                defs.push(self.build()?.val_def(p, tpt, None, span));
            }
            clauses.push(defs);
            t = result;
        }
        let value_clauses = clauses.len() - usize::from(binder.is_some());
        if value_clauses == 0 || !self.is_class_result(t, class, binder, class_refs.len()) {
            return Err("Symbol.newClass with a constructor type other than a method type of the class");
        }
        self.u.set_info(ctor, info);
        Ok((ctor, clauses))
    }

    /// Whether `t` is the type of `class`, applied to the `n` parameters of `binder` if it has any.
    fn is_class_result(&mut self, t: TypeId, class: SymbolId, binder: Option<Binder>, n: usize) -> bool {
        let (tycon, args) = self.u.applied(t).map_or((t, Vec::new()), |(c, a)| (c, a.to_vec()));
        let param = |i: usize| binder.map(|b| Type::Param(b, i as u32));
        let applied =
            args.len() == n && args.iter().enumerate().all(|(i, &a)| Some(self.u.ty(a)) == param(i).as_ref());
        applied && self.u.sym_of(tycon) == Some(class)
    }
}

/// Whether `t` refers to a parameter of `binder`.
fn refers_to(u: &mut Universe, t: TypeId, binder: Binder) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, x| {
        found |= matches!(u.param_ref(x), Some((b, _)) if b == binder);
        None
    });
    found
}

/// `t` with the references to the parameters of `binder` replaced by `refs`.
fn substituted(u: &mut Universe, t: TypeId, binder: Option<Binder>, refs: &[TypeId]) -> TypeId {
    let Some(binder) = binder else { return t };
    u.map_type(t, &mut |u, x| match u.param_ref(x) {
        Some((b, i)) if b == binder => refs.get(i as usize).copied(),
        _ => None,
    })
}
