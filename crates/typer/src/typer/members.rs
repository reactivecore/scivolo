//! The members of a type (dotty's `Type.member`, `findMember`).

use super::Typer;
use crate::ctx::Ctx;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

/// How many steps through bounds, aliases and parts finding a member may take.
const MAX_MEMBER_DEPTH: u32 = 32;

impl Typer<'_> {
    /// The members named `name` of type `t`.
    pub fn type_members(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        self.members_within(t, name, (MAX_MEMBER_DEPTH, None))
    }

    /// The members named `name` of type `t` as selected in `ctx`: of an intersection's matching
    /// members, those inaccessible in `ctx` hidden.
    pub fn type_members_in(&mut self, t: TypeId, name: &str, ctx: Ctx) -> Vec<SymbolId> {
        self.members_within(t, name, (MAX_MEMBER_DEPTH, Some(ctx)))
    }

    /// The members named `name` of type `t`, none if finding them takes more than `depth` steps
    /// through bounds, aliases and parts.
    fn members_within(&mut self, t: TypeId, name: &str, (depth, ctx): (u32, Option<Ctx>)) -> Vec<SymbolId> {
        let Some(depth) = depth.checked_sub(1) else { return Vec::new() };
        let within = (depth, ctx);
        match self.u.types().get(t).clone() {
            Type::Package(p) => self.package_members(p, name),
            Type::This(c) => {
                let class = self.u.class_symbol(c);
                if let Some(k) = class {
                    self.ensure_self(k);
                }
                match class.filter(|&k| self.u.given_self_type(k).is_some()) {
                    Some(k) => {
                        let self_type = self.u.self_type(k);
                        self.members_within(self_type, name, within)
                    }
                    None => self.members_within(c, name, within),
                }
            }
            Type::Super(this, p) if matches!(self.u.types().get(p), Type::And(..)) => {
                let class = match *self.u.types().get(this) {
                    Type::This(c) => self.u.class_symbol(c),
                    _ => None,
                };
                class.map_or_else(Vec::new, |c| self.super_members(c, name))
            }
            Type::Super(_, p) | Type::Skolem(_, p) => self.members_within(p, name, within),
            Type::Constant(..) | Type::Str(_) | Type::TermRef(..) => {
                let widened = self.u.widen(t);
                self.members_within(widened, name, within)
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
                        Some(p) if !self.u.is_type_symbol(s) => {
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
                    self.members_within(info, name, within)
                }
            },
            Type::Alias(a) | Type::Bounds(_, a) => self.members_within(a, name, within),
            Type::Applied(..) if self.u.dealias(t) != t => {
                let dealiased = self.u.dealias(t);
                self.members_within(dealiased, name, within)
            }
            Type::And(a, b) => {
                let (ma, mb) = (self.members_within(a, name, within), self.members_within(b, name, within));
                self.and_members(t, (ma, mb), ctx)
            }
            Type::Or(..) => match self.u.join(t) {
                Some(j) if !matches!(self.u.types().get(j), Type::Or(..)) => {
                    self.members_within(j, name, within)
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
    pub fn instantiated_selected(&mut self, t: TypeId) -> TypeId {
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
            self.u.symbol(p).decls.iter().copied().filter(|&d| self.is_named_package_object(d)).collect();
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
        let classes = self.completed_linearization(c);
        if name == "<init>" {
            return self.u.decl(c, name).unwrap_or_default();
        }
        self.members_of_bases(c, &classes, name)
    }

    /// The members named `name` that `super` selects in class `c`: those of its base classes but
    /// `c`.
    fn super_members(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        let classes = self.completed_linearization(c);
        self.members_of_bases(c, &classes[1..], name)
    }

    /// The linearization of class `c`, `c` and its base classes without TASTy completed.
    fn completed_linearization(&mut self, c: SymbolId) -> Vec<SymbolId> {
        self.complete(c);
        let classes = self.u.linearization(c);
        for &base in classes.iter().skip(1) {
            if self.u.symbol(base).tree.is_none() {
                self.complete(base);
            }
        }
        classes
    }

    /// The members named `name` of `classes`, base classes of `c` in linearization order.
    fn members_of_bases(&mut self, c: SymbolId, classes: &[SymbolId], name: &str) -> Vec<SymbolId> {
        let site = crate::defn::type_ref(self.u, c);
        let mut out: Vec<SymbolId> = Vec::new();
        for &base in classes {
            let decls: Vec<SymbolId> = self.u.decl(base, name).unwrap_or_default();
            let decls: Vec<SymbolId> = decls
                .into_iter()
                .filter(|&m| base == c || !self.u.symbol(m).flags.has(tasty::tags::PRIVATE))
                .collect();
            for m in decls {
                if self.u.is_type_symbol(m) {
                    if !out.iter().any(|&o| self.u.is_type_symbol(o)) {
                        out.push(m);
                    }
                    continue;
                }
                let mut overridden = None;
                for (k, &o) in out.iter().enumerate() {
                    if !self.u.is_type_symbol(o) && self.same_member(site, o, m) {
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
