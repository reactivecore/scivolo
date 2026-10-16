//! The definitions of inline accessors: their class, name and forwarding body (dotty's
//! `hostForAccessorOf`, `InlineAccessorName`, `accessorDef`).

use super::Typer;
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `def name(ps): T = C.this.accessed(ps)` in class `host`, or `= C.this.accessed = x$0` for
    /// a setter; `None`, reported at `span`, for a polymorphic or implicit method.
    pub fn accessor_def(
        &mut self,
        name: &str,
        host: SymbolId,
        accessed: SymbolId,
        setter: bool,
        span: Span,
    ) -> Option<NodeId> {
        self.complete(accessed);
        let mut info = self.u.info(accessed);
        if self.u.is_module(accessed)
            && let Type::Sym(prefix, _) = *self.u.types().get(info)
        {
            info = self.u.intern_type(Type::Sym(prefix, accessed));
        }
        let mut clauses: Vec<Vec<(String, TypeId)>> = Vec::new();
        loop {
            match self.u.types().get(info).clone() {
                Type::Method { params, result, implicit: false, .. } if !setter => {
                    clauses.push(params);
                    info = result;
                }
                Type::Method { .. } | Type::Poly { .. } => {
                    self.unsupported_at(span, "inline accessor of a polymorphic or implicit method");
                    return None;
                }
                _ => break,
            }
        }
        if self.is_context_function_type(info) {
            self.unsupported_at(span, "inline accessor of a context function result");
            return None;
        }
        let result = if setter {
            clauses.push(vec![("x$0".to_owned(), info)]);
            crate::defn::type_ref(self.u, self.defn.unit)
        } else {
            info
        };
        let at = self.syms.def_trees.get(&accessed).map_or(span, |&d| self.ast.span(d));
        let at = Span::at(at.point());
        let target = self.accessed_ref(host, accessed, at);
        let mut rhs = self.ast.node(Tree::TypeTree);
        self.splices.insert(rhs, target);
        let mut defs = Vec::new();
        for clause in &clauses {
            let mut params = Vec::new();
            let mut args = Vec::new();
            for (pname, ptype) in clause {
                let name = TName { name: self.name(pname), is_type: false };
                let tpt = self.splice_type(*ptype);
                let mods = self.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
                params.push(self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods }));
                args.push(self.ast.node(Tree::Ident { name }));
            }
            rhs = match setter {
                true => self.ast.node(Tree::Assign { lhs: rhs, rhs: args[0] }),
                false => {
                    let args = self.ast.list(&args);
                    self.ast.node(Tree::Apply { fun: rhs, args })
                }
            };
            defs.push(self.ast.list(&params));
        }
        let tpt = self.splice_type(result);
        let paramss = self.ast.clauses(&defs);
        let private = self.u.symbol(accessed).flags.has(tasty::tags::PRIVATE);
        let flags = f::SYNTHETIC.or(if private { f::FINAL } else { f::EMPTY });
        let mods = self.ast.mods(Modifiers::with_flags(flags));
        let name = TName { name: self.name(name), is_type: false };
        let def = self.ast.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs), mods });
        self.ast.with_span(def, at);
        Some(def)
    }

    /// A reference to `accessed` from the `this` of `host`: an identifier for a member of a
    /// static object or a parameter accessor outside traits, else a selection (dotty's `ref`).
    fn accessed_ref(&mut self, host: SymbolId, accessed: SymbolId, at: Span) -> TNodeId {
        let owner = self.u.symbol(accessed).owner.expect("a member");
        if self.is_static_owner(owner) {
            return self.member_ref(accessed, at);
        }
        let host_type = crate::defn::type_ref(self.u, host);
        let param =
            self.syms.flags.get(&accessed).is_some_and(|fl| fl.is_one_of(f::PARAM.or(f::PARAM_ACCESSOR)));
        if param && !self.u.is_trait(host) && !self.u.is_trait(owner) {
            let this = self.u.intern_type(Type::This(host_type));
            let tpe = self.u.intern_type(Type::Sym(Some(this), accessed));
            return self.build().ident(accessed, tpe, at);
        }
        let this = self.build().this(host_type, at);
        self.build().select(this, accessed, at)
    }

    /// The closest class enclosing `owner` that has `accessed` as a member or is the companion
    /// of its object; the top-level class of `owner` for a package member (dotty's
    /// `hostForAccessorOf`).
    pub fn accessor_host(&mut self, accessed: SymbolId, owner: SymbolId) -> Option<SymbolId> {
        let target = self.u.symbol(accessed).owner?;
        let mut o = Some(owner);
        let mut top = None;
        while let Some(c) = o {
            if self.u.symbol(c).kind == Kind::Class {
                let companion = self.u.companion_module(c).and_then(|m| self.u.module_class(m));
                if self.u.derives_from(c, target) || companion == Some(target) {
                    return Some(c);
                }
                top = Some(c);
            }
            if self.u.symbol(c).kind == Kind::Package {
                return (c == target).then_some(top).flatten();
            }
            o = self.u.symbol(c).owner;
        }
        None
    }

    /// The name of the accessor of `accessed`, a setter if `setter`, in class `host`: expanded by
    /// the class's name if it may have subclasses (dotty's `accessorNameOf`); `None` in a local
    /// class.
    pub fn accessor_name(&self, accessed: SymbolId, host: SymbolId, setter: bool) -> Option<String> {
        let mut o = self.u.symbol(host).owner;
        while let Some(x) = o.filter(|&x| self.u.symbol(x).kind != Kind::Package) {
            if self.u.symbol(x).kind != Kind::Class {
                return None;
            }
            o = self.u.symbol(x).owner;
        }
        let prefix = self.u.expanded_prefix(host);
        let base = format!("inline${}", self.u.symbol(accessed).name);
        let s = self.u.symbol(host);
        let extensible =
            !s.flags.has(tasty::tags::FINAL) && !self.u.is_module_class(host) && !s.name.starts_with("$anon");
        let name = if extensible { format!("{prefix}$${base}") } else { base };
        Some(if setter { format!("{name}_=") } else { name })
    }
}
