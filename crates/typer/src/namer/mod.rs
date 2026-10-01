//! Entering the symbols of definitions before their statements are typed (dotty's `Namer`).

mod checks;
mod complete;

use crate::ctx::{Ctx, Level};
use crate::symbols::Completion;
use crate::typer::Typer;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Enters the symbols `stats` define in `ctx`'s owner and, if it is a scope, in its level;
    /// each import opens a level the statements after it are completed in.
    pub fn index(&mut self, stats: &[NodeId], mut ctx: Ctx) {
        for &stat in stats {
            if matches!(self.ast.tree(stat), Tree::Import { .. }) {
                ctx.level = self.import_level(stat, ctx);
                continue;
            }
            if let Some(sym) = self.create_symbol(stat, ctx)
                && let Some(scope) = self.levels.def_scope(ctx.level)
                && matches!(self.levels.get(scope), Level::Scope(_))
            {
                let name = self.u.symbol(sym).name.clone();
                self.levels.enter(scope, &name, sym);
            }
        }
    }

    /// The context of the statements after `stat`: the level of an import, else `ctx`.
    pub fn after_stat(&self, stat: NodeId, ctx: Ctx) -> Ctx {
        match self.import_levels.get(&stat) {
            Some(&level) => Ctx { level, ..ctx },
            None => ctx,
        }
    }

    /// The symbol definition `tree` defines, completed later in `ctx` (dotty's `createSymbol`).
    fn create_symbol(&mut self, tree: NodeId, ctx: Ctx) -> Option<SymbolId> {
        let (name, mods, kind, extra) = match *self.ast.tree(tree) {
            Tree::ValDef { name, mods, rhs, .. } => {
                let m = self.ast.modifiers(mods).flags;
                let deferred = rhs.is_none() && !m.is_one_of(f::TERM_PARAM.or(f::PARAM_ACCESSOR));
                (name, mods, Kind::Val, if deferred { f::DEFERRED } else { f::EMPTY })
            }
            Tree::DefDef { name, mods, rhs, .. } => {
                let constructor = self.text(name.name) == "<init>";
                let deferred = rhs.is_none() && !constructor;
                (name, mods, Kind::Def, if deferred { f::METHOD.or(f::DEFERRED) } else { f::METHOD })
            }
            Tree::TypeDef { name, mods, rhs } if matches!(self.ast.tree(rhs), Tree::Template { .. }) => {
                (name, mods, Kind::Class, f::EMPTY)
            }
            Tree::TypeDef { name, mods, rhs } => {
                let param = self.ast.modifiers(mods).flags.is(f::TYPE_PARAM);
                let abstract_ = matches!(self.ast.tree(rhs), Tree::TypeBoundsTree { .. });
                let kind = if param { Kind::TypeParam } else { Kind::Type };
                (name, mods, kind, if abstract_ && !param { f::DEFERRED } else { f::EMPTY })
            }
            _ => return None,
        };
        let written: FlagSet = self.ast.modifiers(mods).flags.or(extra);
        let text = self.text(name.name).to_owned();
        self.check_definition(tree, &text, kind, ctx.owner, written);
        if self.ast.modifiers(mods).private_within.is_some() {
            self.unsupported(tree, "qualified private or protected");
        }
        let mut flags = if kind.is_term() { written.to_term_flags() } else { written.to_type_flags() };
        if flags.is(f::PRIVATE) && text != "<init>" && !flags.is_one_of(f::PARAM.or(f::PARAM_ACCESSOR)) {
            flags = flags.or(f::LOCAL);
        }
        if flags.is(f::PRIVATE) && self.is_package_level(ctx.owner) {
            flags = flags.and_not(f::PRIVATE_LOCAL);
        }
        let sym = self.syms.define(self.u, &text, kind, ctx.owner, flags);
        let completion_ctx = match self.levels.outer(ctx.level) {
            Some(outer) if text == "<init>" => Ctx { level: outer, ..ctx },
            _ => ctx,
        };
        self.syms.completions.insert(sym, Completion::Pending { tree, ctx: completion_ctx });
        self.syms.by_tree.insert(tree, sym);
        self.index_annotations(sym, mods, ctx);
        Some(sym)
    }

    /// Whether `owner` is a package or a package object, whose private members are private to the
    /// package (dotty's `skipWeakOwner`).
    fn is_package_level(&self, owner: SymbolId) -> bool {
        self.u.symbol(owner).kind == Kind::Package || self.is_package_object(owner)
    }

    /// Records the classes of the annotations in `mods` on `sym`.
    fn index_annotations(&mut self, sym: SymbolId, mods: syntax::ast::tree::ModsId, ctx: Ctx) {
        let annots = self.ast.items(self.ast.modifiers(mods).annotations).to_vec();
        let mut classes = Vec::new();
        for annot in annots {
            let mut t = annot;
            while let Tree::Apply { fun: inner, .. } | Tree::Select { qualifier: inner, .. } =
                *self.ast.tree(t)
            {
                t = inner;
            }
            let Tree::New { tpt } = *self.ast.tree(t) else {
                self.unsupported(annot, "annotation");
                continue;
            };
            let typed = self.typed_type(tpt, ctx);
            let tpe = self.tpd.tpe(typed);
            let class = self.u.class_symbol(tpe);
            self.check_annotation(annot, class);
            let span = self.ast.span(annot);
            classes.extend(class.map(|c| (c, span)));
        }
        if !classes.is_empty() {
            self.syms.annotations.insert(sym, classes);
        }
    }
}
