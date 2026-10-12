//! The case an inline match selects, its scrutinee bound once, its guard folded and its sub
//! match reduced (dotty's `reduceInlineMatch`, `reduceCase`).

use super::Reducer;
use crate::inlining::Unsupported;
use crate::inlining::copy::TreeTypeMap;
use crate::tpd::{MatchKind, TNodeId, Tree as T};
use syntax::ast::constant::Constant as C;
use tasty::tags::{ERASED, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

/// The bindings of a pattern's variables and parts, each with the pattern variable it replaces.
pub type Bindings = Vec<(Option<SymbolId>, TNodeId)>;

/// What a case makes of the scrutinee.
enum Case {
    NoMatch,
    Selected(Vec<TNodeId>, TNodeId),
    /// The guard does not fold to a constant.
    Unknown,
}

impl Reducer<'_, '_> {
    /// The bindings and the body of the first of `cases` that `scrutinee` of type `scrut_type`
    /// matches; `None` if none does or a guard does not fold before.
    pub fn reduce(
        &mut self,
        scrutinee: TNodeId,
        scrut_type: TypeId,
        cases: &[TNodeId],
    ) -> Result<Option<(Vec<TNodeId>, TNodeId)>, Unsupported> {
        let tpe = self.i.tpd.tpe(scrutinee);
        let local =
            matches!(*self.i.u.types().get(tpe), Type::Sym(None, s) if self.i.u.symbol(s).kind.is_term());
        let (scrut, binding) = if local && self.i.is_pure_expr(scrutinee) {
            (tpe, None)
        } else {
            let s = self.new_val("$scrutinee", scrut_type);
            if self.refers_to_erased(scrutinee) {
                self.erase(s);
                if !self.i.is_pure_expr(scrutinee) {
                    self.unusable.insert(s);
                }
            }
            let b = self.normalize_binding(s, scrutinee)?;
            (self.i.u.intern_type(Type::Sym(None, s)), Some(b))
        };
        for &c in cases {
            match self.reduce_case(c, scrut, binding)? {
                Case::NoMatch => {}
                Case::Selected(bindings, rhs) => return self.without_unusable(bindings, rhs).map(Some),
                Case::Unknown => return Ok(None),
            }
        }
        Ok(None)
    }

    /// `bindings` without those of unusable values, and `rhs`; `Err` if a reference to one
    /// remains.
    fn without_unusable(
        &mut self,
        bindings: Vec<TNodeId>,
        rhs: TNodeId,
    ) -> Result<(Vec<TNodeId>, TNodeId), Unsupported> {
        let kept: Vec<TNodeId> = bindings
            .into_iter()
            .filter(|&b| self.i.tpd.sym(b).is_none_or(|s| !self.unusable.contains(&s)))
            .collect();
        for &t in kept.iter().chain([&rhs]) {
            if let Some(r) = self.unusable_ref_in(t) {
                return Err((self.i.tpd.span(r), "reference to an erased scrutinee of an inline match"));
            }
        }
        Ok((kept, rhs))
    }

    fn unusable_ref_in(&self, t: TNodeId) -> Option<TNodeId> {
        if matches!(self.i.tpd.tree(t), T::Ident { .. })
            && self.i.tpd.sym(t).is_some_and(|s| self.unusable.contains(&s))
        {
            return Some(t);
        }
        self.i.tpd.children(t).into_iter().find_map(|c| self.unusable_ref_in(c))
    }

    /// Flags `s` erased.
    pub fn erase(&mut self, s: SymbolId) {
        let flags = self.i.u.symbol(s).flags.with(ERASED);
        self.i.u.set_flags(s, flags);
    }

    /// Flags `s`, defined by `rhs`, erased if `rhs` refers to an erased definition, and unusable
    /// if to an unusable one (dotty's `adjustErased`).
    pub fn adjust_erased(&mut self, s: SymbolId, rhs: TNodeId) {
        if let T::Ident { .. } = self.i.tpd.tree(rhs)
            && let Some(r) = self.i.tpd.sym(rhs)
            && self.i.is_erased(r)
        {
            self.erase(s);
            if self.unusable.contains(&r) {
                self.unusable.insert(s);
            }
        }
        for c in self.i.tpd.children(rhs) {
            self.adjust_erased(s, c);
        }
    }

    fn reduce_case(
        &mut self,
        c: TNodeId,
        scrut: TypeId,
        binding: Option<TNodeId>,
    ) -> Result<Case, Unsupported> {
        let T::CaseDef { pat, guard, body } = *self.i.tpd.tree(c) else { return Ok(Case::NoMatch) };
        let mut map: Bindings = binding.map(|b| (None, b)).into_iter().collect();
        if !self.reduce_pattern(&mut map, scrut, pat)? {
            return Ok(Case::NoMatch);
        }
        let mut m = TreeTypeMap::default();
        for &(from, b) in &map {
            if let (Some(from), Some(to)) = (from, self.i.tpd.sym(b)) {
                m.syms.insert(from, to);
            }
        }
        for &to in m.syms.clone().values() {
            let info = self.i.u.info(to);
            let info = self.i.map_type(&m, info);
            self.i.u.set_info(to, info);
        }
        let bindings: Vec<TNodeId> = map.iter().map(|&(_, b)| b).collect();
        for &b in &bindings {
            self.subst(&m, b);
        }
        if let Some(g) = guard {
            self.subst(&m, g);
            let g = self.i.retype(g, self.args, self.owner)?;
            match self.i.constant_of(g) {
                Some(C::Boolean(true)) => {}
                Some(C::Boolean(false)) => return Ok(Case::NoMatch),
                _ => return Ok(Case::Unknown),
            }
        }
        self.subst(&m, body);
        let T::Match { selector, cases, kind: MatchKind::Sub } = *self.i.tpd.tree(body) else {
            return Ok(Case::Selected(bindings, body));
        };
        let cases = self.i.tpd.items(cases).to_vec();
        let sel_type = self.i.tpd.tpe(selector);
        Ok(match self.reduce(selector, sel_type, &cases)? {
            Some((sub, rhs)) => Case::Selected([bindings, sub].concat(), rhs),
            None => Case::NoMatch,
        })
    }

    /// Replaces in `t` the symbols of `m` and the references to them in types, in place, and
    /// folds the operations on constants this makes (dotty's `subst`).
    fn subst(&mut self, m: &TreeTypeMap, t: TNodeId) {
        if let Some(tpe) = self.i.tpd.get(t).tpe {
            let tpe = self.i.map_type(m, tpe);
            self.i.tpd.get_mut(t).tpe = Some(tpe);
        }
        if let Some(&to) = self.i.tpd.sym(t).and_then(|s| m.syms.get(&s)) {
            self.i.tpd.get_mut(t).sym = Some(to);
        }
        for c in self.i.tpd.children(t) {
            self.subst(m, c);
        }
        self.i.fold_constants(t);
        if let T::Block { stats: defs, expr } | T::Inlined { bindings: defs, expansion: expr, .. } =
            *self.i.tpd.tree(t)
        {
            let locals: Vec<SymbolId> =
                self.i.tpd.items(defs).iter().filter_map(|&d| self.i.tpd.sym(d)).collect();
            let tpe = self.i.tpd.tpe(expr);
            if let Some(tpe) = self.i.avoid(tpe, &locals) {
                self.i.tpd.get_mut(t).tpe = Some(tpe);
            }
        }
    }

    /// A new synthetic value named `prefix` and a fresh number, of type `info`, owned by the
    /// call's owner.
    pub fn new_val(&mut self, prefix: &str, info: TypeId) -> SymbolId {
        let name = self.i.fresh.next(self.i.names, prefix);
        let s = self.i.u.define(Symbol {
            name: self.i.names.text(name).to_owned(),
            kind: Kind::Val,
            owner: Some(self.owner),
            flags: Flags::default().with(SYNTHETIC),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.i.u.set_info(s, info);
        s
    }

    fn refers_to_erased(&self, t: TNodeId) -> bool {
        self.i.tpd.sym(t).is_some_and(|s| self.i.is_erased(s))
            || self.i.tpd.children(t).into_iter().any(|c| self.refers_to_erased(c))
    }
}
