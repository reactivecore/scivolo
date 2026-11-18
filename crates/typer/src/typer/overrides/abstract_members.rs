// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Concrete classes with abstract members left unimplemented, which dotty's
//! `checkNoAbstractMembers` rejects.

use super::super::Typer;
use super::OverrideChecks;
use crate::defn::{EQUALS, PRODUCT};
use crate::symbols::Syms;
use names::{name_kinds, nme};
use rustc_hash::FxHashMap;
use tasty::tags::{ARTIFACT, CASE, ENUM, MUTABLE, PRIVATE, TRACKED};
use tasty::{Kind, SymbolId, Type, Universe};

impl OverrideChecks {
    /// Why the class, which is concrete, may not be defined, if it may not: an abstract term
    /// member of it or of a base class has no concrete member of the class matching it, as `f` in
    /// `trait A { def f: Int }; class B extends A`; tracked members do not count. Once macro
    /// annotations `expanded`, the members a macro made count as concrete.
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
            let decls = t.u.jdk_viewed_decls(base);
            for d in decls {
                let (kind, name) = (t.u.symbol(d).kind, t.u.symbol(d).name.clone());
                let accessor = t.u.symbol(d).flags.has(ARTIFACT) && name.contains("$super$");
                let mixin =
                    !t.syms.is_script(d) && (accessor || name.contains(name_kinds::TRAIT_SETTER_SEPARATOR));
                if !matches!(kind, Kind::Val | Kind::Def)
                    || name == nme::CONSTRUCTOR
                    || mixin
                    || t.is_concrete(d)
                    || t.u.symbol(d).flags.has(TRACKED)
                    || expanded && is_made(&t.syms, t.u, d)
                    || t.is_deferred_given(d)
                {
                    continue;
                }
                if let Some(getter) = var_of_setter(t.u, base, d)
                    && members_named(t, self.cls, &getter, &mut FxHashMap::default())
                        .into_iter()
                        .any(|m| t.u.symbol(m).flags.has(MUTABLE))
                {
                    continue;
                }
                let members = members_named(t, self.cls, &name, &mut FxHashMap::default());
                t.complete(d);
                let implemented = members.into_iter().any(|m| {
                    t.complete(m);
                    let concrete =
                        t.is_concrete(m) || is_class_param(t.u, m) || expanded && is_made(&t.syms, t.u, m);
                    concrete && t.implements(site, m, d)
                });
                if !implemented {
                    return Some("concrete class with an unimplemented member");
                }
            }
        }
        None
    }
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

/// The name of the variable of class `c` whose setter `d`, a declaration of `c`, is.
fn var_of_setter(u: &Universe, c: SymbolId, d: SymbolId) -> Option<String> {
    let getter = u.symbol(d).name.strip_suffix("_=")?;
    let is_var = u.symbol(c).decls.iter().any(|&v| {
        let s = u.symbol(v);
        s.name == getter && matches!(s.kind, Kind::Val | Kind::Param) && s.flags.has(MUTABLE)
    });
    is_var.then(|| getter.to_owned())
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
