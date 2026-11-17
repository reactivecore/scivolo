//! Completion of the unit's classes and type members before a base class test, a conformance
//! test or an erasure.

use super::Typer;
use crate::symbols::Completion;
use rustc_hash::FxHashSet;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether `t` derives from class `class`, given by its full name, the unit's classes among
    /// its base classes completed first.
    pub fn derives_from_named(&mut self, t: TypeId, class: &str) -> bool {
        let base = self.resolve_full_name(class, true);
        let t = self.u.widen(t);
        match (self.u.class_symbol(t), base) {
            (Some(c), Some(b)) => {
                self.complete_bases(c);
                self.u.derives_from(c, b)
            }
            _ => false,
        }
    }

    /// Completes the unit's classes and type members not yet completed, members of packages and
    /// classes, that `a` or `b` reach through the classes, types and values they refer to, those
    /// classes' parents and the others' infos, as a library type may reach them, unless a
    /// definition is being completed; whether there were any.
    pub fn complete_reached_unit_classes(&mut self, a: TypeId, b: TypeId) -> bool {
        if self.syms.completions.values().any(|c| matches!(c, Completion::Completing(_))) {
            return false;
        }
        let mut any = false;
        let mut attempted = FxHashSet::default();
        loop {
            let mut pending = self.reached_pending_unit_classes(a, b);
            pending.retain(|&s| attempted.insert(s));
            if pending.is_empty() {
                return any;
            }
            pending.sort_by_key(|s| s.0);
            pending.iter().for_each(|&s| self.complete(s));
            any = true;
        }
    }

    /// The unit's classes and type members not completed yet that `a` or `b` reach through
    /// completed definitions.
    fn reached_pending_unit_classes(&mut self, a: TypeId, b: TypeId) -> Vec<SymbolId> {
        let mut seen = FxHashSet::default();
        let mut todo = self.referred_types(a);
        todo.extend(self.referred_types(b));
        let mut pending = Vec::new();
        while let Some(s) = todo.pop() {
            if !seen.insert(s) {
                continue;
            }
            if self.is_pending_unit_class(s) {
                pending.push(s);
                continue;
            }
            match self.u.symbol(s).kind {
                Kind::Class => {
                    for p in self.u.parents(s) {
                        todo.extend(self.referred_types(p));
                    }
                }
                Kind::Type => {
                    let info = self.u.info(s);
                    todo.extend(self.referred_types(info));
                }
                Kind::Val if matches!(self.syms.completions.get(&s), None | Some(Completion::Done)) => {
                    let info = self.u.info(s);
                    todo.extend(self.referred_types(info));
                }
                _ => {}
            }
        }
        pending
    }

    /// The classes, types and values `t` refers to.
    fn referred_types(&mut self, t: TypeId) -> Vec<SymbolId> {
        let mut out = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            let s = u.sym_of(part).or_else(|| u.resolve_ref(part));
            if let Some(s) = s.filter(|&s| matches!(u.symbol(s).kind, Kind::Class | Kind::Type | Kind::Val)) {
                out.push(s);
            }
            None
        });
        out
    }

    /// Whether `s` is a class or type member of the unit not completed yet, outside a method.
    fn is_pending_unit_class(&self, s: SymbolId) -> bool {
        matches!(self.syms.completions.get(&s), Some(Completion::Pending { .. }))
            && matches!(self.u.symbol(s).kind, Kind::Class | Kind::Type)
            && self.u.symbol(s).owner.is_some_and(|o| self.u.symbol(o).kind != Kind::Def)
    }

    /// Completes the classes of the parameter and result types of method `def` and the underlying
    /// values of those that are value classes, which its erasure reads (dotty's
    /// `isDerivedValueClass` forces the class's info).
    pub fn complete_erased_classes(&mut self, def: SymbolId) {
        let mut t = self.u.info(def);
        let mut types = Vec::new();
        loop {
            match *self.u.ty(t) {
                Type::Poly { result, .. } => t = result,
                Type::Method { params, result, .. } => {
                    types.extend(self.u.params_of(params).iter().map(|&(_, p)| p));
                    t = result;
                }
                _ => break types.push(t),
            }
        }
        for t in types {
            let widened = self.u.widen(t);
            let Some(c) = self.u.class_symbol(widened) else { continue };
            self.complete(c);
            if let Some(value) = self.u.underlying(c) {
                self.complete(value);
            }
        }
    }

    /// Completes the unit's type members not yet completed that `a` or `b` refer to by name, as
    /// library types do, or select from a prefix whose class overrides them, as `p.T` for
    /// `p: C` and `C`'s `type T = D`; whether there were any.
    pub fn complete_named_type_members(&mut self, a: TypeId, b: TypeId) -> bool {
        let mut named = Vec::new();
        for t in [a, b] {
            self.u.map_type(t, &mut |u, part| {
                if matches!(u.ty(part), Type::TypeRef(..) | Type::Sym(Some(_), _)) {
                    named.push(part);
                }
                None
            });
        }
        let mut any = false;
        for part in named {
            let s = match *self.u.ty(part) {
                Type::Sym(Some(prefix), s) => self.overriding_type_member(prefix, s),
                _ => self.u.resolve_ref(part),
            };
            let Some(s) = s else { continue };
            if self.u.symbol(s).kind == Kind::Type
                && matches!(self.syms.completions.get(&s), Some(Completion::Pending { .. }))
            {
                self.complete(s);
                any = true;
            }
        }
        any
    }

    /// The type member named as type member `s` of the class of `prefix`'s underlying type.
    fn overriding_type_member(&mut self, prefix: TypeId, s: SymbolId) -> Option<SymbolId> {
        if self.u.symbol(s).kind != Kind::Type {
            return None;
        }
        let widened = self.u.widen(prefix);
        let class = self.u.class_symbol(widened)?;
        let name = self.u.symbol(s).name.clone();
        self.u.member_where(class, &name, |u, m| u.symbol(m).kind == Kind::Type).first().copied()
    }

    /// Completes class `c` and the unit's classes among its base classes.
    fn complete_bases(&mut self, c: SymbolId) {
        loop {
            let pending: Vec<SymbolId> = std::iter::once(c)
                .chain(self.u.linearization(c))
                .filter(|s| matches!(self.syms.completions.get(s), Some(Completion::Pending { .. })))
                .collect();
            if pending.is_empty() {
                return;
            }
            pending.into_iter().for_each(|s| self.complete(s));
        }
    }
}
