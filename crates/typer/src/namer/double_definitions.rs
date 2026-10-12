//! Definitions entered twice in one owner (dotty's `Namer.checkNoConflict`, and in a class
//! `Checking.checkNoDoubleDeclaration` as far as values go).

use crate::ctx::{Ctx, Level};
use crate::typer::Typer;
use syntax::ast::NodeId;
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// Whether definition `tree` of `name` and `kind` in `ctx` conflicts with a definition this
    /// unit entered already: in the owner if it is a class or package, else in the scope of
    /// `ctx`. Two methods are left to `check_overrides`; in a class, a value and a method
    /// conflict only when all clauses of the method are empty.
    pub fn doubly_defined(&mut self, tree: NodeId, name: &str, kind: Kind, ctx: Ctx) -> bool {
        if name == "$anon" {
            return false;
        }
        let owner = ctx.owner;
        let is_type = !kind.is_term();
        let in_class = self.u.symbol(owner).kind == Kind::Class;
        let getter = name.contains("$default$");
        let scope =
            self.levels.def_scope(ctx.level).filter(|&l| matches!(self.levels.get(l), Level::Scope(_)));
        let decls = match scope {
            Some(l) if !in_class && self.u.symbol(owner).kind != Kind::Package => {
                self.levels.scope_symbols(l)
            }
            _ => self.u.symbol(owner).decls.clone(),
        };
        for d in decls {
            let s = self.u.symbol(d);
            if !self.syms.flags.contains_key(&d) || s.name != name || self.u.is_type_symbol(d) != is_type {
                continue;
            }
            let conflicts = match (kind, s.kind) {
                _ if is_type || getter => true,
                (Kind::Def, Kind::Val) => !in_class || self.empty_clauses_def(tree),
                (Kind::Def, _) => false,
                (_, Kind::Def) => !in_class || self.empty_clauses(d),
                _ => true,
            };
            if conflicts {
                return true;
            }
        }
        false
    }

    /// Whether every parameter clause of method `s` is empty, type-parameter clauses included.
    fn empty_clauses(&mut self, s: SymbolId) -> bool {
        use crate::symbols::Completion;
        if let Some(&(Completion::Pending { tree, .. } | Completion::Completing(tree))) =
            self.syms.completions.get(&s)
        {
            return self.empty_clauses_def(tree);
        }
        let mut info = self.u.info(s);
        loop {
            match self.u.types().get(info) {
                Type::Method { params, result, .. } if params.is_empty() => info = *result,
                Type::Method { .. } | Type::Poly { .. } => return false,
                _ => return true,
            }
        }
    }

    /// Whether every parameter clause of method definition `tree` is empty.
    fn empty_clauses_def(&self, tree: NodeId) -> bool {
        self.def_clauses(tree).iter().all(Vec::is_empty)
    }
}
