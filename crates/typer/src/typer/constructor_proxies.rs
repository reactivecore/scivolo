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
    fn needs_constructor_proxies(&self, c: SymbolId) -> bool {
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

    /// `new C` for `f`, a reference to the companion object of class `C` without an `apply`
    /// member, `None` if `C` gets no constructor proxies.
    pub fn proxy_companion(&mut self, f: TNodeId, pt: Pt, ctx: Ctx) -> Option<TNodeId> {
        let tpe = self.tpd.tpe(f);
        let widened = self.u.widen(tpe);
        let module_class = self.u.class_symbol(widened).filter(|&m| self.u.symbol(m).flags.has(OBJECT))?;
        let name = self.u.symbol(module_class).name.strip_suffix('$')?.to_owned();
        let owner = self.u.symbol(module_class).owner?;
        let decls = self.u.decl(owner, &name).unwrap_or_default();
        let class = decls.into_iter().find(|&d| self.u.symbol(d).kind == Kind::Class)?;
        if !self.needs_constructor_proxies(class) {
            return None;
        }
        let module = self.tpd.sym(f)?;
        let is_module = self.u.module_class(module) == Some(module_class);
        let tpe = match *self.u.types().get(tpe) {
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
