// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Methods' parameters: their symbols, entered clause by clause before their types are typed.

use crate::ctx::{Ctx, Level};
use crate::typer::Typer;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// `t` with references to type parameters `tparams` made references to the parameters of
    /// polymorphic type `binder`.
    pub fn param_refs(
        &mut self,
        t: tasty::TypeId,
        tparams: &[SymbolId],
        binder: tasty::Binder,
    ) -> tasty::TypeId {
        self.u.map_type(t, &mut |u, x| match *u.types().get(x) {
            Type::Sym(None, s) => {
                let i = tparams.iter().position(|&p| p == s)?;
                Some(u.intern_type(Type::Param(binder, i as u32)))
            }
            _ => None,
        })
    }

    /// The type of parameter `p` in its method's type, `@InlineParam` if it is inline and
    /// `@ErasedParam` if it is erased, inside a by-name type (dotty's `adaptParamInfo`).
    pub fn method_param_type(&mut self, p: SymbolId) -> tasty::TypeId {
        let mut t = self.param_type(p);
        let flags = self.syms.flags.get(&p).copied().unwrap_or_default();
        for (flag, annot) in [(f::INLINE, "InlineParam"), (f::ERASED, "ErasedParam")] {
            if flags.is(flag) {
                t = self.annotated_param(t, annot);
            }
        }
        t
    }

    fn annotated_param(&mut self, t: tasty::TypeId, annot: &str) -> tasty::TypeId {
        let Some(class) = self.resolve_full_name(&format!("scala.annotation.internal.{annot}"), true) else {
            return t;
        };
        let annot = crate::defn::type_ref(self.u, class);
        match *self.u.types().get(t) {
            Type::ByName(r) => {
                let r = self.u.intern_type(Type::Annotated(r, annot));
                self.u.intern_type(Type::ByName(r))
            }
            _ => self.u.intern_type(Type::Annotated(t, annot)),
        }
    }

    /// Constructor parameter clauses `clauses` with an empty clause before a leading `implicit`
    /// one, or after clauses that are all `using` ones (dotty's `normalizeIfConstructor`).
    pub fn normalized_constructor(&self, mut clauses: Vec<Vec<SymbolId>>) -> Vec<Vec<SymbolId>> {
        let flags =
            |c: &Vec<SymbolId>| c.first().and_then(|p| self.syms.flags.get(p)).copied().unwrap_or_default();
        let is_type = |c: &Vec<SymbolId>| c.first().is_some_and(|&p| self.u.is_type_symbol(p));
        if let Some(i) =
            clauses.iter().position(|c| !is_type(c)).filter(|&i| flags(&clauses[i]).is(f::IMPLICIT))
        {
            clauses.insert(i, Vec::new());
        } else if clauses.iter().all(|c| is_type(c) || !c.is_empty() && flags(c).is(f::GIVEN)) {
            clauses.push(Vec::new());
        }
        clauses
    }

    /// The context of the parameters and body of method `sym`, defined in `ctx`.
    pub fn method_ctx(&mut self, sym: SymbolId, ctx: Ctx) -> Ctx {
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        Ctx { owner: sym, level, ..ctx }
    }

    /// The parameter symbols of method definition `tree`, entered in `ctx`, by clause.
    pub fn enter_params(&mut self, sym: SymbolId, tree: NodeId, ctx: Ctx) -> Vec<Vec<SymbolId>> {
        let mut out = Vec::new();
        for clause in self.def_clauses(tree) {
            let mut syms = Vec::new();
            let mut fresh = Vec::new();
            for &p in &clause {
                let param = match self.syms.by_tree.get(&p) {
                    Some(&s) => s,
                    None => {
                        let s = self.define_param(sym, p);
                        fresh.push((s, p));
                        s
                    }
                };
                let name = self.u.symbol(param).name.clone();
                self.levels.enter(ctx.level, &name, param);
                syms.push(param);
            }
            for (s, p) in fresh {
                self.param_info(s, p, ctx);
            }
            out.push(syms);
        }
        out
    }

    /// The symbol of parameter `p` of `method`.
    fn define_param(&mut self, method: SymbolId, p: NodeId) -> SymbolId {
        let (name, mods, kind, extra) = match *self.ast.tree(p) {
            Tree::ValDef { name, mods, .. } => (name, mods, Kind::Param, f::TERM_PARAM),
            Tree::TypeDef { name, mods, .. } => (name, mods, Kind::TypeParam, f::TYPE_PARAM),
            _ => {
                self.unsupported(p, "parameter");
                return self.syms.define(self.u, "?", Kind::Param, method, f::EMPTY);
            }
        };
        let flags = self.ast.modifiers(mods).flags.or(extra);
        let flags = if kind == Kind::Param { flags.to_term_flags() } else { flags.to_type_flags() };
        if flags.is(f::INLINE) && !self.u.symbol(method).flags.has(tasty::tags::INLINE) {
            self.unsupported(p, "inline parameter of a method that is not inline");
        }
        if flags.is_all_of(f::INLINE.or(f::HAS_DEFAULT)) {
            self.unsupported(p, "default argument of an inline parameter");
        }
        let text = self.text(name.name).to_owned();
        let s = self.syms.define(self.u, &text, kind, method, flags);
        self.syms.param_trees.insert(s, p);
        if self.syms.by_tree.insert(p, s).is_some() {
            self.unsupported(p, "definition tree shared by two definitions");
        }
        s
    }

    /// Sets the info of parameter `s` from its type or bounds in `p`, typed in `ctx`.
    fn param_info(&mut self, s: SymbolId, p: NodeId, ctx: Ctx) {
        self.add_annotations(s, p, ctx);
        let info = match *self.ast.tree(p) {
            Tree::ValDef { tpt, .. } => {
                let info = self.value_type_ahead(tpt, ctx.with_owner(s));
                let repeated = self.repeated_elem(info).is_some();
                match *self.u.types().get(info) {
                    Type::Annotated(seq, _) if repeated => {
                        self.syms.repeated_params.insert(s, info);
                        seq
                    }
                    _ => info,
                }
            }
            Tree::TypeDef { rhs, .. } => self.type_def_info(rhs, ctx.with_owner(s)),
            _ => self.u.intern_type(Type::Error),
        };
        self.u.set_info(s, info);
        if self.u.is_type_symbol(s) {
            self.check_non_cyclic(s, info, p);
        } else {
            self.mark_erased_by_type(s, p, info);
        }
    }
}
