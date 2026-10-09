//! Constructor proxies: `C(args)` typed as `new C(args)` when `C` names a class but no value with
//! an `apply` member (dotty's `addConstructorProxies`, `newExpr`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::tags::{ABSTRACT, CASE, INVISIBLE, OBJECT, SYNTHETIC, TRAIT};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// Whether class `c` gets constructor proxies (dotty's `needsConstructorProxies`).
    pub fn needs_constructor_proxies(&self, c: SymbolId) -> bool {
        let s = self.u.symbol(c);
        s.kind == Kind::Class
            && ![ABSTRACT, TRAIT, CASE, SYNTHETIC, OBJECT, INVISIBLE].iter().any(|&f| s.flags.has(f))
            && !s.name.starts_with("$anon")
    }

    /// `new C` for the constructor proxy `C` that identifier `id` named `name` refers to.
    pub fn proxy_ident(&mut self, id: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let text = self.text(name.name).to_owned();
        let found = match self.find_ref(&text, true, ctx) {
            Ok(Some(found)) => found.syms.first().map(|&c| (found.prefix, c)),
            _ => None,
        };
        let found = found.and_then(|(prefix, s)| match self.u.symbol(s).kind {
            tasty::Kind::Type => self.package_class_alias(s).map(|c| (None, c)),
            _ => Some((prefix, s)),
        });
        let Some((prefix, class)) = found.filter(|&(_, c)| self.needs_constructor_proxies(c)) else {
            return self.unsupported(id, "constructor proxy");
        };
        if let Err(what) = self.check_reference(class, prefix, ctx) {
            return self.unsupported(id, what);
        }
        let name = TName { is_type: true, ..name };
        let tpe = self.ref_type(prefix, class);
        let span = self.ast.span(id);
        let tpt = if self.term_prefix_is_elidable(prefix, class, ctx) {
            self.tpd.node(T::Ident { name }, span, Some(tpe), Some(class))
        } else {
            self.this_select(span, name, tpe, class)
        };
        self.proxy_new(tpt, pt, ctx)
    }

    /// `new q.C` for the constructor proxy `q.C` that selection `id`, `name` of `qual` of type
    /// `qual_tpe`, refers to, if `C` is a class member of `qual_tpe` that gets proxies and no
    /// base class of its owner declares a term `C` (dotty's `memberExists`).
    pub fn proxy_select(
        &mut self,
        id: NodeId,
        (qual, qual_tpe): (TNodeId, tasty::TypeId),
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let text = self.text(name.name).to_owned();
        let classes: Vec<SymbolId> = self
            .type_members(qual_tpe, &text)
            .into_iter()
            .filter(|&c| self.u.symbol(c).kind == Kind::Class)
            .collect();
        let [class] = classes[..] else { return None };
        let owner = self.u.symbol(class).owner?;
        let term_exists = self.u.linearization(owner).into_iter().any(|b| {
            let decls = self.u.decl(b, &text).unwrap_or_default();
            decls.iter().any(|&d| self.u.symbol(d).kind.is_term())
        });
        if term_exists || !self.needs_constructor_proxies(class) {
            return None;
        }
        if let Err(what) = self.check_reference(class, Some(qual_tpe), ctx) {
            return Some(self.unsupported(id, what));
        }
        let span = self.ast.span(id);
        let tpe = if self.is_static_owner(owner) {
            crate::defn::type_ref(self.u, class)
        } else {
            self.ref_type(Some(qual_tpe), class)
        };
        let name = TName { is_type: true, ..name };
        let tpt = self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), Some(class));
        Some(self.proxy_new(tpt, pt, ctx))
    }

    /// The class of a package without a companion object that type alias `s`, a class member,
    /// stands for, such as `java.lang.Exception` for `scala.Exception`.
    fn package_class_alias(&mut self, s: SymbolId) -> Option<SymbolId> {
        let alias_owner = self.u.symbol(s).owner?;
        if self.u.symbol(alias_owner).kind != Kind::Class {
            return None;
        }
        let tpe = self.u.intern_type(Type::Sym(None, s));
        let dealiased = self.u.dealias(tpe);
        if !matches!(self.u.types().get(dealiased), Type::Sym(..) | Type::TypeRef(..)) {
            return None;
        }
        let c = self.u.class_symbol(dealiased)?;
        let owner = self.u.symbol(c).owner?;
        if self.u.symbol(c).kind != Kind::Class || self.u.symbol(owner).kind != Kind::Package {
            return None;
        }
        let name = self.u.symbol(c).name.clone();
        let decls = self.u.decl(owner, &name).unwrap_or_default();
        let companion = decls.iter().any(|&d| self.u.symbol(d).flags.has(OBJECT));
        (!companion).then_some(c)
    }

    /// `new C` for `f`, a reference to the companion object of class `C` without an `apply`
    /// member, `None` if `C` gets no constructor proxies.
    pub fn proxy_companion(&mut self, f: TNodeId, pt: Pt, ctx: Ctx) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(f);
        let widened = self.u.widen(tpe);
        let module_class = self.u.class_symbol(widened).filter(|&m| self.u.symbol(m).flags.has(OBJECT))?;
        let owner = self.u.symbol(module_class).owner?;
        let class = self.u.companion_class(module_class)?;
        if !self.needs_constructor_proxies(class) {
            return None;
        }
        let module = self.tpd.sym(f)?;
        let is_module = self.u.module_class(module) == Some(module_class);
        let tpe = match *self.u.types().get(tpe) {
            _ if self.is_static_owner(owner) => crate::defn::type_ref(self.u, class),
            Type::Sym(prefix, _) if is_module => self.ref_type(prefix, class),
            _ if self.is_static_owner(module_class) => crate::defn::type_ref(self.u, class),
            _ => return None,
        };
        let span = self.tpd.span(f);
        let tpt = match *self.tpd.tree(f) {
            T::Ident { name } => {
                let name = TName { is_type: true, ..name };
                self.tpd.node(T::Ident { name }, span, Some(tpe), Some(class))
            }
            T::Select { qualifier, name } => {
                let name = TName { is_type: true, ..name };
                self.tpd.node(T::Select { qualifier, name }, span, Some(tpe), Some(class))
            }
            _ => return None,
        };
        Some(self.proxy_new(tpt, pt, ctx))
    }

    /// `new tpt` selecting its constructor (dotty's `newExpr`).
    fn proxy_new(&mut self, tpt: TNodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let span = self.tpd.span(tpt);
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, span);
        self.splices.insert(splice, tpt);
        let synthetic = Span::synthetic(span.start(), span.end());
        let new = self.ast.node(Tree::New { tpt: splice });
        self.ast.with_span(new, synthetic);
        let init = TName { name: self.name("<init>"), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: new, name: init });
        self.ast.with_span(select, synthetic);
        self.typed_select(select, new, init, pt, ctx)
    }
}
