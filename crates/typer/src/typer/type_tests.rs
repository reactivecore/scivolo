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
    /// `tag(expr)` of a given `TypeTest[pt, tpe]` or `ClassTag[tpe]` if `tpe` is or applies an
    /// abstract type.
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
        let same = self.u.is_subtype(dealiased, p) && self.u.is_subtype(p, dealiased);
        if !self.is_abstract_type(tycon) || same || has_wildcard(self.u, dealiased) {
            return Ok(None);
        }
        let at = Span::at(self.ast.span(tpt).start());
        let mut found = None;
        for (class_name, args) in TAGS.into_iter().zip([vec![p, tpe], vec![tpe]]) {
            let Some(class) = self.resolve_full_name(class_name, true) else { continue };
            let wanted = crate::defn::applied(self.u, class, args);
            match self.implicit_arg(wanted, at, ctx.with_mode(Mode::NONE), 0) {
                Ok(_) if class_name == TAGS[0] => return Err("type pattern tested by a given TypeTest"),
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
