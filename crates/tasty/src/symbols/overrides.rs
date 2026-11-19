// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Whether one method's signature matches another's as members of the same class (dotty's
//! `matches`), on types in a normal form.

use super::method_clauses::Clauses;
use super::{SymbolId, Universe};
use crate::types::{Binder, Type, TypeId};

/// The rules by which `info_matches_in` relaxes the comparison of parameter types.
#[derive(Clone, Copy, Default)]
struct Relaxed {
    /// Whether repeated parameters are compared as `ElimRepeated` translates them.
    repeated: bool,
    /// Whether the first and the second member are Java members, whose `Object` parameters
    /// match `Any`.
    java: (bool, bool),
}

impl Universe {
    /// Whether members `a` and `b` of `site` match as Scala members (dotty's `matchesLoosely`):
    /// their signatures match, a method without parameters matching a non-method, or their
    /// parameters do and so do their types (`info_matches`).
    pub fn matches(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        use crate::erasure::MatchDegree;
        self.info_matches(site, a, b)
            || matches!(self.signature_match(site, a, b), MatchDegree::Full | MatchDegree::MethodNotAMethod)
    }

    /// Determines whether the signatures of members `a` and `b` of `site` have the same erased
    /// parameter types (dotty's `consistentParams`).
    pub fn parameters_agree(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        let scala2 = self.erased_as_scala2(a) && self.erased_as_scala2(b);
        let (sa, sb) = (self.dealiased_signature(site, a, scala2), self.dealiased_signature(site, b, scala2));
        !matches!(crate::erasure::match_degree(&sa, &sb), crate::erasure::MatchDegree::No)
    }

    fn signature_match(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> crate::erasure::MatchDegree {
        let scala2 = self.erased_as_scala2(a) && self.erased_as_scala2(b);
        let (sa, sb) = (self.signature(site, a, scala2), self.signature(site, b, scala2));
        crate::erasure::match_degree(&sa, &sb)
    }

    /// Whether the types of members `a` and `b` of `site` match (dotty's relaxed `matchesType`):
    /// the same numbers of type parameters and the same parameter types, `b`'s type parameters,
    /// symbols or those of its polymorphic type, renamed to `a`'s; clauses past the other's last
    /// empty.
    pub fn info_matches(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        self.info_matches_in(site, (a, b), Relaxed::default())
    }

    /// Determines whether the types of members `a` and `b` of `site` match as `info_matches`
    /// does, but as dotty compares them in `RefChecks`, after `ElimRepeated`: a repeated
    /// parameter `T*` matches `Seq[T]`, Java varargs `Array[T]`; a parameter a Java member
    /// declares `Object` (`java`, whether `a` and `b` are Java members) matches `Any` and `Object`.
    pub fn info_matches_in_refchecks(
        &mut self,
        site: TypeId,
        a: SymbolId,
        b: SymbolId,
        java: (bool, bool),
    ) -> bool {
        self.info_matches_in(site, (a, b), Relaxed { repeated: true, java })
    }

    fn info_matches_in(&mut self, site: TypeId, (a, b): (SymbolId, SymbolId), relaxed: Relaxed) -> bool {
        let (ta, tb) = (self.type_params(a), self.type_params(b));
        if ta.len() != tb.len() {
            return false;
        }
        if self.is_class_site(site) {
            let (da, db) = (self.info(a), self.info(b));
            if !self.same_param_counts(da, db) {
                return false;
            }
        }
        let renamed: Vec<TypeId> = ta.iter().map(|&p| self.intern_type(Type::Sym(None, p))).collect();
        let (ia, ib) = (self.member_info(site, a), self.member_info(site, b));
        if !self.same_param_counts(ia, ib) {
            return false;
        }
        let (pa, pb) = (self.poly_binders(ia), self.poly_binders(ib));
        let (ca, cb) = (self.param_clauses(ia), self.param_clauses(ib));
        let (ma, mb) = (self.method_clauses(ia), self.method_clauses(ib));
        let (da, db) = (self.info(a), self.info(b));
        let declared = |u: &Self, info: TypeId| u.param_clauses(info).into_iter().flat_map(|(_, ps)| ps);
        let declared: Vec<(TypeId, TypeId)> = declared(self, da).zip(declared(self, db)).collect();
        let pairs = ca.into_iter().zip(cb).flat_map(|((_, x), (_, y))| x.into_iter().zip(y));
        pairs.collect::<Vec<_>>().into_iter().enumerate().all(|(k, (x, y))| {
            let (x, y) = match (self.repeated_elem(x), self.repeated_elem(y)) {
                (Some(ex), Some(ey)) => (ex, ey),
                (None, None) => (x, y),
                _ if relaxed.repeated => (self.from_repeated(x), self.from_repeated(y)),
                _ => return false,
            };
            let (x, y) = (self.normalize(x), self.normalize(y));
            if let Some(&(da, db)) = declared.get(k) {
                let (ja, jb) = relaxed.java;
                if ja && self.is_top(da) && self.is_top(y) || jb && self.is_top(db) && self.is_top(x) {
                    return true;
                }
            }
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

    /// Whether `t` is `Any`, `Object` or Java's `Object`.
    fn is_top(&mut self, t: TypeId) -> bool {
        let d = self.defn();
        let s = match *self.ty(t) {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(..) => self.resolve_ref(t),
            _ => None,
        };
        s.is_some_and(|s| s == d.any || s == d.object || s == d.from_java_object)
    }

    /// Whether `t` is a reference to a class, whose members have the parameters they declare.
    fn is_class_site(&self, t: TypeId) -> bool {
        matches!(*self.ty(t), Type::Sym(_, c) if self.symbol(c).kind == super::Kind::Class)
    }

    /// Whether method types `a` and `b` have as many type parameters per clause and as many value
    /// parameters per clause, clauses past the other's last empty.
    fn same_param_counts(&self, a: TypeId, b: TypeId) -> bool {
        let (pa, pb) = (self.poly_binders(a), self.poly_binders(b));
        if pa.iter().map(|p| p.1).ne(pb.iter().map(|p| p.1)) {
            return false;
        }
        let (ca, cb) = (self.param_clauses(a), self.param_clauses(b));
        let n = ca.len().min(cb.len());
        let rest_empty = ca[n..].iter().chain(&cb[n..]).all(|(_, ps)| ps.is_empty());
        rest_empty && ca.iter().zip(&cb).all(|((_, x), (_, y))| x.len() == y.len())
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
            let name = u.interned_name(s);
            let named =
                if u.symbol(s).kind.is_term() { Type::TermRef(p, name) } else { Type::TypeRef(p, name) };
            Some(u.intern_type(named))
        })
    }
}
