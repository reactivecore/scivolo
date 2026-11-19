// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The order in which an export's members get forwarders: the names a wildcard selects, and the
//! alternatives of one name (dotty's `memberNames`, `computeMembersNamed`).

use crate::typer::Typer;
use tasty::tags as t;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The members named `name` of path type `t`, the terms first, each in the order of dotty's
    /// `membersNamed`.
    pub fn terms_then_types(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        let mut members = self.type_members(t, name);
        if let Some(c) = self.path_class(t) {
            let mut order = Vec::new();
            self.members_named_order(c, name, &mut order, &mut Vec::new());
            let keys: Vec<usize> = members.iter().map(|&m| self.first_matching(t, &order, m)).collect();
            let mut keyed: Vec<(usize, SymbolId)> = keys.into_iter().zip(members).collect();
            keyed.sort_by_key(|&(k, _)| k);
            members = keyed.into_iter().map(|(_, m)| m).collect();
        }
        members.sort_by_key(|&m| self.u.is_type_symbol(m));
        members
    }

    /// The index of the first of `order` that is `m` or a term matching `m` as a member of `t`,
    /// the place dotty's `meet` gives to the denotation merging them.
    fn first_matching(&mut self, t: TypeId, order: &[SymbolId], m: SymbolId) -> usize {
        let term = |t: &Self, s: SymbolId| !t.u.is_type_symbol(s);
        let found =
            order.iter().position(|&o| o == m || term(self, m) && term(self, o) && self.same_member(t, o, m));
        found.unwrap_or(order.len())
    }

    /// Appends to `out` the declarations named `name` of class `c` and its base classes not in
    /// `visited`: its own, the last declared first, then those of its parents, the last parent
    /// first (dotty's `computeMembersNamed`).
    fn members_named_order(
        &mut self,
        c: SymbolId,
        name: &str,
        out: &mut Vec<SymbolId>,
        visited: &mut Vec<SymbolId>,
    ) {
        if visited.contains(&c) {
            return;
        }
        visited.push(c);
        self.complete(c);
        for d in self.u.decls_named(c, name).into_iter().rev() {
            if !out.contains(&d) {
                out.push(d);
            }
        }
        for p in self.u.parents(c).into_iter().rev() {
            if let Some(pc) = self.u.class_symbol(p) {
                self.members_named_order(pc, name, out, visited);
            }
        }
    }

    /// The class whose members a path of type `t` has.
    pub fn path_class(&mut self, t: TypeId) -> Option<SymbolId> {
        if let Some(s) = self.u.sym_of(t)
            && self.u.symbol(s).flags.has(t::OBJECT)
        {
            return self.u.module_class(s);
        }
        let widened = self.u.widen(t);
        let widened = self.u.dealias(widened);
        let tycon = self.u.applied(widened).map_or(widened, |(tycon, _)| tycon);
        let c = match *self.u.ty(tycon) {
            Type::Sym(_, c) => c,
            Type::TypeRef(..) => self.u.resolve_ref(tycon)?,
            _ => return None,
        };
        (self.u.symbol(c).kind == Kind::Class).then_some(c)
    }

    /// The names of the members of class `c` and its base classes, ordered as Java strings.
    pub fn member_names_of(&mut self, c: SymbolId) -> Vec<String> {
        self.complete(c);
        let mut names: Vec<String> = Vec::new();
        for base in self.u.linearization(c) {
            self.complete(base);
            for d in self.u.decls(base).unwrap_or_default() {
                names.push(self.u.symbol(d).name.clone());
            }
        }
        names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        names.dedup();
        names
    }
}
