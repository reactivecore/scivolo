// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    pub fn is_stable_member(&mut self, s: SymbolId) -> bool {
        is_stable_member(self.u, s)
    }

    /// Whether `tree` is a literal, `this`, or a path of stable members.
    pub fn is_stable_path(&mut self, tree: TNodeId) -> bool {
        let stable = |t: &mut Self, r| t.tpd.sym(r).is_some_and(|s| t.is_stable_member(s));
        match *self.tpd.tree(tree) {
            T::Literal { .. } | T::This { .. } => true,
            T::Ident { .. } => stable(self, tree),
            T::Select { qualifier, .. } => stable(self, tree) && self.is_stable_path(qualifier),
            _ => false,
        }
    }

    /// `t` widened while it is a by-name type or a reference to an unstable term (dotty's
    /// `widenIfUnstable`).
    pub fn widen_if_unstable(&mut self, t: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::ByName(r) => self.widen_if_unstable(r),
            Type::Sym(_, s) if !self.u.is_type_symbol(s) && !self.is_stable_member(s) => {
                if let Some(&annotated) = self.syms.repeated_params.get(&s) {
                    return annotated;
                }
                match self.method_info(t) {
                    Some(info) if info != t => self.widen_if_unstable(info),
                    _ => t,
                }
            }
            _ => t,
        }
    }

    /// `t`, or the result type of the by-name parameter it refers to, as the parts of a `try`
    /// take it.
    pub fn by_name_result(&mut self, t: TypeId) -> TypeId {
        let Type::Sym(_, s) = *self.u.types().get(t) else { return t };
        if self.u.symbol(s).kind != Kind::Param {
            return t;
        }
        let info = self.u.info(s);
        match *self.u.types().get(info) {
            Type::ByName(r) => r,
            _ => t,
        }
    }
}

/// Whether term `s` denotes the same value each time: not a variable, an inline or by-name
/// parameter, not a method unless a stable one (dotty's `isStableMember`).
pub fn is_stable_member(u: &mut Universe, s: SymbolId) -> bool {
    let sym = u.symbol(s);
    match sym.kind {
        Kind::Param if sym.flags.has(tasty::tags::INLINE) => false,
        Kind::Param => {
            let info = u.info(s);
            !matches!(u.types().get(info), Type::ByName(_))
        }
        Kind::Val => !sym.flags.has(tasty::tags::MUTABLE),
        Kind::Def => sym.flags.has(tasty::tags::STABLE),
        _ => true,
    }
}

/// Whether `t` is a stable path: a `this`, a constant, a package, or a reference to a stable
/// member through a stable prefix (dotty's `isStable`).
pub fn is_stable_path(u: &mut Universe, t: TypeId) -> bool {
    match *u.types().get(t) {
        Type::This(_)
        | Type::RecThis(_)
        | Type::Constant(..)
        | Type::Str(_)
        | Type::Package(_)
        | Type::Skolem(..) => true,
        Type::Sym(p, s) if u.symbol(s).kind.is_term() => {
            is_stable_member(u, s)
                && p.is_none_or(|p| {
                    matches!(u.types().get(p), Type::NoPrefix | Type::Sym(..) if !is_term_ref(u, p))
                        || is_stable_path(u, p)
                })
        }
        _ => false,
    }
}

fn is_term_ref(u: &Universe, t: TypeId) -> bool {
    matches!(*u.types().get(t), Type::Sym(_, s) if u.symbol(s).kind.is_term())
}
