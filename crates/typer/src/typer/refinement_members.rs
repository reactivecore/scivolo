// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The members of refined type trees, definitions in a synthetic `<refinement>` class (dotty's
//! `refinedTypeToClass`, `typedRefinedTypeTree`).

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::TNodeId;
use syntax::ast::flags::{self as f, GIVEN, IMPLICIT, MUTABLE};
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

/// A refinement: its name, its info and its typed definition.
pub type Refinement = (String, TypeId, TNodeId);

impl Typer<'_> {
    /// The class the members of a refined type tree typed in `ctx` belong to.
    pub fn refinement_class(&mut self, ctx: Ctx) -> SymbolId {
        let class = self.syms.define(self.u, "<refinement>", Kind::Class, ctx.owner, f::SYNTHETIC);
        self.u.unenter(class);
        class
    }

    /// Type refinement `r` of right-hand side `rhs`, a member of `class`, its info from
    /// `type_def_info`; `Err` with the tree of a refusal.
    pub fn type_refinement(&mut self, class: SymbolId, r: NodeId, ctx: Ctx) -> Result<Refinement, TNodeId> {
        let Tree::TypeDef { name, rhs, mods } = *self.ast.tree(r) else { unreachable!("a type refinement") };
        let text = self.text(name.name).to_owned();
        let flags = self.ast.modifiers(mods).flags.to_type_flags();
        let sym = self.syms.define(self.u, &text, Kind::Type, class, flags);
        let mctx = Ctx { owner: sym, ..ctx };
        let info = self.type_def_info(rhs, mctx);
        let info = match *self.u.types().get(info) {
            Type::Error => return Err(self.unsupported(r, "refinement of an unsupported type")),
            Type::Lambda { .. } => self.u.intern_type(Type::Alias(info)),
            _ => info,
        };
        if let Type::Bounds(lo, hi) = *self.u.types().get(info)
            && !self.u.is_subtype(lo, hi)
        {
            return Err(self.unsupported(r, "refinement with conflicting bounds"));
        }
        self.u.set_info(sym, info);
        self.syms.by_tree.insert(r, sym);
        let tree = self.typed_type_def(r, mctx);
        Ok((text, info, tree))
    }

    /// Term refinement `r`, a member of `class`: a value's type, a parameterless method's
    /// by-name result, a method type as its info; `Err` for one that is not a plain value or
    /// method.
    pub fn term_refinement(
        &mut self,
        class: SymbolId,
        r: NodeId,
        ctx: Ctx,
    ) -> Result<Refinement, &'static str> {
        let (name, clauses, tpt, mods) = match *self.ast.tree(r) {
            Tree::ValDef { name, tpt, mods, .. } => (name, None, tpt, mods),
            Tree::DefDef { name, paramss, tpt, mods, .. } => (name, Some(paramss), tpt, mods),
            _ => return Err("structural refinement"),
        };
        let flags = self.ast.modifiers(mods).flags;
        if flags.is_one_of(MUTABLE.or(GIVEN).or(IMPLICIT)) {
            return Err("structural refinement with modifiers");
        }
        let text = self.text(name.name).to_owned();
        let (kind, flags) = match clauses {
            Some(_) => (Kind::Def, flags.or(f::METHOD)),
            None => (Kind::Val, flags),
        };
        let sym = self.syms.define(self.u, &text, kind, class, flags.to_term_flags());
        let mut mctx = Ctx { owner: sym, ..ctx };
        let mut clause_trees = Vec::new();
        let mut clause_params = Vec::new();
        for clause in clauses.map(|c| self.ast.clause_items(c).to_vec()).unwrap_or_default() {
            let (mut trees, mut params, mut kind) = (Vec::new(), Vec::new(), (false, false));
            for p in self.ast.items(clause).to_vec() {
                let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else {
                    return Err("polymorphic structural refinement");
                };
                let pflags = self.ast.modifiers(mods).flags;
                kind = (pflags.is_one_of(GIVEN.or(IMPLICIT)), pflags.is(GIVEN));
                if matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. }) {
                    return Err("structural refinement with a by-name parameter");
                }
                let typed = self.typed_type(tpt, mctx);
                let tpe = self.tpd.tpe(typed);
                if matches!(self.u.types().get(tpe), Type::Error) {
                    return Err("structural refinement of an unsupported type");
                }
                let ptext = self.text(name.name).to_owned();
                let psym = self.syms.define(
                    self.u,
                    &ptext,
                    Kind::Param,
                    sym,
                    pflags.or(f::TERM_PARAM).to_term_flags(),
                );
                self.u.set_info(psym, tpe);
                let level = self.levels.push(Some(mctx.level), Level::Scope(vec![(ptext.clone(), psym)]));
                mctx = Ctx { level, ..mctx };
                let pspan = self.ast.span(p);
                trees.push(self.build().val_def(psym, typed, None, pspan));
                params.push((ptext, tpe));
            }
            clause_trees.push(trees);
            clause_params.push((params, kind));
        }
        let result = self.typed_type(tpt, mctx);
        let mut info = self.tpd.tpe(result);
        if matches!(self.u.types().get(info), Type::Error) {
            return Err("structural refinement of an unsupported type");
        }
        match clauses {
            Some(_) if clause_params.is_empty() => info = self.u.intern_type(Type::ByName(info)),
            _ => {
                for (params, (implicit, contextual)) in clause_params.into_iter().rev() {
                    let binder = self.u.fresh_binder();
                    let method = Type::Method { binder, params, result: info, implicit, contextual };
                    info = self.u.intern_type(method);
                }
            }
        }
        self.u.set_info(sym, info);
        let span = self.ast.span(r);
        let tree = match clauses {
            Some(_) => {
                let refs: Vec<&[TNodeId]> = clause_trees.iter().map(Vec::as_slice).collect();
                self.build().def_def(sym, &refs, result, None, span)
            }
            None => self.build().val_def(sym, result, None, span),
        };
        Ok((text, info, tree))
    }
}
