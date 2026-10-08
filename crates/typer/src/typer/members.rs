//! The members of a type (dotty's `Type.member`, `findMember`).

use super::Typer;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The members named `name` of type `t`.
    pub fn type_members(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        match self.u.types().get(t).clone() {
            Type::Package(p) => self.u.decl(p, name).unwrap_or_default(),
            Type::This(c) => self.type_members(c, name),
            Type::Constant(..) | Type::Str(_) | Type::TermRef(..) => {
                let widened = self.u.widen(t);
                self.type_members(widened, name)
            }
            Type::Sym(prefix, s) => match self.u.symbol(s).kind {
                Kind::Package => self.u.decl(s, name).unwrap_or_default(),
                Kind::Class => self.class_members(s, name),
                Kind::Val if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => {
                    self.u.module_class(s).map_or_else(Vec::new, |c| self.class_members(c, name))
                }
                _ => {
                    self.complete(s);
                    let info = match prefix {
                        Some(p) if !self.is_type_symbol(s) => self.u.member_info(p, s),
                        _ => self.u.info(s),
                    };
                    let info = match *self.u.types().get(info) {
                        Type::ByName(r) => r,
                        _ => info,
                    };
                    self.type_members(info, name)
                }
            },
            Type::Alias(a) | Type::Bounds(_, a) => self.type_members(a, name),
            Type::And(a, b) => {
                let (ma, mb) = (self.type_members(a, name), self.type_members(b, name));
                match (ma.is_empty(), mb.is_empty()) {
                    (true, _) => mb,
                    (_, true) => ma,
                    _ if ma == mb => ma,
                    _ => Vec::new(),
                }
            }
            Type::Or(..) => match self.u.join(t) {
                Some(j) if !matches!(self.u.types().get(j), Type::Or(..)) => self.type_members(j, name),
                _ => Vec::new(),
            },
            _ => match self.u.class_symbol(t) {
                Some(c) => self.class_members(c, name),
                None => Vec::new(),
            },
        }
    }

    /// The members named `name` of class `c`, its base classes' included: the first type member,
    /// or the term members, of each overriding pair the one dotty's `preferSym` prefers.
    pub fn class_members(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        self.complete(c);
        let classes = self.u.linearization(c);
        for &base in classes.iter().skip(1) {
            self.complete(base);
        }
        if name == "<init>" {
            return self.u.decl(c, name).unwrap_or_default();
        }
        let site = crate::defn::type_ref(self.u, c);
        let mut out: Vec<SymbolId> = Vec::new();
        for base in classes {
            let decls: Vec<SymbolId> = self.u.decl(base, name).unwrap_or_default();
            let decls: Vec<SymbolId> = decls
                .into_iter()
                .filter(|&m| base == c || !self.u.symbol(m).flags.has(tasty::tags::PRIVATE))
                .collect();
            for m in decls {
                if self.is_type_symbol(m) {
                    if !out.iter().any(|&o| self.is_type_symbol(o)) {
                        out.push(m);
                    }
                    continue;
                }
                let mut overridden = None;
                for (k, &o) in out.iter().enumerate() {
                    let def_with_params = match (self.u.symbol(o).kind, self.u.symbol(m).kind) {
                        (Kind::Val, Kind::Def) => self.has_params(m),
                        (Kind::Def, Kind::Val) => self.has_params(o),
                        _ => false,
                    };
                    if !self.is_type_symbol(o) && !def_with_params && self.same_member(site, o, m) {
                        overridden = Some(k);
                        break;
                    }
                }
                match overridden {
                    Some(k) if self.is_concrete(m) && !self.is_concrete(out[k]) => out[k] = m,
                    Some(_) => {}
                    None => out.push(m),
                }
            }
        }
        out
    }
}
