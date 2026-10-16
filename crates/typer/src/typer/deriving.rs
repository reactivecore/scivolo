//! Type class instances of `derives` clauses (dotty's `Deriving`): `given derived$TC: TC[C] =
//! TC.derived` in the companion of `C`, entered with the companion and typed after its body.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

/// Why an instance's `TC.derived` cannot be called: the type class, or its companion's path.
pub enum Problem {
    NoCompanion(String),
    ExplicitParams(String),
}

impl Typer<'_> {
    /// Enters into module class `cls` the instances of its `derives` clause `derived`, for its
    /// companion class if `companion`, else for itself (dotty's `Deriver.enterDerived`).
    pub fn enter_derived(&mut self, cls: SymbolId, companion: bool, derived: &[NodeId], ctx: Ctx) {
        let deriving = if companion { self.u.companion_class(cls).unwrap_or(cls) } else { cls };
        let mut defs = Vec::new();
        let mut names = Vec::new();
        for &d in derived {
            if matches!(self.ast.tree(d), Tree::UseRef { .. }) {
                self.unsupported(d, "uses clause");
                continue;
            }
            if let Some(def) = self.derived_instance(deriving, d, &mut names, ctx) {
                defs.push(def);
            }
        }
        let nodes: Vec<NodeId> = defs.iter().map(|&(d, _)| d).collect();
        self.index(&nodes, ctx);
        self.derived_instances.insert(cls, defs);
    }

    /// The definition of the instance of class `cls` for type class tree `d` (dotty's
    /// `processDerivedInstance` and `typeclassInstance`).
    fn derived_instance(
        &mut self,
        cls: SymbolId,
        d: NodeId,
        names: &mut Vec<String>,
        ctx: Ctx,
    ) -> Option<(NodeId, Option<Problem>)> {
        let typed = self.typed_type(d, ctx);
        let original = self.tpd.tpe(typed);
        if matches!(self.u.types().get(original), Type::Error) {
            return None;
        }
        let Some(tc_type) = self.underlying_class_ref(original, 16) else {
            let shown = self.show_type(original);
            self.error(d, format!("{shown} is not a class type"));
            return None;
        };
        let Type::Sym(prefix, tc) = *self.u.types().get(tc_type) else { unreachable!("a class reference") };
        let Type::Sym(_, written) = *self.u.types().get(original) else { unreachable!("a type reference") };
        let name = format!("derived${}", self.u.symbol(written).name);
        let declared =
            self.u.decls(ctx.owner).unwrap_or_default().iter().any(|&s| self.u.symbol(s).name == name);
        if declared || names.contains(&name) {
            let tc_name = self.u.symbol(written).name.clone();
            self.error(d, format!("duplicate type class derivation for {tc_name}"));
            return None;
        }
        names.push(name.clone());
        let _ = self.u.decls(tc);
        let tc_params = self.u.type_params(tc);
        let span = Span::at(self.ast.span(d).point()).to_synthetic();
        let cls_name = self.u.symbol(cls).name.clone();
        let tc_name = self.u.symbol(tc).name.clone();
        let cannot_be_unified = format!("{cls_name} cannot be unified with the type argument of {tc_name}");
        let instance = match tc_params.len() {
            1 => self.derive_single_parameter(cls, tc_params[0], span),
            _ if self.u.full_name(tc) == "scala.CanEqual" => self.derive_can_equal(cls, &tc_params, span),
            0 => Err(format!("type {tc_name} in derives clause of {cls_name} has no type parameters")),
            _ => Err(cannot_be_unified.clone()),
        };
        let instance = match instance {
            Ok(Some(i)) => i,
            Ok(None) => {
                self.error(d, cannot_be_unified);
                return None;
            }
            Err(msg) => {
                self.error(d, msg);
                return None;
            }
        };
        let (rhs, problem) = match self.u.companion_module(tc) {
            None => {
                let kind = if self.u.is_trait(tc) { "trait" } else { "class" };
                (
                    self.derived_node(Tree::TypeTree, span),
                    Some(Problem::NoCompanion(format!("{kind} {tc_name}"))),
                )
            }
            Some(module) => {
                let module_ref = self.u.intern_type(Type::Sym(prefix, module));
                let Some(qualifier) = self.path_tree(module_ref, span, 8) else {
                    self.unsupported(d, "derives clause of a type class without a stable path");
                    return None;
                };
                let explicit = self.derived_takes_explicit_params(module);
                let shown = self.ref_text(module_ref);
                let qualifier = self.spliced(qualifier);
                let derived = TName { name: self.name("derived"), is_type: false };
                let rhs = self.derived_node(Tree::Select { qualifier, name: derived }, span);
                (rhs, explicit.then_some(Problem::ExplicitParams(shown)))
            }
        };
        let tpt = self.tc_applied(tc_type, instance.args, span);
        let name = TName { name: self.name(&name), is_type: false };
        let evidence: Vec<NodeId> = instance
            .evidence
            .into_iter()
            .enumerate()
            .map(|(i, args)| {
                let tpt = self.tc_applied(tc_type, args, span);
                let name = TName { name: self.name(&format!("x${i}")), is_type: false };
                let mods = self.ast.mods(Modifiers::with_flags(f::TERM_PARAM.or(f::IMPLICIT)));
                self.derived_node(Tree::ValDef { name, tpt, rhs: None, mods }, span)
            })
            .collect();
        if instance.tparams.is_empty() && evidence.is_empty() {
            let mods = self.ast.mods(Modifiers::with_flags(f::GIVEN.or(f::LAZY)));
            return Some((
                self.derived_node(Tree::ValDef { name, tpt, rhs: Some(rhs), mods }, span),
                problem,
            ));
        }
        let clauses: Vec<_> =
            [instance.tparams, evidence].iter().filter(|c| !c.is_empty()).map(|c| self.ast.list(c)).collect();
        let paramss = self.ast.clauses(&clauses);
        let mods = self.ast.mods(Modifiers::with_flags(f::GIVEN));
        Some((self.derived_node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs), mods }, span), problem))
    }

    /// Appends to the body of each module class among typed statements `stats`, typed in `ctxs`,
    /// the instances of its `derives` clause (dotty's `Deriver.finalize`).
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

    /// The instances of module class `cls`'s `derives` clause, typed in `ctx` (dotty's
    /// `Deriver.syntheticDefs`); one whose type class's `derived` cannot be called reported.
    fn typed_derived_instances(&mut self, cls: SymbolId, ctx: Ctx) -> Vec<TNodeId> {
        let mut out = Vec::new();
        for (d, problem) in self.derived_instances.remove(&cls).unwrap_or_default() {
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
        out
    }

    /// The text of path type `t` as a reference to it shows it: `a.b.C`.
    fn ref_text(&mut self, t: TypeId) -> String {
        match *self.u.types().get(t) {
            Type::Sym(Some(p), s) if matches!(self.u.types().get(p), Type::Sym(_, q) if self.u.symbol(*q).kind.is_term()) =>
            {
                format!("{}.{}", self.ref_text(p), self.u.symbol(s).name)
            }
            Type::Sym(_, s) => self.u.symbol(s).name.clone(),
            _ => self.show_type(t),
        }
    }

    /// Whether the only `derived` member of type class companion `module` takes explicit term
    /// parameters (dotty's `hasExplicitParams`).
    fn derived_takes_explicit_params(&mut self, module: SymbolId) -> bool {
        let Some(class) = self.u.module_class(module) else { return false };
        let [m] = self.u.member(class, "derived")[..] else { return false };
        self.complete(m);
        let mut info = self.u.info(m);
        loop {
            match *self.u.types().get(info) {
                Type::Poly { result, .. } => info = result,
                Type::Method { implicit: false, .. } => return true,
                Type::Method { result, .. } => info = result,
                _ => return false,
            }
        }
    }

    /// The class reference under type `t`, also of a higher-kinded alias (dotty's
    /// `underlyingClassRef`).
    fn underlying_class_ref(&mut self, t: TypeId, depth: u32) -> Option<TypeId> {
        let depth = depth.checked_sub(1)?;
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class => Some(t),
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Type => {
                self.complete(s);
                let info = self.u.info(s);
                match *self.u.types().get(info) {
                    Type::Alias(a) => self.underlying_class_ref(a, depth),
                    Type::Lambda { .. } => self.underlying_class_ref(info, depth),
                    _ => None,
                }
            }
            Type::Lambda { result, .. } => self.underlying_class_ref(result, depth),
            Type::Applied(tycon, _) => self.underlying_class_ref(tycon, depth),
            _ => None,
        }
    }
}
