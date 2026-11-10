// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Concrete classes with abstract members left unimplemented, which dotty's
//! `checkNoAbstractMembers` rejects.

use super::super::Typer;
use super::OverrideChecks;
use crate::defn::{EQUALS, PRODUCT};
use crate::symbols::Syms;
use rustc_hash::FxHashMap;
use tasty::tags::{ARTIFACT, CASE, ENUM, MUTABLE, PRIVATE};
use tasty::{Erased, Kind, SymbolId, Type, TypeId, Universe};

impl OverrideChecks {
    /// Why the class, which is concrete, may not be defined, if it may not: an abstract term
    /// member of it or of a base class has no concrete member of the class matching it, as `f` in
    /// `trait A { def f: Int }; class B extends A`. Once macro annotations `expanded`, the members
    /// a macro made count as concrete.
    pub fn unimplemented(&self, t: &mut Typer<'_>, expanded: bool) -> Option<&'static str> {
        let bases = t.u.linearization(self.cls);
        let case = bases.iter().any(|&b| t.u.symbol(b).flags.has(CASE));
        let enum_ = bases.iter().any(|&b| t.u.symbol(b).flags.has(ENUM));
        let site = t.u.intern_type(Type::This(self.site));
        for base in bases {
            let synthesized = match t.u.full_name(base).as_str() {
                PRODUCT | EQUALS => case,
                "scala.reflect.Enum" => enum_,
                _ => false,
            };
            if synthesized {
                continue;
            }
            let decls = t.u.symbol(base).decls.clone();
            for d in decls {
                let (kind, name) = (t.u.symbol(d).kind, t.u.symbol(d).name.clone());
                let accessor = t.u.symbol(d).flags.has(ARTIFACT) && name.contains("$super$");
                let mixin = !t.syms.is_script(d) && (accessor || name.contains("$_setter_$"));
                if !matches!(kind, Kind::Val | Kind::Def)
                    || name == "<init>"
                    || mixin
                    || t.is_concrete(d)
                    || expanded && is_made(&t.syms, t.u, d)
                    || is_var_setter(t.u, base, d)
                    || t.is_deferred_given(d)
                {
                    continue;
                }
                let members = members_named(t, self.cls, &name, &mut FxHashMap::default());
                t.complete(d);
                let implemented = members.into_iter().any(|m| {
                    t.complete(m);
                    let concrete =
                        t.is_concrete(m) || is_class_param(t.u, m) || expanded && is_made(&t.syms, t.u, m);
                    concrete && (t.same_member(site, m, d) || same_shape(t.u, site, m, d))
                });
                if !implemented {
                    return Some("concrete class with an unimplemented member");
                }
            }
        }
        None
    }
}

/// Whether methods `a` and `b` take as many type and value parameters in each clause, with the
/// same erased parameter types as members of `site`, as `matches` does not compare interleaved
/// clauses and path-dependent parameter types.
fn same_shape(u: &mut Universe, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
    let def = |u: &Universe, s| u.symbol(s).kind == Kind::Def;
    def(u, a)
        && def(u, b)
        && clause_shape(u, a) == clause_shape(u, b)
        && erased_params(u, site, a) == erased_params(u, site, b)
}

/// Whether each clause of method `s` is a type clause, with its number of parameters.
fn clause_shape(u: &mut Universe, s: SymbolId) -> Vec<(bool, usize)> {
    let mut t = u.info(s);
    let mut out = Vec::new();
    loop {
        match u.types().get(t) {
            Type::Poly { params, result, .. } => (out.push((true, params.len())), t = *result),
            Type::Method { params, result, .. } => (out.push((false, params.len())), t = *result),
            _ => return out,
        };
    }
}

/// The erased parameter types of method `s` as a member of `site`, a parameter's singleton
/// type read as the parameter's type there.
fn erased_params(u: &mut Universe, site: TypeId, s: SymbolId) -> Vec<Erased> {
    let info = u.member_info(site, s);
    let mut params = Vec::new();
    let mut t = info;
    loop {
        match u.types().get(t) {
            Type::Poly { result, .. } => t = *result,
            Type::Method { params: ps, result, .. } => {
                params.extend_from_slice(ps);
                t = *result;
            }
            _ => break,
        }
    }
    let info = u.map_type(info, &mut |u, x| {
        let &Type::Sym(None, p) = u.types().get(x) else { return None };
        let param = u.symbol(p);
        let own = param.kind == Kind::Param && param.owner == Some(s);
        own.then(|| params.iter().find(|(n, _)| *n == param.name).map(|&(_, t)| t)).flatten()
    });
    let info = u.normalize(info);
    u.erased_method_type(info).0
}

/// Whether `m` is the parameter of a library class, a field.
fn is_class_param(u: &Universe, m: SymbolId) -> bool {
    let s = u.symbol(m);
    s.tree.is_some() && s.kind == Kind::Param && s.owner.is_some_and(|o| u.is_class(o))
}

/// Whether `m` is a member a macro made in a class of the unit, which is concrete.
fn is_made(syms: &Syms, u: &Universe, m: SymbolId) -> bool {
    let owner = u.symbol(m).owner;
    !syms.flags.contains_key(&m) && owner.is_some_and(|o| syms.flags.contains_key(&o))
}

/// Whether `d`, a declaration of class `c`, is the setter of a variable of `c`; the variable
/// is checked itself.
fn is_var_setter(u: &Universe, c: SymbolId, d: SymbolId) -> bool {
    let Some(getter) = u.symbol(d).name.strip_suffix("_=") else { return false };
    u.symbol(c).decls.iter().any(|&v| {
        let s = u.symbol(v);
        s.name == getter && matches!(s.kind, Kind::Val | Kind::Param) && s.flags.has(MUTABLE)
    })
}

/// The non-private term members of class `c` named `name`: its own, and those of its parents
/// that no own member matches (dotty's `nonPrivateMembersNamed`).
fn members_named(
    t: &mut Typer<'_>,
    c: SymbolId,
    name: &str,
    memo: &mut FxHashMap<SymbolId, Vec<SymbolId>>,
) -> Vec<SymbolId> {
    if let Some(found) = memo.get(&c) {
        return found.clone();
    }
    memo.insert(c, Vec::new());
    let own: Vec<SymbolId> = t.u.symbol(c).decls.clone();
    let own: Vec<SymbolId> = own
        .into_iter()
        .filter(|&d| {
            let s = t.u.symbol(d);
            s.name == name && s.kind.is_term() && !s.flags.has(PRIVATE)
        })
        .collect();
    own.iter().for_each(|&o| t.complete(o));
    let site = crate::defn::type_ref(t.u, c);
    let mut out = own.clone();
    for p in t.u.parents(c) {
        let Some(pc) = t.u.class_symbol(p) else { continue };
        for m in members_named(t, pc, name, memo) {
            if !out.contains(&m) && !own.iter().any(|&o| t.same_member(site, o, m)) {
                out.push(m);
            }
        }
    }
    memo.insert(c, out.clone());
    out
}
