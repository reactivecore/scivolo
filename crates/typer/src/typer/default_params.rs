//! Which parameters of a method have default arguments (dotty's `HasDefault` and
//! `hasDefaultParams`), its own or those of a method it overrides.

use super::Typer;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Per parameter of the first value clause of method `s`, whether it has a default argument,
    /// declared or inherited from a method `s` overrides.
    pub fn default_params(&mut self, s: SymbolId) -> Vec<bool> {
        let mut own = self.declared_defaults(s, true);
        for m in self.overridden(s) {
            let inherited = self.declared_defaults(m, true);
            for (d, i) in own.iter_mut().zip(inherited) {
                *d |= i;
            }
        }
        own
    }

    /// Whether a parameter of method `s`, in any clause, has a default argument, declared or
    /// inherited (dotty's `hasDefaultParams`).
    pub fn has_defaults(&mut self, s: SymbolId) -> bool {
        let methods: Vec<SymbolId> = std::iter::once(s).chain(self.overridden(s)).collect();
        methods.into_iter().any(|m| self.declared_defaults(m, false).contains(&true))
    }

    /// The methods of base classes that method `s` overrides, not private, none for a constructor
    /// (dotty's `allOverriddenSymbols`).
    fn overridden(&mut self, s: SymbolId) -> Vec<SymbolId> {
        let name = self.u.symbol(s).name.clone();
        let Some(owner) = self.u.symbol(s).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class) else {
            return Vec::new();
        };
        if name == "<init>" {
            return Vec::new();
        }
        let site = crate::defn::type_ref(self.u, owner);
        let mut out = Vec::new();
        for base in self.u.linearization(owner).into_iter().skip(1) {
            for m in self.u.decl(base, &name).unwrap_or_default() {
                let private = self.u.symbol(m).flags.has(tasty::tags::PRIVATE);
                if m != s && !private && self.same_member(site, m, s) {
                    out.push(m);
                }
            }
        }
        out
    }

    /// Per parameter of the first value clause of method `s`, or of all its clauses unless
    /// `first`, whether `s` declares a default for it.
    fn declared_defaults(&mut self, s: SymbolId, first: bool) -> Vec<bool> {
        self.complete(s);
        if let Some(clauses) = self.syms.param_clauses.get(&s) {
            let taken = if first { clauses.len().min(1) } else { clauses.len() };
            let params: Vec<SymbolId> = clauses[..taken].iter().flatten().copied().collect();
            return params.iter().map(|p| self.syms.flags.get(p).is_some_and(|fl| fl.is(f::HAS_DEFAULT))).collect();
        }
        let info = self.u.info(s);
        let clauses = self.u.param_clauses(info);
        let count = if first { clauses.first().map_or(0, |(_, ps)| ps.len()) } else { clauses.iter().map(|(_, ps)| ps.len()).sum() };
        let params: Vec<SymbolId> =
            self.u.symbol(s).decls.iter().copied().filter(|&p| self.u.symbol(p).kind == Kind::Param).collect();
        (0..count).map(|i| params.get(i).is_some_and(|&p| self.u.symbol(p).flags.has(tasty::tags::HASDEFAULT))).collect()
    }
}
