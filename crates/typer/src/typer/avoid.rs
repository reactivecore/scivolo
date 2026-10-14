//! Types without references to local symbols (dotty's `ensureNoLocalRefs`, `TypeOps.avoid`).

use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether type `t` refers to one of `syms`.
    pub fn refers_to(&mut self, t: TypeId, syms: &[SymbolId]) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = u.types().get(part)
                && syms.contains(s)
            {
                found = true;
            }
            None
        });
        found
    }

    /// `{ defs; tree }`, or `tree` if there are no definitions (dotty's `wrapDefs`), its type
    /// widened if it refers to the definitions, and classes selected on them selected on their
    /// widened types (dotty's `avoid`); outside the subset if it still refers to them.
    pub fn wrap_defs(&mut self, defs: Vec<TNodeId>, tree: TNodeId) -> TNodeId {
        if defs.is_empty() {
            return tree;
        }
        let span = defs.iter().fold(self.tpd.span(tree), |s, &d| s.union(self.tpd.span(d)));
        let span = syntax::ast::Span::synthetic(span.start(), span.end());
        let syms: Vec<SymbolId> = defs.iter().filter_map(|&d| self.tpd.sym(d)).collect();
        let tpe = self.tpd.get(tree).tpe.map(|t| if self.refers_to(t, &syms) { self.u.widen(t) } else { t });
        let tpe = tpe.map(|t| {
            self.u.map_type(t, &mut |u, part| {
                let Type::Sym(Some(pre), s) = *u.types().get(part) else { return None };
                let local = matches!(*u.types().get(pre), Type::Sym(_, p) if syms.contains(&p));
                if !local || u.symbol(s).kind != tasty::Kind::Class {
                    return None;
                }
                let widened = u.widen(pre);
                Some(u.intern_type(Type::Sym(Some(widened), s)))
            })
        });
        if tpe.is_some_and(|t| self.refers_to(t, &syms)) {
            self.unsupported_at(span, "type of a lifted call referring to a lifted value");
        }
        let stats = self.tpd.list(&defs);
        self.tpd.node(T::Block { stats, expr: tree }, span, tpe, None)
    }

    /// Block result `expr`, ascribed a type without references to `locals` if its own has some
    /// (dotty's `ensureNoLocalRefs`): its references to local terms widened, or `pt` if that
    /// does not conform to `pt`; `None` if a reference remains.
    pub fn no_local_refs(&mut self, expr: TNodeId, locals: &[SymbolId], pt: Pt) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(expr);
        let tpe = self.zonk(tpe);
        if !self.escapes(tpe, locals) {
            return Some(expr);
        }
        let mut avoided = self.u.map_type(tpe, &mut |u, part| {
            let pre = match u.types().get(part) {
                Type::Sym(Some(pre), _) | Type::TypeRef(pre, _) => *pre,
                _ => return None,
            };
            let mut local = false;
            u.map_type(pre, &mut |u, q| {
                local |= matches!(*u.types().get(q), Type::Sym(_, l) if locals.contains(&l));
                None
            });
            let dealiased = u.dealias(part);
            (local && dealiased != part).then_some(dealiased)
        });
        let mut checked = avoided != tpe;
        for _ in 0..16 {
            let Type::Sym(_, s) = *self.u.types().get(avoided) else { break };
            if locals.contains(&s) && self.u.symbol(s).kind == tasty::Kind::Class {
                let object = self.u.builtins().object;
                let parents = self.u.parents(s);
                let proper = parents.iter().filter(|&&p| self.u.class_symbol(p) != Some(object)).count();
                avoided = parents.into_iter().fold(tasty::ANY, |acc, p| self.u.glb(acc, p));
                if proper > 2 || self.needs_refinement(s, avoided) {
                    return None;
                }
                checked = true;
                break;
            }
            if self.u.is_type_symbol(s) {
                break;
            }
            let pre = match *self.u.types().get(avoided) {
                Type::Sym(pre, _) => pre,
                _ => None,
            };
            let prefix = pre.filter(|&p| !locals.contains(&s) && self.refers_to(p, locals));
            if !locals.contains(&s) && prefix.is_none() {
                break;
            }
            let info = match prefix {
                Some(pre) => self.u.member_info(pre, s),
                None => self.u.info(s),
            };
            let info = match *self.u.types().get(info) {
                Type::Bounds(_, hi) => hi,
                _ => info,
            };
            let info = match *self.u.types().get(info) {
                Type::ByName(r) => r,
                _ => info,
            };
            avoided = if prefix.is_some() { info } else { self.u.dealias(info) };
        }
        if let Pt::Type(p) = pt
            && let p = self.zonk(p)
            && self.is_fully_defined(p)
            && !self.u.is_subtype(avoided, p)
        {
            (avoided, checked) = (p, true);
        }
        if self.refers_to(avoided, locals) || checked && !self.u.is_subtype(tpe, avoided) {
            return None;
        }
        Some(self.ascribed(expr, avoided))
    }

    /// Whether local class `c` with parents `parent` declares a member that dotty's `classBound`
    /// may add as a refinement: a type member, a member narrowing an inherited one or polymorphic
    /// and overriding one, or a member of a `Selectable`.
    fn needs_refinement(&mut self, c: SymbolId, parent: TypeId) -> bool {
        let selectable =
            self.resolve_full_name("scala.Selectable", true).is_some_and(|s| self.u.derives_from(c, s));
        let site = crate::defn::type_ref(self.u, c);
        for d in self.u.symbol(c).decls.clone() {
            let sym = self.u.symbol(d);
            if sym.flags.has(tasty::tags::PRIVATE) || sym.name == "<init>" || sym.kind == tasty::Kind::Class {
                continue;
            }
            if self.u.is_type_symbol(d) {
                return true;
            }
            let name = sym.name.clone();
            let info = self.u.info(d);
            if matches!(self.u.types().get(info), Type::Poly { .. })
                && !self.type_members(parent, &name).is_empty()
            {
                return true;
            }
            let inherited: Vec<SymbolId> = self
                .type_members(parent, &name)
                .into_iter()
                .filter(|&m| !self.u.is_type_symbol(m) && self.same_member(site, m, d))
                .collect();
            if inherited.is_empty() {
                if selectable {
                    return true;
                }
                continue;
            }
            let own = self.final_result(info);
            for m in inherited {
                let info = self.u.member_info(parent, m);
                let theirs = self.final_result(info);
                if self.u.is_subtype(own, theirs) && !self.u.is_subtype(theirs, own) {
                    return true;
                }
            }
        }
        false
    }

    /// The result of method type `t` after all its clauses.
    pub fn final_result(&mut self, t: TypeId) -> TypeId {
        match *self.u.types().get(t) {
            Type::Method { result, .. } | Type::Poly { result, .. } => self.final_result(result),
            Type::ByName(r) => r,
            _ => t,
        }
    }

    /// `expr: tpe`, the ascription moved into the result of a block (dotty's `ascribeType`).
    fn ascribed(&mut self, expr: TNodeId, tpe: TypeId) -> TNodeId {
        let span = self.tpd.span(expr);
        if let T::Block { stats, expr: last } = *self.tpd.tree(expr)
            && !matches!(self.tpd.tree(last), T::Closure { .. })
        {
            let last = self.ascribed(last, tpe);
            return self.build().block(stats, last, span);
        }
        let tpe = self.simplified(tpe);
        let tpt = self.type_tree(tpe, span.end_pos(), true);
        self.tpd.node(T::Typed { expr, tpt }, span.to_synthetic(), Some(tpe), None)
    }
}
