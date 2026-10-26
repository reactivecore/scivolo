// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/InlineReducer.scala
//! The case an inline match selects, its scrutinee bound once, its guard folded and its sub
//! match reduced (dotty's `reduceInlineMatch`, `reduceCase`).

use super::Reducer;
use crate::inlining::Unsupported;
use crate::inlining::copy::TreeTypeMap;
use crate::tpd::{MatchKind, TNodeId, Tree as T};
use syntax::ast::constant::Constant as C;
use tasty::{SymbolId, Type, TypeId};

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
        self.reduce_cases(binding, scrut, cases)
    }

    /// The bindings and the body of the first of `cases` that the scrutinee `scrut`, bound by
    /// `binding` if it needs to be, matches.
    pub fn reduce_cases(
        &mut self,
        binding: Option<TNodeId>,
        scrut: TypeId,
        cases: &[TNodeId],
    ) -> Result<Option<(Vec<TNodeId>, TNodeId)>, Unsupported> {
        for &c in cases {
            match self.reduce_case(c, scrut, binding)? {
                Case::NoMatch => {}
                Case::Selected(bindings, rhs) => return self.without_unusable(bindings, rhs).map(Some),
                Case::Unknown => return Ok(None),
            }
        }
        Ok(None)
    }

    fn reduce_case(
        &mut self,
        c: TNodeId,
        scrut: TypeId,
        binding: Option<TNodeId>,
    ) -> Result<Case, Unsupported> {
        let T::CaseDef { pat, guard, body } = *self.i.tpd.tree(c) else { return Ok(Case::NoMatch) };
        let mut map: Bindings = binding.map(|b| (None, b)).into_iter().collect();
        let mark = self.type_bindings.len();
        if !self.reduce_pattern(&mut map, scrut, pat)? {
            self.type_bindings.truncate(mark);
            return Ok(Case::NoMatch);
        }
        let mut m =
            TreeTypeMap { refs: self.type_bindings.drain(mark..).collect(), ..TreeTypeMap::default() };
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
            let mark = self.with_givens(&bindings);
            let g = self.i.retype(g, self.args, self.owner);
            self.i.case_givens.truncate(mark);
            let g = g?;
            match self.i.constant_of(g) {
                Some(C::Boolean(true)) => {}
                Some(C::Boolean(false)) => return Ok(Case::NoMatch),
                _ => return Ok(Case::Unknown),
            }
        }
        self.subst(&m, body);
        let T::Match { selector: Some(selector), cases, kind: MatchKind::Sub } = *self.i.tpd.tree(body)
        else {
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
        if let T::TypeApply { args, .. } = *self.i.tpd.tree(t) {
            for a in self.i.tpd.items(args).to_vec() {
                if let T::Ident { .. } = self.i.tpd.tree(a)
                    && self
                        .i
                        .tpd
                        .sym(a)
                        .is_some_and(|s| m.refs.contains_key(&s) && self.i.u.is_type_symbol(s))
                {
                    let node = self.i.tpd.get_mut(a);
                    node.tree = T::TypeTree { inferred: false };
                    node.sym = None;
                }
            }
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
}
