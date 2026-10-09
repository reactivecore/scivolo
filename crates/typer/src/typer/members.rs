//! The members of a type (dotty's `Type.member`, `findMember`).

use super::Typer;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

/// How many steps through bounds, aliases and parts finding a member may take.
const MAX_MEMBER_DEPTH: u32 = 32;

impl Typer<'_> {
    /// The members named `name` of type `t`.
    pub fn type_members(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        self.members_within(t, name, MAX_MEMBER_DEPTH)
    }

    /// The members named `name` of type `t`, none if finding them takes more than `depth` steps
    /// through bounds, aliases and parts.
    fn members_within(&mut self, t: TypeId, name: &str, depth: u32) -> Vec<SymbolId> {
        let Some(depth) = depth.checked_sub(1) else { return Vec::new() };
        match self.u.types().get(t).clone() {
            Type::Package(p) => self.package_members(p, name),
            Type::This(c) => self.members_within(c, name, depth),
            Type::Constant(..) | Type::Str(_) | Type::TermRef(..) => {
                let widened = self.u.widen(t);
                self.members_within(widened, name, depth)
            }
            Type::Sym(prefix, s) => match self.u.symbol(s).kind {
                Kind::Package => self.package_members(s, name),
                Kind::Class => self.class_members(s, name),
                Kind::Val if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => {
                    self.u.module_class(s).map_or_else(Vec::new, |c| self.class_members(c, name))
                }
                _ => {
                    self.complete(s);
                    let info = match prefix {
                        Some(p) if !self.is_type_symbol(s) => {
                            let info = self.u.member_info(p, s);
                            let info = self.resolve_class_params(info);
                            self.instantiated_selected(info)
                        }
                        _ => self.u.info(s),
                    };
                    let info = match *self.u.types().get(info) {
                        Type::ByName(r) => r,
                        _ => info,
                    };
                    self.members_within(info, name, depth)
                }
            },
            Type::Alias(a) | Type::Bounds(_, a) => self.members_within(a, name, depth),
            Type::Applied(..) if self.u.dealias(t) != t => {
                let dealiased = self.u.dealias(t);
                self.members_within(dealiased, name, depth)
            }
            Type::And(a, b) => {
                let (ma, mb) = (self.members_within(a, name, depth), self.members_within(b, name, depth));
                match (ma.is_empty(), mb.is_empty()) {
                    (true, _) => mb,
                    (_, true) => ma,
                    _ if ma == mb => ma,
                    _ => Vec::new(),
                }
            }
            Type::Or(..) => match self.u.join(t) {
                Some(j) if !matches!(self.u.types().get(j), Type::Or(..)) => {
                    self.members_within(j, name, depth)
                }
                _ => Vec::new(),
            },
            _ => match self.u.class_symbol(t) {
                Some(c) => self.class_members(c, name),
                None => Vec::new(),
            },
        }
    }

    /// `t`, a type variable without instance but with a lower bound instantiated from below, so
    /// that members can be selected on it (dotty's `isFullyDefined` with `ForceDegree.failBottom`
    /// in `typedSelect`).
    fn instantiated_selected(&mut self, t: TypeId) -> TypeId {
        let t = self.zonk(t);
        if let Type::Var(v) = *self.u.types().get(t)
            && self.constraint.bounds(v).0 != tasty::NOTHING
            && self.instantiate_var(v, true).is_ok()
        {
            return self.zonk(t);
        }
        t
    }

    /// The members named `name` of package `p`, those of its package objects that the unit
    /// defines among them once completed.
    fn package_members(&mut self, p: SymbolId, name: &str) -> Vec<SymbolId> {
        let objects: Vec<SymbolId> =
            self.u.symbol(p).decls.iter().copied().filter(|&d| self.is_package_object(d)).collect();
        for o in objects {
            if matches!(self.syms.completions.get(&o), Some(crate::symbols::Completion::Pending { .. })) {
                self.complete(o);
            }
        }
        self.u.decl(p, name).unwrap_or_default()
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
                    if !self.is_type_symbol(o) && self.same_member(site, o, m) {
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
