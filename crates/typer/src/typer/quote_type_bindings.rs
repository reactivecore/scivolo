// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/QuotesAndSplices.scala
//! The type variables of quote patterns (dotty's `typedQuotedTypeVar` and the
//! `extractTypeBindings` of `typedQuotePattern`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags::{CASE, SYNTHETIC};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{ANY, Kind, NOTHING, Type, TypeId};

impl Typer<'_> {
    /// The name of `id` if it is a type variable of the quote pattern being typed in `ctx`: a
    /// lower-case type name outside the pattern's splices.
    pub fn quoted_type_var(&self, id: NodeId, ctx: Ctx) -> Option<TName> {
        let Tree::Ident { name } = *self.ast.tree(id) else { return None };
        let quoted = !ctx.mode.is(Mode::PATTERN) && ctx.mode.is(Mode::QUOTED_PATTERN);
        (quoted && name.is_type && self.is_var_pattern(id, self.text(name.name))).then_some(name)
    }

    /// Type variable `name` at `id` in a quote pattern, a bind of a new type of bounds `pt` unless
    /// the pattern bound it before.
    pub fn typed_quoted_type_var(&mut self, id: NodeId, name: TName, pt: Pt) -> TNodeId {
        let text = self.text(name.name).to_owned();
        let span = self.ast.span(id);
        let Some(scope) = self.quote_pattern.as_ref() else {
            return self.unsupported(id, "quoted type variable");
        };
        let (outer, before) = (scope.outer, scope.before);
        if let Some(&(_, sym)) = scope.type_vars.iter().find(|(n, _)| *n == text) {
            let tpe = self.u.intern_type(Type::Sym(None, sym));
            return self.tpd.node(T::Ident { name }, span, Some(tpe), Some(sym));
        }
        let info = match pt {
            Pt::Type(p) if self.u.bounds(p).is_some() => p,
            _ => self.u.intern_type(Type::Bounds(NOTHING, ANY)),
        };
        let sym = self.syms.define(self.u, &text, Kind::Type, outer.owner, SYNTHETIC.or(CASE));
        self.u.set_info(sym, info);
        self.known_pattern_types.insert(sym);
        self.constraint.define_at(sym, before);
        self.levels.enter(outer.level, &text, sym);
        if let Some(scope) = self.quote_pattern.as_mut() {
            scope.type_vars.push((text, sym));
        }
        let wild = TName { name: self.name("_"), is_type: false };
        let wild = self.tpd.node(T::Ident { name: wild }, Span::at(span.start()), Some(info), None);
        let tpe = self.u.intern_type(Type::Sym(None, sym));
        self.tpd.node(T::Bind { name, body: wild }, span, Some(tpe), Some(sym))
    }

    /// The bounds a type variable gets as argument `i` of `tycon` in a quote pattern: those of
    /// the type parameter of the class or of the type lambda an alias stands for, if they refer
    /// to no other parameter.
    pub fn quoted_type_arg_bounds(&mut self, tycon: TypeId, i: usize) -> TypeId {
        let empty = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        let class = match *self.u.ty(tycon) {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, name) => self.u.resolve(p, name, true),
            _ => None,
        };
        let params = match class {
            Some(c) if self.u.symbol(c).kind == Kind::Class => self.u.type_params(c),
            Some(a) => {
                let lambda = self.u.alias_of(a).and_then(|t| self.u.lambda(t));
                let Some(params) = lambda.map(|l| l.params.to_vec()) else { return empty };
                let refers = |u: &mut tasty::Universe, t| {
                    let mut found = false;
                    u.map_type(t, &mut |u, x| {
                        found |= matches!(u.ty(x), Type::Param(..) | Type::Local(..));
                        None
                    });
                    found
                };
                return match params.get(i) {
                    Some(&(_, b)) if self.u.bounds(b).is_some() && !refers(self.u, b) => b,
                    _ => empty,
                };
            }
            _ => return empty,
        };
        let Some(&param) = params.get(i) else { return empty };
        self.complete(param);
        let info = self.u.info(param);
        let bounds = self.u.bounds(info).is_some();
        if bounds && !self.refers_to(info, &params) { info } else { empty }
    }

    /// Replaces the type binds in quote pattern body `t` outside its splices by type trees and
    /// appends them to `out`, those in contravariant positions (`variance` -1) marked
    /// `@fromAbove`; whether `t` changed. An intersection type tree that changed gets its
    /// unsimplified type, a type ascription that of its type tree (dotty's `cpy`).
    pub fn extract_type_bindings(&mut self, t: TNodeId, variance: i8, out: &mut Vec<TNodeId>) -> bool {
        match *self.tpd.tree(t) {
            T::Bind { .. } if self.tpd.sym(t).is_some_and(|s| self.u.is_type_symbol(s)) => {
                let sym = self.tpd.sym(t).expect("a type bind's symbol");
                if variance == -1
                    && let Some(c) = self.resolve_full_name("scala.quoted.runtime.Patterns$.fromAbove", true)
                {
                    self.u.annotate(sym, c);
                }
                let (tree, span, tpe) = (self.tpd.tree(t).clone(), self.tpd.span(t), self.tpd.get(t).tpe);
                out.push(self.tpd.node(tree, span, tpe, Some(sym)));
                let node = self.tpd.get_mut(t);
                node.tree = T::TypeTree { inferred: false };
                node.sym = None;
                true
            }
            T::SplicePattern { .. } => false,
            T::AppliedTypeTree { tpt, args } => {
                let tycon = self.tpd.tpe(tpt);
                let args = self.tpd.items(args).to_vec();
                let and = self.u.sym_of(tycon).is_some_and(|s| self.u.full_name(s) == crate::defn::AND);
                let variances = match (and, self.u.class_symbol(tycon)) {
                    (true, _) => vec![1; args.len()],
                    (false, Some(c)) => {
                        Some(self.u.variances(c)).filter(|v| v.len() == args.len()).unwrap_or_default()
                    }
                    (false, None) => Vec::new(),
                };
                let mut changed = false;
                for (i, &a) in args.iter().enumerate() {
                    let bounds = self.u.bounds(self.tpd.tpe(a)).is_some();
                    let v = if bounds { variance } else { variances.get(i).copied().unwrap_or(0) * variance };
                    changed |= self.extract_type_bindings(a, v, out);
                }
                changed |= self.extract_type_bindings(tpt, variance, out);
                if changed
                    && and
                    && let &[a, b] = &args[..]
                {
                    let tpe = Type::And(self.tpd.tpe(a), self.tpd.tpe(b));
                    self.tpd.get_mut(t).tpe = Some(self.u.intern_type(tpe));
                }
                changed
            }
            T::Typed { expr, tpt } => {
                let changed = self.extract_type_bindings(expr, variance, out);
                let changed = self.extract_type_bindings(tpt, variance, out) || changed;
                if changed {
                    self.tpd.get_mut(t).tpe = Some(self.tpd.tpe(tpt));
                }
                changed
            }
            _ => {
                let mut changed = false;
                for c in self.tpd.children(t) {
                    changed |= self.extract_type_bindings(c, variance, out);
                }
                changed
            }
        }
    }
}
