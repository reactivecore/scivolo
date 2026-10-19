// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/TypeTestsCasts.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Type patterns of abstract types (dotty's `tryWithTypeTest`, and the unchecked warning of
//! `TypeTestsCasts.checkTypePattern`).

use super::dependent_results::has_wildcard;
use super::implicits::is_misfit;
use super::irrefutable::UNCHECKED;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

const TAGS: [&str; 2] = ["scala.reflect.TypeTest", "scala.reflect.ClassTag"];

impl Typer<'_> {
    /// Pattern `id`, `expr: tpe` with type tree `tpt`, against `pt` as the extractor pattern
    /// `tag(expr)` of a given `TypeTest[pt, tpe]` or `ClassTag[tpe]` if `tpe`, its aliases
    /// followed up to a match-type alias, is or applies a type that is not a class.
    pub fn try_with_type_test(
        &mut self,
        id: NodeId,
        expr: NodeId,
        tpt: NodeId,
        tpe: TypeId,
        pt: Pt,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let Pt::Type(p) = pt else { return Ok(None) };
        let dealiased = self.dealiased(tpe);
        let tycon = match *self.u.types().get(dealiased) {
            Type::Applied(tycon, _) => tycon,
            _ => dealiased,
        };
        let tycon = self.match_alias(tpe).unwrap_or(tycon);
        let same = self.u.is_subtype(dealiased, p) && self.u.is_subtype(p, dealiased);
        let non_class = self.referenced(tycon).is_some_and(|s| {
            self.complete(s);
            matches!(self.u.symbol(s).kind, Kind::TypeParam | Kind::Type)
        });
        if !non_class || same || has_wildcard(self.u, dealiased) {
            return Ok(None);
        }
        let at = Span::at(self.ast.span(tpt).start());
        let mut found = None;
        for (class_name, args) in TAGS.into_iter().zip([vec![p, tpe], vec![tpe]]) {
            let Some(class) = self.resolve_full_name(class_name, true) else { continue };
            let wanted = crate::defn::applied(self.u, class, args);
            match self.implicit_arg(wanted, at, ctx.with_mode(Mode::NONE), 0) {
                Ok(tag) => {
                    found = Some(tag);
                    break;
                }
                Err(what) if is_misfit(what) || what.starts_with("class tag") => {}
                Err(what) => return Err(what),
            }
        }
        let Some(tag) = found else { return Ok(None) };
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, at);
        self.splices.insert(splice, tag);
        let args = self.ast.list(&[expr]);
        let apply = self.ast.node(Tree::Apply { fun: splice, args });
        self.ast.with_span(apply, Span::synthetic(self.ast.span(id).start(), at.start()));
        Ok(Some(self.typed(apply, pt, ctx)))
    }

    /// Extractor pattern `id`, typed as `tree` of type `own`, against `sel` as `tree: own`,
    /// tested by a given `TypeTest` or `ClassTag` if `own` refers to an abstract type.
    pub fn type_tested_unapply(
        &mut self,
        id: NodeId,
        tree: TNodeId,
        own: TypeId,
        sel: TypeId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let span = self.ast.span(id);
        if !self.is_testable_type(own) {
            let expr = self.ast.node(Tree::TypeTree);
            self.ast.with_span(expr, span);
            self.splices.insert(expr, tree);
            let tpt = self.ast.node(Tree::TypeTree);
            self.ast.with_span(tpt, span.end_pos());
            if let Some(tagged) = self.try_with_type_test(id, expr, tpt, own, Pt::Type(sel), ctx)? {
                return Ok(tagged);
            }
            if !self.is_erased_test_type(own) {
                return Err("extractor of a type that needs a type test");
            }
            self.warn_unchecked_type_test(Pt::Type(sel), own, span);
        }
        let tpt = self.type_tree(own, span.end_pos(), false);
        Ok(self.tpd.node(T::Typed { expr: tree, tpt }, span.to_synthetic(), Some(own), None))
    }

    /// The match-type alias that `t`, or an alias `t` refers to, is or applies.
    fn match_alias(&mut self, mut t: TypeId) -> Option<TypeId> {
        for _ in 0..8 {
            let tycon = match *self.u.types().get(t) {
                Type::Applied(tycon, _) => tycon,
                _ => t,
            };
            let s = self.referenced(tycon).filter(|&s| self.u.symbol(s).kind == Kind::Type)?;
            self.complete(s);
            let info = self.u.info(s);
            let rhs = match *self.u.types().get(info) {
                Type::Alias(rhs) => rhs,
                _ => info,
            };
            let body = match *self.u.types().get(rhs) {
                Type::Lambda { result, .. } => result,
                _ => rhs,
            };
            match *self.u.types().get(body) {
                Type::Match { .. } => return Some(tycon),
                _ if tycon == t && rhs != info => t = rhs,
                _ => return None,
            }
        }
        None
    }

    /// Whether `t` refers to a type parameter or an abstract type member.
    pub fn is_abstract_type(&mut self, t: TypeId) -> bool {
        let Some(s) = self.referenced(t) else { return false };
        match self.u.symbol(s).kind {
            Kind::TypeParam => true,
            Kind::Type => {
                self.complete(s);
                let info = self.u.info(s);
                !matches!(self.u.types().get(info), Type::Alias(_))
            }
            _ => false,
        }
    }

    /// Whether a typed pattern can test for `t` by its erasure: an abstract type, or a union or
    /// intersection of such types and testable ones.
    pub fn is_erased_test_type(&mut self, t: TypeId) -> bool {
        let t = self.dealiased(t);
        match *self.u.types().get(t) {
            Type::And(a, b) | Type::Or(a, b, _) => {
                let side = |s: &mut Self, x| s.is_erased_test_type(x) || s.is_testable_type(x);
                side(self, a) && side(self, b)
            }
            _ => self.is_abstract_type(t) && !has_wildcard(self.u, t),
        }
    }

    /// dotty's unchecked warning at `span` about testing a value of type `pt` for `tpe` that
    /// refers to an abstract type, unless `pt` conforms or is `@unchecked`.
    pub fn warn_unchecked_type_test(&mut self, pt: Pt, tpe: TypeId, span: Span) {
        let Pt::Type(p) = pt else { return };
        if self.without_annotation(p, UNCHECKED) != p || self.u.is_subtype(p, tpe) {
            return;
        }
        let shown = self.plain_text(tpe).unwrap_or_else(|| self.u.show_user(tpe));
        let text = format!(
            "the type test for {shown} cannot be checked at runtime because it refers to an \
             abstract type member or type parameter"
        );
        self.erasure_diags.warning_at(span.start(), span.end(), span.point(), text);
    }

    /// `x @ tag(e)` as `tag(x @ e)` if `body` is the extractor pattern of a type test.
    pub fn bind_in_type_test(&mut self, id: NodeId, body: TNodeId, ctx: Ctx) -> Option<TNodeId> {
        let T::UnApply { fun, implicits, patterns } = *self.tpd.tree(body) else { return None };
        if !self.tpd.items(implicits).is_empty() {
            return None;
        }
        let [arg] = self.tpd.items(patterns)[..] else { return None };
        let owner = self.tpd.sym(self.tpd.meth_part(fun)).and_then(|s| self.u.symbol(s).owner)?;
        let name = self.u.full_name(owner);
        if !TAGS.contains(&name.as_str()) && !self.derives_from_named_class(owner, TAGS[0]) {
            return None;
        }
        let Tree::Bind { name, mods, .. } = *self.ast.tree(id) else { return None };
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, self.tpd.span(arg));
        self.splices.insert(splice, arg);
        let bind = self.ast.node(Tree::Bind { name, body: splice, mods });
        self.ast.with_span(bind, self.ast.span(id));
        let arg_tpe = self.tpd.tpe(arg);
        let bound = self.typed(bind, Pt::Type(arg_tpe), ctx);
        let patterns = self.tpd.list(&[bound]);
        let (span, tpe) = (self.tpd.span(body), self.tpd.tpe(body));
        Some(self.tpd.node(T::UnApply { fun, implicits, patterns }, span, Some(tpe), None))
    }

    fn derives_from_named_class(&mut self, class: SymbolId, name: &str) -> bool {
        let Some(base) = self.resolve_full_name(name, true) else { return false };
        self.u.symbol(class).kind == Kind::Class && self.u.derives_from(class, base)
    }
}
