// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/InlineReducer.scala
//! Patterns matched against a scrutinee known at the inline call (dotty's `reducePattern`).

use super::Reducer;
use super::cases::Bindings;
use crate::inlining::Unsupported;
use crate::inlining::quote_match::{bind_variances, bound_types};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::tags::{CASE, ENUM, SYNTHETIC};
use tasty::{Binding, Symbol, SymbolId, Type, TypeId};

impl Reducer<'_, '_> {
    /// Whether pattern `pat` matches the value `scrut` refers to; the bindings of its variables
    /// and parts added to `map`.
    pub fn reduce_pattern(
        &mut self,
        map: &mut Bindings,
        scrut: TypeId,
        pat: TNodeId,
    ) -> Result<bool, Unsupported> {
        let span = self.i.tpd.span(pat);
        Ok(match self.i.tpd.tree(pat).clone() {
            T::Typed { expr, tpt } => {
                let extractor = self.extractor_binds(expr);
                let unapply = matches!(self.i.tpd.tree(expr), T::UnApply { .. });
                if extractor.is_none() && !unapply && self.binds_type(expr) {
                    return Err((span, "type binding in an inline match pattern"));
                }
                let tpe = self.i.tpd.tpe(tpt);
                let mut binds = self.type_binds(tpt);
                for b in extractor.unwrap_or_default() {
                    let mut occurs = false;
                    self.i.u.map_type(tpe, &mut |u, part| {
                        occurs |= matches!(*u.types().get(part), Type::Sym(None, s) if s == b);
                        None
                    });
                    if occurs && !binds.contains(&b) {
                        binds.push(b);
                    }
                }
                let binds = bind_variances(self.i.u, tpe, &binds);
                let conforms = match binds.is_empty() {
                    true => self.i.u.is_subtype(scrut, tpe),
                    false => match bound_types(self.i.u, scrut, tpe, &binds) {
                        Some(types) => {
                            self.type_bindings.extend(binds.iter().map(|&(s, _)| s).zip(types));
                            true
                        }
                        None => false,
                    },
                };
                conforms && self.reduce_pattern(map, scrut, expr)?
            }
            T::Bind { name, .. } if name.is_type => {
                return Err((span, "type binding in an inline match pattern"));
            }
            T::Bind { body, .. } if self.implicit => {
                let (&T::Typed { tpt, .. }, Some(s)) = (self.i.tpd.tree(body), self.i.tpd.sym(pat)) else {
                    return Err((span, "summonFrom pattern"));
                };
                if self.binds_type(body) {
                    return Err((span, "type binding in a summonFrom pattern"));
                }
                let (tpe, at) = (self.i.tpd.tpe(tpt), self.i.tpd.span(tpt).start_pos());
                let Some(evidence) = self.i.summon(tpe, at)? else { return Ok(false) };
                self.new_term_binding(map, s, evidence, Span::at(span.point()));
                true
            }
            T::Bind { name, body } => {
                if !self.reduce_pattern(map, scrut, body)? {
                    return Ok(false);
                }
                if self.i.names.text(name.name) != "_"
                    && let Some(s) = self.i.tpd.sym(pat)
                {
                    let at = Span::at(span.point());
                    let rhs = self.reference(scrut, at)?;
                    self.new_term_binding(map, s, rhs, at);
                }
                true
            }
            T::Ident { name } if self.i.names.text(name.name) == "_" => true,
            T::Literal { .. } => {
                let widened = self.widen_term_ref(scrut);
                let tpe = self.i.tpd.tpe(pat);
                self.same_type(widened, tpe)
            }
            T::Ident { .. } | T::Select { .. } => {
                let tpe = self.i.tpd.tpe(pat);
                let module = self.i.u.class_symbol(scrut).is_some_and(|c| self.i.u.is_module_class(c));
                let prefix = match *self.i.u.types().get(scrut) {
                    Type::Sym(Some(p), _) => Some(p),
                    _ => None,
                };
                let singleton_prefix = prefix.is_none_or(|p| self.is_singleton(p));
                self.same_type(scrut, tpe)
                    || module && singleton_prefix && {
                        let (a, b) = (self.i.u.widen(scrut), self.i.u.widen(tpe));
                        self.same_type(a, b)
                    }
            }
            T::UnApply { fun, patterns, .. } => {
                let pats = self.i.tpd.items(patterns).to_vec();
                self.reduce_unapply(map, scrut, fun, &pats)?
            }
            T::Alternative { trees } => {
                for alt in self.i.tpd.items(trees).to_vec() {
                    if self.reduce_pattern(map, scrut, alt)? {
                        return Ok(true);
                    }
                }
                false
            }
            T::Inlined { call: None, expansion, .. } => self.reduce_pattern(map, scrut, expansion)?,
            _ => false,
        })
    }

    /// Whether extractor `fun` with `pats` matches `scrut`: a case class's synthetic `unapply`, its
    /// fields bound and matched one by one.
    fn reduce_unapply(
        &mut self,
        map: &mut Bindings,
        scrut: TypeId,
        fun: TNodeId,
        pats: &[TNodeId],
    ) -> Result<bool, Unsupported> {
        let fun_tpe = self.i.tpd.tpe(fun);
        let widened = self.i.u.widen(fun_tpe);
        let Some(Binding { params, .. }) = self.i.u.method(widened) else { return Ok(false) };
        let [(_, param)] = params[..] else { return Ok(false) };
        let param = self.with_type_bindings(param);
        let unapp = self.i.method_part(fun).and_then(|f| self.i.tpd.sym(f));
        let synthetic = unapp.is_some_and(|s| self.i.u.symbol(s).flags.has(SYNTHETIC));
        let Some(cls) = self.i.u.class_symbol(param).filter(|&c| self.i.u.symbol(c).flags.has(CASE)) else {
            return Ok(false);
        };
        if !synthetic || !self.i.u.is_subtype(scrut, param) {
            return Ok(false);
        }
        let at = Span::at(self.i.call_span.point());
        let mut selectors = Vec::new();
        for a in self.param_accessors(cls) {
            let qual = self.reference(scrut, at)?;
            let tpe = self.i.u.intern_type(Type::Sym(Some(scrut), a));
            let name = TName { name: self.i.names.intern(&self.i.u.symbol(a).name.clone()), is_type: false };
            let select = self.i.tpd.node(T::Select { qualifier: qual, name }, at, Some(tpe), Some(a));
            let reduced = self.reduce_projection(select)?;
            selectors.push(self.i.const_to_literal(reduced));
        }
        if selectors.len() != pats.len() {
            return Ok(false);
        }
        for (&pat, selector) in pats.iter().zip(selectors) {
            let tpe = self.i.tpd.tpe(selector);
            let info = self.widen_inline_scrutinee(tpe);
            let elem = self.new_val("$proxy", info);
            self.adjust_erased(elem, selector);
            let rhs = self.i.const_to_literal(selector);
            self.rhs_of.insert(elem, rhs);
            let def = self.val_def(elem, rhs, at);
            map.push((None, def));
            let elem_ref = self.i.u.intern_type(Type::Sym(None, elem));
            if !self.reduce_pattern(map, elem_ref, pat)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Binds a copy of pattern variable `s` to `rhs` at `at`.
    fn new_term_binding(&mut self, map: &mut Bindings, s: SymbolId, rhs: TNodeId, at: Span) {
        let tpe = self.i.tpd.tpe(rhs);
        let info = self.widen_inline_scrutinee(tpe);
        let old = self.i.u.symbol(s);
        let copied = self.i.u.define(Symbol {
            name: old.name.clone(),
            kind: old.kind,
            owner: old.owner,
            flags: old.flags.without(CASE),
            target: None,
            is_extern: false,
            derived: old.derived,
            decls: Vec::new(),
            tree: None,
        });
        self.i.u.set_info(copied, info);
        self.adjust_erased(copied, rhs);
        let rhs = self.i.const_to_literal(rhs);
        let def = self.val_def(copied, rhs, at);
        map.push((Some(s), def));
    }

    /// A reference to the term `t` refers to, at `at` (dotty's `ref`).
    fn reference(&mut self, t: TypeId, at: Span) -> Result<TNodeId, Unsupported> {
        self.i.singleton(t, at).ok_or((at, "inline match on a scrutinee without a path"))
    }

    /// `t` widened through references to terms, not to modules and enum cases (dotty's
    /// `widenInlineScrutinee`).
    fn widen_inline_scrutinee(&mut self, mut t: TypeId) -> TypeId {
        while let Type::Sym(_, s) = *self.i.u.types().get(t) {
            let sym = self.i.u.symbol(s);
            let enum_case = sym.flags.has(ENUM) && sym.flags.has(CASE);
            if !sym.kind.is_term() || enum_case || self.i.u.is_module(s) {
                return t;
            }
            let Some(next) = self.i.u.widen_once(t) else { return t };
            t = match *self.i.u.types().get(next) {
                Type::ByName(r) => r,
                _ => next,
            };
        }
        t
    }

    /// `t` widened through references to terms (dotty's `widenTermRefExpr`).
    fn widen_term_ref(&mut self, mut t: TypeId) -> TypeId {
        while let Type::Sym(_, s) = *self.i.u.types().get(t) {
            if !self.i.u.symbol(s).kind.is_term() {
                return t;
            }
            let Some(next) = self.i.u.widen_once(t) else { return t };
            t = match *self.i.u.types().get(next) {
                Type::ByName(r) => r,
                _ => next,
            };
        }
        t
    }

    /// Whether `t` is a singleton type: a reference to a term, `this`, a constant or a package.
    fn is_singleton(&mut self, t: TypeId) -> bool {
        match *self.i.u.types().get(t) {
            Type::Sym(_, s) => self.i.u.symbol(s).kind.is_term(),
            Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::Package(_) | Type::Skolem(..) => true,
            _ => false,
        }
    }

    fn same_type(&mut self, a: TypeId, b: TypeId) -> bool {
        self.i.u.is_subtype(a, b) && self.i.u.is_subtype(b, a)
    }

    /// The type variables extractor pattern `t` binds in its type arguments, if its argument
    /// patterns bind none.
    fn extractor_binds(&self, t: TNodeId) -> Option<Vec<SymbolId>> {
        let T::UnApply { fun, patterns, .. } = *self.i.tpd.tree(t) else { return None };
        let pats = self.i.tpd.items(patterns);
        (!pats.iter().any(|&p| self.binds_type(p))).then(|| self.type_binds(fun))
    }

    /// `t` with the type variables bound so far replaced by their types.
    fn with_type_bindings(&mut self, t: TypeId) -> TypeId {
        let bindings = self.type_bindings.clone();
        self.i.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => bindings.iter().find(|&&(b, _)| b == s).map(|&(_, to)| to),
            _ => None,
        })
    }

    /// Whether pattern `t` binds a type variable.
    fn binds_type(&self, t: TNodeId) -> bool {
        matches!(*self.i.tpd.tree(t), T::Bind { name, .. } if name.is_type)
            || self.i.tpd.children(t).into_iter().any(|c| self.binds_type(c))
    }

    /// The type variables type tree `t` binds.
    fn type_binds(&self, t: TNodeId) -> Vec<SymbolId> {
        let own = match *self.i.tpd.tree(t) {
            T::Bind { name, .. } if name.is_type => self.i.tpd.sym(t),
            _ => None,
        };
        let inner = self.i.tpd.children(t).into_iter().flat_map(|c| self.type_binds(c));
        own.into_iter().chain(inner).collect()
    }
}
