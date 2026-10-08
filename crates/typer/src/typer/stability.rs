use super::Typer;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether term `s` denotes the same value each time: not a variable or a by-name
    /// parameter, not a method unless a stable one (dotty's `isStableMember`).
    pub fn is_stable_member(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        match sym.kind {
            Kind::Param => {
                let info = self.u.info(s);
                !matches!(self.u.types().get(info), Type::ByName(_))
            }
            Kind::Val => !sym.flags.has(tasty::tags::MUTABLE),
            Kind::Def => sym.flags.has(tasty::tags::STABLE),
            _ => true,
        }
    }

    /// `t` widened while it is a by-name type or a reference to an unstable term (dotty's
    /// `widenIfUnstable`).
    pub fn widen_if_unstable(&mut self, t: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::ByName(r) => self.widen_if_unstable(r),
            Type::Sym(_, s) if !self.is_type_symbol(s) && !self.is_stable_member(s) => {
                match self.method_info(t) {
                    Some(info) if info != t => self.widen_if_unstable(info),
                    _ => t,
                }
            }
            _ => t,
        }
    }
}
