// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Whether one method's signature matches another's as members of the same class (dotty's
//! `matches`), on types in a normal form.

use super::{SymbolId, Universe};
use crate::types::{Binder, Type, TypeId};

/// The binder and parameters of each value parameter clause of a method type.
type Clauses = Vec<(Binder, Vec<(String, TypeId)>)>;

impl Universe {
    /// Whether members `a` and `b` of `site` match as Scala members (dotty's `matchesLoosely`):
    /// their signatures match, a method without parameters matching a non-method, or their
    /// parameters do and so do their types (`info_matches`).
    pub fn matches(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        use crate::erasure::{MatchDegree, match_degree};
        if self.info_matches(site, a, b) {
            return true;
        }
        let scala2 = self.erased_as_scala2(a) && self.erased_as_scala2(b);
        let (sa, sb) = (self.signature(site, a, scala2), self.signature(site, b, scala2));
        matches!(match_degree(&sa, &sb), MatchDegree::Full | MatchDegree::MethodNotAMethod)
    }

    /// Whether the types of members `a` and `b` of `site` match (dotty's relaxed `matchesType`):
    /// the same numbers of type parameters and the same parameter types, `b`'s type parameters,
    /// symbols or those of its polymorphic type, renamed to `a`'s; clauses past the other's last
    /// empty.
    pub fn info_matches(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        let (ta, tb) = (self.type_params(a), self.type_params(b));
        if ta.len() != tb.len() {
            return false;
        }
        let renamed: Vec<TypeId> = ta.iter().map(|&p| self.intern_type(Type::Sym(None, p))).collect();
        let (ia, ib) = (self.member_info(site, a), self.member_info(site, b));
        let (pa, pb) = (self.poly_binders(ia), self.poly_binders(ib));
        if pa.iter().map(|p| p.1).ne(pb.iter().map(|p| p.1)) {
            return false;
        }
        let (ca, cb) = (self.param_clauses(ia), self.param_clauses(ib));
        let n = ca.len().min(cb.len());
        let rest_empty = ca[n..].iter().chain(&cb[n..]).all(|(_, ps)| ps.is_empty());
        if !rest_empty || ca.iter().zip(&cb).any(|((_, x), (_, y))| x.len() != y.len()) {
            return false;
        }
        let (ma, mb) = (self.method_clauses(ia), self.method_clauses(ib));
        let pairs = ca.into_iter().zip(cb).flat_map(|((_, x), (_, y))| x.into_iter().zip(y));
        pairs.collect::<Vec<_>>().into_iter().all(|(x, y)| {
            let (ex, ey) = (self.repeated_elem(x), self.repeated_elem(y));
            if ex.is_some() != ey.is_some() {
                return false;
            }
            let (x, y) = (self.normalize(ex.unwrap_or(x)), self.normalize(ey.unwrap_or(y)));
            let rename =
                |u: &mut Universe, t, own: &[SymbolId], binders: &[(Binder, usize)], methods: &Clauses| {
                    let t = methods.iter().fold(t, |t, (b, ps)| u.param_syms_as_refs(t, ps, *b));
                    u.map_type(t, &mut |u, t| match *u.ty(t) {
                        Type::Sym(_, s) => own.iter().position(|&p| p == s).map(|i| renamed[i]),
                        Type::Param(b, i) => {
                            if let Some(k) = methods.iter().position(|(own, _)| *own == b) {
                                return Some(u.intern_type(Type::Param(ma[k].0, i)));
                            }
                            let k = binders.iter().position(|&(own, _)| own == b)?;
                            let offset: usize = binders[..k].iter().map(|p| p.1).sum();
                            let to = u.intern_type(Type::Param(pa[k].0, i));
                            renamed.get(offset + i as usize).copied().or(Some(to))
                        }
                        _ => None,
                    })
                };
            let x = rename(self, x, &ta, &pa, &ma);
            let y = rename(self, y, &tb, &pb, &mb);
            let (x, y) = (self.param_paths_named(x, &ma), self.param_paths_named(y, &ma));
            x == y || self.is_same_type_in(x, y, &mut crate::subtype::NoVars)
        })
    }

    /// `t` with each member reference on a path from a parameter of clauses `methods` made a
    /// reference by name.
    fn param_paths_named(&mut self, t: TypeId, methods: &Clauses) -> TypeId {
        self.map_type(t, &mut |u, x| {
            let Type::Sym(Some(p), s) = *u.ty(x) else { return None };
            let mut root = p;
            while let Type::Sym(Some(q), _) | Type::TermRef(q, _) | Type::TypeRef(q, _) = *u.ty(root) {
                root = q;
            }
            let (b, _) = u.param_ref(root)?;
            if !methods.iter().any(|(own, _)| *own == b) {
                return None;
            }
            let p = u.param_paths_named(p, methods);
            let name = u.symbol(s).name.clone();
            let named =
                if u.symbol(s).kind.is_term() { Type::TermRef(p, name) } else { Type::TypeRef(p, name) };
            Some(u.intern_type(named))
        })
    }

    /// The binder and parameters of each value parameter clause of method type `t`.
    fn method_clauses(&self, mut t: TypeId) -> Clauses {
        let mut out = Vec::new();
        while let Type::Poly { result, .. } | Type::Method { result, .. } = self.ty(t) {
            if let Type::Method { binder, params, .. } = self.ty(t) {
                out.push((*binder, params.clone()));
            }
            t = *result;
        }
        out
    }

    /// The element type `T` of repeated parameter type `t`: `<repeated>[T]` or `Seq[T] @Repeated`
    /// as TASTy gives it.
    pub fn repeated_elem(&mut self, t: TypeId) -> Option<TypeId> {
        let applied = match self.ty(t).clone() {
            Type::Annotated(seq, annot)
                if self.is_class_named(annot, "scala.annotation.internal.Repeated") =>
            {
                seq
            }
            Type::Applied(c, _) if self.is_class_named(c, "scala.<repeated>") => t,
            _ => return None,
        };
        match self.applied(applied)? {
            (_, &[elem]) => Some(elem),
            _ => None,
        }
    }

    /// The result type of method type `t` after all its clauses.
    pub fn result_type(&self, mut t: TypeId) -> TypeId {
        while let Type::Poly { result, .. } | Type::Method { result, .. } = self.ty(t) {
            t = *result;
        }
        t
    }

    /// The binder of each type-parameter clause of method type `t` and its number of parameters.
    fn poly_binders(&self, mut t: TypeId) -> Vec<(Binder, usize)> {
        let mut out = Vec::new();
        while let Type::Poly { result, .. } | Type::Method { result, .. } = self.ty(t) {
            if let Type::Poly { binder, params, .. } = self.ty(t) {
                out.push((*binder, params.len()));
            }
            t = *result;
        }
        out
    }

    /// The value parameter clauses of method type `t`: whether each is implicit, and its
    /// parameters' types.
    pub fn param_clauses(&self, mut t: TypeId) -> Vec<(bool, Vec<TypeId>)> {
        let mut out = Vec::new();
        loop {
            match self.ty(t) {
                Type::Poly { result, .. } => t = *result,
                Type::Method { params, result, implicit, .. } => {
                    out.push((*implicit, params.iter().map(|(_, p)| *p).collect()));
                    t = *result;
                }
                _ => return out,
            }
        }
    }
}
