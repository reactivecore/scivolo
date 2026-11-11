// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Definitions entered twice in one owner (dotty's `Namer.checkNoConflict`, and in a class
//! `Checking.checkNoDoubleDeclaration` as far as values go).

use crate::ctx::{Ctx, Level};
use crate::typer::Typer;
use names::{name_kinds, nme};
use syntax::ast::NodeId;
use syntax::ast::flags as f;
use tasty::Kind;

impl Typer<'_> {
    /// Whether definition `tree` of `name` and `kind` in `ctx` conflicts with a definition this
    /// unit entered already: in the owner if it is a class or package, else in the scope of
    /// `ctx`. Two methods are left to `check_overrides`; in a class, a value and a method
    /// conflict only when all clauses of the method are empty.
    pub fn doubly_defined(&mut self, tree: NodeId, name: &str, kind: Kind, ctx: Ctx) -> bool {
        if name == nme::ANON_CLASS {
            return false;
        }
        let owner = ctx.owner;
        let is_type = !kind.is_term();
        let in_class = self.u.symbol(owner).kind == Kind::Class;
        let getter = name.contains(name_kinds::DEFAULT_GETTER);
        let scope =
            self.levels.def_scope(ctx.level).filter(|&l| matches!(self.levels.get(l), Level::Scope(_)));
        let decls = match scope {
            Some(l) if !in_class && self.u.symbol(owner).kind != Kind::Package => {
                self.levels.scope_symbols(l)
            }
            _ => self.u.symbol(owner).decls.clone(),
        };
        let synthetic_tree = match *self.ast.tree(tree) {
            syntax::ast::Tree::DefDef { mods, .. } => self.ast.modifiers(mods).flags.is(f::SYNTHETIC),
            _ => false,
        };
        let retractable = in_class && self.is_retractable_name(owner, name);
        for d in decls {
            let s = self.u.symbol(d);
            if !self.syms.flags.contains_key(&d) || s.name != name || self.u.is_type_symbol(d) != is_type {
                continue;
            }
            let synthetic = self.syms.flags.get(&d).is_some_and(|fl| fl.is(f::SYNTHETIC));
            if retractable && (synthetic || synthetic_tree) {
                continue;
            }
            let conflicts = match (kind, s.kind) {
                _ if is_type || getter => true,
                (Kind::Def, Kind::Val) => !in_class || self.is_nullary_def(tree),
                (Kind::Def, _) => false,
                (_, Kind::Def) => !in_class || self.is_nullary(d),
                _ => true,
            };
            if conflicts {
                return true;
            }
        }
        false
    }
}
