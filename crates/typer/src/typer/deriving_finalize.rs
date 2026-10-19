//! Derived instances typed after the statements around their companion (dotty's
//! `Deriver.finalize`, `syntheticDefs`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::SymbolId;

/// Why an instance's `TC.derived` cannot be called: the type class, or its companion's path.
pub enum Problem {
    NoCompanion(String),
    ExplicitParams(String),
}

impl Typer<'_> {
    /// Appends to the body of each module class among typed statements `stats`, typed in `ctxs`,
    /// the instances of its `derives` clause.
    pub fn finalize_derived(&mut self, stats: &[TNodeId], ctxs: &[Ctx]) {
        for (&s, &ctx) in stats.iter().zip(ctxs) {
            let T::TypeDef { rhs, .. } = *self.tpd.tree(s) else { continue };
            let Some(cls) = self.tpd.sym(s).filter(|c| self.derived_instances.contains_key(c)) else {
                continue;
            };
            let cctx = self.class_ctx(cls, ctx);
            let derived = self.typed_derived_instances(cls, cctx);
            let T::Template { constr, parents, self_, body } = *self.tpd.tree(rhs) else { continue };
            let body = [self.tpd.items(body), &derived].concat();
            let body = self.tpd.list(&body);
            self.tpd.get_mut(rhs).tree = T::Template { constr, parents, self_, body };
        }
    }

    /// The instances of module class `cls`'s `derives` clause, typed in `ctx`; one whose type
    /// class's `derived` cannot be called reported.
    fn typed_derived_instances(&mut self, cls: SymbolId, ctx: Ctx) -> Vec<TNodeId> {
        let mut out = Vec::new();
        let instances = self.derived_instances.remove(&cls).unwrap_or_default();
        let defs: Vec<_> = instances.iter().map(|&(d, _)| d).collect();
        for (d, problem) in instances {
            if let Some(problem) = problem {
                let sym = self.syms.by_tree[&d];
                self.complete(sym);
                let result = match self.syms.def_results.get(&sym) {
                    Some(&r) => r,
                    None => self.u.info(sym),
                };
                let shown = self.show_type(result);
                let msg = match problem {
                    Problem::NoCompanion(tc) => {
                        format!("{shown} cannot be derived since {tc} has no companion object")
                    }
                    Problem::ExplicitParams(module) => format!(
                        "derived instance {shown} failed to generate:\nmethod `derived` from object {module} takes explicit term parameters"
                    ),
                };
                self.error_at(self.ast.span(d), msg);
                continue;
            }
            let typed = self.typed_stat(d, ctx);
            out.push(self.with_plain_type_trees(typed));
        }
        self.derived_lambdas.retain(|_, def| !defs.contains(def));
        out
    }
}
