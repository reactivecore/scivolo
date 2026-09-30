//! Enum classes and enum case classes: the cases moved out of the enum's body, the type
//! parameters a case takes over from its enum, and its parent.

use super::Desugar;
use super::class_params::Keep;
use crate::ast::children::Child;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::DerivedKind;
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// Whether `t` is an enum case: a value, class or object case.
    pub fn is_enum_case(&self, t: NodeId) -> bool {
        self.ast.mods_of(t).is_some_and(|m| self.ast.modifiers(m).flags.is_all_of(flags::ENUM_CASE))
    }

    /// The body of enum class `cdef` named `name`: an import of its cases from the companion,
    /// then the other statements; and the cases, the last marked to define the lookups.
    pub fn enum_body(&mut self, cdef: NodeId, name: TName, body: &[NodeId]) -> (Vec<NodeId>, Vec<NodeId>) {
        let (cases, stats): (Vec<NodeId>, Vec<NodeId>) = body.iter().partition(|&&s| self.is_enum_case(s));
        match cases.last() {
            Some(&last) => self.ast.add_attrs(last, Attrs::ENUM_LOOKUPS),
            None => {
                let span = self.name_span(cdef, name);
                self.diags.error_at(
                    span.start(),
                    span.end(),
                    span.point(),
                    "Enumerations must contain at least one case",
                );
            }
        }
        let watched = Some(TName { is_type: false, ..name });
        let companion = self.ast.node(Tree::DerivedTypeTree { kind: DerivedKind::TermRef, watched });
        let mut selectors = Vec::new();
        for &case in &cases {
            let ids = match *self.ast.tree(case) {
                Tree::PatDef { pats, .. } => self.ast.items(pats).to_vec(),
                _ => {
                    let name = TName { is_type: false, ..self.name_of_def(case) };
                    vec![self.ident(name)]
                }
            };
            for id in ids {
                let start = self.ast.span(id).start_pos();
                let imported = self.ast.respan(id, start);
                selectors.push(self.ast.node(Tree::ImportSelector { imported, renamed: None, bound: None }));
            }
        }
        let selectors = self.list(&selectors);
        let import = self.ast.node(Tree::Import { expr: companion, selectors });
        ([&[import][..], &stats].concat(), cases)
    }

    /// The type parameters an enum case class without its own takes over from its enum: those
    /// of the enum, if the case refers to them or has no parent.
    pub fn enum_case_tparams(
        &mut self,
        tparams: &[NodeId],
        vparamss: &[Vec<NodeId>],
        parents: &[NodeId],
    ) -> Vec<NodeId> {
        let enum_tparams = self.enum_class().map(|c| self.classes[c].tparams.clone()).unwrap_or_default();
        let names: Vec<TName> = enum_tparams.iter().map(|&(n, _)| n).collect();
        let referenced = self.type_param_is_referenced(&names, tparams, vparamss, parents);
        if !tparams.is_empty() || !parents.is_empty() && !referenced {
            return Vec::new();
        }
        enum_tparams
            .into_iter()
            .map(|(name, variance)| {
                let rhs =
                    self.ast.node(Tree::DerivedTypeTree { kind: DerivedKind::FromParam, watched: None });
                let flags = flags::TYPE_PARAM.or(flags::SYNTHETIC).or(variance).or(flags::PRIVATE_LOCAL);
                let mods = self.ast.mods(Modifiers::with_flags(flags));
                self.ast.node(Tree::TypeDef { name, rhs, mods })
            })
            .collect()
    }

    /// Whether the value parameters or parents of an enum case refer to a type parameter of
    /// the enum, with an error if the case has its own type parameters or no value parameters
    /// (dotty's `typeParamIsReferenced`).
    pub fn type_param_is_referenced(
        &mut self,
        enum_tparams: &[TName],
        case_tparams: &[NodeId],
        vparamss: &[Vec<NodeId>],
        parents: &[NodeId],
    ) -> bool {
        let report = !case_tparams.is_empty() || vparamss.is_empty();
        let bound: Vec<TName> = case_tparams.iter().filter_map(|&t| self.def_name(t)).collect();
        let names: Vec<TName> = enum_tparams.iter().copied().filter(|n| !bound.contains(n)).collect();
        for &p in vparamss.iter().flatten() {
            if let Tree::ValDef { tpt, .. } = *self.ast.tree(p)
                && self.refers_to(tpt, &names, report)
            {
                return true;
            }
        }
        parents.iter().any(|&p| self.parent_refers_to(p, &names, report))
    }

    fn parent_refers_to(&mut self, parent: NodeId, names: &[TName], report: bool) -> bool {
        match *self.ast.tree(parent) {
            Tree::Apply { fun, .. } => self.parent_refers_to(fun, names, report),
            Tree::TypeApply { args, .. } => {
                self.ast.items(args).to_vec().into_iter().any(|a| self.refers_to(a, names, report))
            }
            Tree::Select { qualifier, name } if self.text(name) == "<init>" => {
                self.parent_refers_to(qualifier, names, report)
            }
            Tree::New { tpt } => self.refers_to(tpt, names, report),
            _ => self.ast.is_type(parent) && self.refers_to(parent, names, report),
        }
    }

    /// Whether `t` refers to one of `names`, not counting names its lambdas and refinements
    /// rebind; with an error at the reference if `report`.
    fn refers_to(&mut self, t: NodeId, names: &[TName], report: bool) -> bool {
        let rebound: Vec<TName> = match *self.ast.tree(t) {
            Tree::Ident { name } => {
                let found = names.contains(&name);
                if found && report {
                    let msg =
                        format!("illegal reference to type parameter {} from enum case", self.text(name));
                    self.error(t, &msg);
                }
                return found;
            }
            Tree::LambdaTypeTree { tparams: l, .. } | Tree::RefinedTypeTree { refinements: l, .. } => {
                self.ast.items(l).iter().filter_map(|&d| self.def_name(d)).collect()
            }
            _ => Vec::new(),
        };
        let names: Vec<TName> = names.iter().copied().filter(|n| !rebound.contains(n)).collect();
        let mut subtrees = Vec::new();
        for &c in self.ast.children(t).iter() {
            match c {
                Child::Node(n) => subtrees.push(n),
                Child::List(l) => subtrees.extend_from_slice(self.ast.items(l)),
                Child::Clauses(cs) => {
                    for &l in self.ast.clause_items(cs) {
                        subtrees.extend_from_slice(self.ast.items(l));
                    }
                }
                Child::Empty | Child::Mods(_) => {}
            }
        }
        subtrees.into_iter().any(|s| self.refers_to(s, &names, report))
    }

    /// The parent of an enum case class without an explicit one: the enum class, applied to the
    /// case's type parameters if the enum has any.
    pub fn enum_class_type_ref(
        &mut self,
        cdef: NodeId,
        has_own_tparams: bool,
        constr_tparams: &[NodeId],
    ) -> NodeId {
        let arity = self.enum_class().map_or(0, |c| self.classes[c].tparams.len());
        let class = self.enum_class_ref();
        if arity == 0 {
            return class;
        }
        if !has_own_tparams {
            return self.applied_ref(cdef, constr_tparams, class);
        }
        let start = self.ast.span(cdef).start_pos();
        let msg = "explicit extends clause needed because both enum case and enum class have type parameters";
        self.diags.error_at(start.start(), start.end(), start.point(), msg);
        let args: Vec<NodeId> = constr_tparams.iter().map(|_| self.sym_ref("scala.AnyRef", true)).collect();
        let args = self.list(&args);
        let applied = self.ast.node(Tree::AppliedTypeTree { tpt: class, args });
        self.ast.respan(applied, start)
    }

    /// Default getter `getter` with the type parameters an enum case takes over from its enum in
    /// place of its own.
    pub fn with_enum_tparams(&mut self, getter: NodeId, enum_tparams: &[NodeId]) -> NodeId {
        let tparams: Vec<NodeId> = enum_tparams.iter().map(|&t| self.meth_type_param(t, Keep::All)).collect();
        let clauses = self.clauses_of(getter);
        let skip = usize::from(clauses.first().is_some_and(|&l| self.is_type_clause(l)));
        let trailing: Vec<Vec<NodeId>> =
            clauses[skip..].iter().map(|&l| self.ast.items(l).to_vec()).collect();
        let joined = self.join_params(&tparams, &trailing);
        self.with_clauses(getter, &joined)
    }

    /// Whether `def` was marked to define the enum's lookups; the mark is removed.
    pub fn defines_enum_lookups(&mut self, def: NodeId) -> bool {
        let marked = self.ast.attrs(def).contains(Attrs::ENUM_LOOKUPS);
        self.ast.remove_attrs(def, Attrs::ENUM_LOOKUPS);
        marked
    }
}
