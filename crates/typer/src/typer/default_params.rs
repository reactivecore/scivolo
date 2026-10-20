// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Which parameters of a method have default arguments (dotty's `HasDefault` and
//! `hasDefaultParams`), its own or those of a method it overrides.

use super::Typer;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Per parameter of the first value clause of method `s`, whether it has a default argument,
    /// declared or inherited from a method `s` overrides.
    pub fn default_params(&mut self, s: SymbolId) -> Vec<bool> {
        let (s, clause) = self.mapped_alts.get(&s).copied().unwrap_or((s, 0));
        let mut own = self.declared_defaults(s, Some(clause));
        for m in self.overridden(s) {
            let inherited = self.declared_defaults(m, Some(clause));
            for (d, i) in own.iter_mut().zip(inherited) {
                *d |= i;
            }
        }
        own
    }

    /// Whether a parameter of method `s`, in any clause, has a default argument, declared or
    /// inherited (dotty's `hasDefaultParams`).
    pub fn has_defaults(&mut self, s: SymbolId) -> bool {
        let s = self.mapped_alts.get(&s).map_or(s, |&(original, _)| original);
        let methods: Vec<SymbolId> = std::iter::once(s).chain(self.overridden(s)).collect();
        methods.into_iter().any(|m| self.declared_defaults(m, None).contains(&true))
    }

    /// Whether parameter `idx` of method `s`, counted over all its clauses, has a default
    /// argument, declared or inherited.
    pub fn has_default_at(&mut self, s: SymbolId, idx: usize) -> bool {
        let methods: Vec<SymbolId> = std::iter::once(s).chain(self.overridden(s)).collect();
        methods.into_iter().any(|m| self.declared_defaults(m, None).get(idx) == Some(&true))
    }

    /// The methods of base classes that method `s` overrides, not private, none for a constructor
    /// (dotty's `allOverriddenSymbols`).
    pub fn overridden(&mut self, s: SymbolId) -> Vec<SymbolId> {
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

    /// Per parameter of value clause `clause` of method `s`, or of all its clauses if none,
    /// whether `s` declares a default for it.
    fn declared_defaults(&mut self, s: SymbolId, clause: Option<usize>) -> Vec<bool> {
        self.complete(s);
        let sizes: Vec<usize> = match self.syms.param_clauses.get(&s) {
            Some(clauses) => clauses.iter().map(Vec::len).collect(),
            None => {
                let info = self.u.info(s);
                self.u.param_clauses(info).iter().map(|(_, ps)| ps.len()).collect()
            }
        };
        let (skip, count) = match clause {
            Some(c) => (sizes.iter().take(c).sum(), sizes.get(c).copied().unwrap_or(0)),
            None => (0, sizes.iter().sum()),
        };
        let params: Vec<SymbolId> = match self.syms.param_clauses.get(&s) {
            Some(clauses) => clauses.iter().flatten().copied().collect(),
            None => {
                let decls = self.u.symbol(s).decls.iter().copied();
                decls.filter(|&p| self.u.symbol(p).kind == Kind::Param).collect()
            }
        };
        let script = self.syms.param_clauses.contains_key(&s);
        (skip..skip + count)
            .map(|i| match params.get(i) {
                Some(p) if script => self.syms.flags.get(p).is_some_and(|fl| fl.is(f::HAS_DEFAULT)),
                Some(&p) => self.u.symbol(p).flags.has(tasty::tags::HASDEFAULT),
                None => false,
            })
            .collect()
    }
}
