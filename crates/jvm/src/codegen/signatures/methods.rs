// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Signatures of method types (dotty's `jsig` of `MethodOrPoly`).

use super::{Boxing, Sig};
use names::nme;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Sig<'_> {
    /// A method type: type parameters at the top, of an extension method then its value
    /// class's, parameters, result.
    pub fn method(&mut self, mut t: TypeId, top: bool) {
        let mut tparams = Vec::new();
        let mut vparams = Vec::new();
        loop {
            match *self.u.ty(t) {
                Type::Poly { binder, params, result } => {
                    let encoded: Vec<(String, TypeId)> = self
                        .u
                        .params_of(params)
                        .iter()
                        .map(|&(n, b)| (tasty::encode(self.u.name(n)), b))
                        .collect();
                    if top && !self.constructor {
                        tparams.extend(encoded.iter().cloned());
                    }
                    self.poly.push((binder, encoded));
                    t = result;
                }
                Type::Method { params, result, .. } => {
                    let params = self.u.params_of(params);
                    let names: Vec<&str> = params.iter().map(|&(n, _)| self.u.name(n)).collect();
                    let java_enum =
                        self.constructor && names.ends_with(&[nme::NAME_DOLLAR_, nme::ORDINAL_DOLLAR_]);
                    let keep = params.len() - if java_enum { 2 } else { 0 };
                    let outer = usize::from(self.constructor && names.first() == Some(&nme::OUTER));
                    vparams.extend(params.iter().take(keep).skip(outer).map(|&(_, p)| p));
                    t = self.context_result(result, &mut vparams);
                }
                _ => break,
            }
        }
        vparams.retain(|&p| !self.u.is_erased_param(p));
        if let Some(c) = self.extended.filter(|_| top) {
            let class_params = self.u.type_params(c).into_iter();
            let class_params = class_params.map(|p| (tasty::encode(&self.u.symbol(p).name), self.u.info(p)));
            tparams.extend(class_params.collect::<Vec<_>>());
        }
        if top && !self.constructor && !self.field {
            self.rename_shadowing(&mut tparams, vparams.iter().copied().chain([t]));
        }
        self.type_params(&tparams);
        self.out.push('(');
        for p in vparams {
            self.jsig(p, false, Boxing::Unbox);
        }
        self.out.push(')');
        self.result(t);
    }

    /// Renames each of `tparams` named like a class type parameter that `types` mention, `A` to
    /// the first of `A1`, `A2`, … not taken, in `tparams` and the references to them (dotty's
    /// `freshTypeParamName`).
    fn rename_shadowing(&mut self, tparams: &mut [(String, TypeId)], types: impl Iterator<Item = TypeId>) {
        let mut shadowed: Vec<String> = Vec::new();
        for t in types {
            for name in self.class_params_in(t) {
                if tparams.iter().any(|p| p.0 == name) && !shadowed.contains(&name) {
                    shadowed.push(name);
                }
            }
        }
        let mut renamed: Vec<(String, String)> = Vec::new();
        for (name, _) in tparams.iter_mut() {
            if !shadowed.contains(name) {
                continue;
            }
            let fresh =
                (1..).map(|i| format!("{name}{i}")).find(|n| !shadowed.contains(n)).expect("a fresh name");
            shadowed.push(fresh.clone());
            renamed.push((std::mem::replace(name, fresh.clone()), fresh));
        }
        for (_, params) in &mut self.poly {
            for (name, _) in params.iter_mut() {
                if let Some((_, fresh)) = renamed.iter().find(|r| r.0 == *name) {
                    *name = fresh.clone();
                }
            }
        }
        self.renamed.extend(renamed);
    }

    /// Appends a reference to type parameter `p`: a method's under its fresh name, or as the
    /// erasure of its bound if that is a primitive (dotty's `jsig` of a `TypeParamRef`).
    pub fn param_ref(&mut self, p: SymbolId, boxing: Boxing) {
        let mut name = tasty::encode(&self.u.symbol(p).name);
        if self.u.symbol(p).owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Def) {
            if let Some((_, fresh)) = self.renamed.iter().find(|r| r.0 == name) {
                name = fresh.clone();
            }
            let bounds = self.poly.iter().rev().flat_map(|(_, ps)| ps).find(|q| q.0 == name).map(|q| q.1);
            if let Some(bounds) = bounds.filter(|&b| self.primitive_bound(b)) {
                let hi = self.u.bounds(bounds).map_or(bounds, |(_, hi)| hi);
                return self.erased(hi, boxing);
            }
        }
        self.out.push_str(&format!("T{name};"));
    }

    /// The names of the type parameters of the classes in the top-level class that `t` mentions.
    fn class_params_in(&mut self, t: TypeId) -> Vec<String> {
        let top = self.tops[0];
        let mut out = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            let s = match *u.ty(part) {
                Type::Sym(_, s) => Some(s),
                Type::TypeRef(..) => u.resolve_ref(part),
                _ => None,
            };
            let class_param = |s: SymbolId| {
                u.symbol(s).kind == Kind::TypeParam && u.symbol(s).owner.is_some_and(|o| u.is_class(o))
            };
            if let Some(s) = s.filter(|&s| class_param(s)) {
                let mut o = u.symbol(s).owner;
                while let Some(x) = o.filter(|&x| x != top) {
                    o = u.symbol(x).owner;
                }
                if o.is_some() {
                    out.push(tasty::encode(&u.symbol(s).name));
                }
            }
            None
        });
        out
    }

    /// The result type of context function type `t` with its parameter types added to `vparams`,
    /// else `t` (dotty's `collectMethodParams`).
    fn context_result(&mut self, t: TypeId, vparams: &mut Vec<TypeId>) -> TypeId {
        let d = self.u.dealias(t);
        let Some((tycon, args)) = self.u.applied(d) else { return t };
        let mut args = args.to_vec();
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
