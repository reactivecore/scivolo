//! Types the desugaring leaves to the typer (dotty's `DerivedTypeTree`s): copies of a class's
//! constructor parameters, a setter's parameter, references to a class or its companion.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level};
use crate::defn::type_ref;
use crate::tpd::TNodeId;
use syntax::ast::flags::{PARAM, PARAM_ACCESSOR};
use syntax::ast::tree::DerivedKind;
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_derived(
        &mut self,
        id: NodeId,
        kind: DerivedKind,
        watched: Option<TName>,
        ctx: Ctx,
    ) -> TNodeId {
        let Some(watched) = watched else { return self.unsupported(id, "derived type without a source") };
        if kind == DerivedKind::TermRef {
            let ident = self.ast.node(Tree::Ident { name: TName { is_type: false, ..watched } });
            self.ast.with_span(ident, self.ast.span(id));
            return self.typed(ident, Pt::Any, ctx);
        }
        let text = self.text(watched.name).to_owned();
        let tpe = match kind {
            DerivedKind::FromParam => self.derived_from_param(&text, watched.is_type, ctx),
            DerivedKind::SetterParam => self.setter_param_type(&text, ctx),
            _ => self.class_named(&text, ctx).map(|c| type_ref(self.u, c)),
        };
        let Some(tpe) = tpe else { return self.unsupported(id, &format!("derived type {kind:?}")) };
        let span = self.ast.span(id);
        self.type_tree(tpe, span, false)
    }

    /// The constructor whose parameters the class members and companion methods of `ctx` copy:
    /// the enclosing class's primary one, or, in an object, its companion class's.
    fn source_constructor(&mut self, ctx: Ctx) -> Option<SymbolId> {
        let cls = self.enclosing_class(ctx.owner)?;
        if let Some(&ctor) = self.syms.primary_ctors.get(&cls)
            && !self.u.symbol(cls).flags.has(tasty::tags::OBJECT)
        {
            return Some(ctor);
        }
        self.companion_constructor(cls)
    }

    /// The primary constructor of the companion class of object class `module`.
    pub fn companion_constructor(&mut self, module: SymbolId) -> Option<SymbolId> {
        let name = tasty::module_name(&self.u.symbol(module).name)?.to_owned();
        let owner = self.u.symbol(module).owner?;
        let decls = self.u.symbol(owner).decls.clone();
        let companion = decls
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Class && self.u.symbol(d).name == name)?;
        self.complete(companion);
        self.syms.primary_ctors.get(&companion).copied()
    }

    /// The info of parameter `name` of the source constructor, its references to the
    /// constructor's other parameters made to the same-named parameters or accessors visible in
    /// `ctx` (dotty's `DerivedFromParamTree.derivedTree`).
    fn derived_from_param(&mut self, name: &str, is_type: bool, ctx: Ctx) -> Option<TypeId> {
        let ctor = self.source_constructor(ctx)?;
        self.complete(ctor);
        let decls = self.u.symbol(ctor).decls.clone();
        let original = decls
            .into_iter()
            .find(|&p| self.u.symbol(p).name == name && self.is_type_symbol(p) == is_type)?;
        let info = match self.syms.repeated_params.get(&original) {
            Some(&written) => written,
            None => self.u.info(original),
        };
        let mut siblings = Vec::new();
        self.u.map_type(info, &mut |u, t| {
            if let Type::Sym(_, s) = *u.types().get(t)
                && u.symbol(s).owner == Some(ctor)
            {
                siblings.push(s);
            }
            None
        });
        let mut replacements = Vec::new();
        for s in siblings {
            let (n, ty) = (self.u.symbol(s).name.clone(), self.is_type_symbol(s));
            let local = self.param_or_accessor(&n, ty, ctx)?;
            replacements.push((s, local));
        }
        Some(self.u.map_type(info, &mut |u, t| match *u.types().get(t) {
            Type::Sym(_, s) => replacements.iter().find(|&&(o, _)| o == s).map(|&(_, l)| l),
            _ => None,
        }))
    }

    /// A reference to the parameter or parameter accessor named `name` visible from `ctx`.
    fn param_or_accessor(&mut self, name: &str, is_type: bool, ctx: Ctx) -> Option<TypeId> {
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            let candidates: Vec<SymbolId> = match self.levels.get(l) {
                Level::Scope(entries) => entries.iter().filter(|(n, _)| n == name).map(|&(_, s)| s).collect(),
                &Level::Members(owner) if self.u.symbol(owner).kind == Kind::Class => self
                    .u
                    .symbol(owner)
                    .decls
                    .iter()
                    .copied()
                    .filter(|&d| self.u.symbol(d).name == name)
                    .collect(),
                _ => Vec::new(),
            };
            let found = candidates.into_iter().find(|&s| {
                self.is_type_symbol(s) == is_type
                    && self.syms.flags.get(&s).is_some_and(|f| f.is(PARAM) || f.is(PARAM_ACCESSOR))
            });
            if let Some(s) = found {
                let owner = self.u.symbol(s).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class);
                let prefix = owner.map(|o| {
                    let class = type_ref(self.u, o);
                    self.u.intern_type(Type::This(class))
                });
                return Some(self.u.intern_type(Type::Sym(prefix, s)));
            }
            level = self.levels.outer(l);
        }
        None
    }

    /// The type of the variable `name` whose setter's parameter is being typed in `ctx`.
    fn setter_param_type(&mut self, name: &str, ctx: Ctx) -> Option<TypeId> {
        let owner = self.u.symbol(ctx.owner).owner?;
        let decls = self.u.symbol(owner).decls.clone();
        let var = decls
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Val && self.u.symbol(d).name == name)?;
        self.complete(var);
        Some(self.u.info(var))
    }

    /// The class named `name` defined in the owner of `ctx` or around it.
    fn class_named(&mut self, name: &str, ctx: Ctx) -> Option<SymbolId> {
        match self.find_ref(name, true, ctx) {
            Ok(Some(found)) => found.syms.into_iter().find(|&s| self.u.symbol(s).kind == Kind::Class),
            _ => None,
        }
    }
}
