//! What the expansion of a class definition is made of: its desugared constructors, the
//! parameter copies, the body, and the reference to the class (the first half of dotty's
//! `classDef`).

use super::Desugar;
use super::class_params::Keep;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::DerivedKind;
use crate::ast::{NodeId, TName, Tree};

pub struct ClassParts {
    pub cdef: NodeId,
    pub name: TName,
    pub mods: Modifiers,
    pub template: NodeId,
    pub parents: Vec<NodeId>,
    pub derived: Vec<NodeId>,
    /// The `uses` clause, after the derived classes.
    pub uses: Vec<NodeId>,
    pub constr1: NodeId,
    pub constr: NodeId,
    pub default_getters: Vec<NodeId>,
    pub original_tparams: Vec<NodeId>,
    /// The type parameters of an enum case taken over from its enum, then the original ones.
    pub implied_tparams: Vec<NodeId>,
    pub original_vparamss: Vec<Vec<NodeId>>,
    pub constr_tparams: Vec<NodeId>,
    pub constr_vparamss: Vec<Vec<NodeId>>,
    pub derived_tparams: Vec<NodeId>,
    pub derived_vparamss: Vec<Vec<NodeId>>,
    pub body: Vec<NodeId>,
    /// The cases of an enum class, moved to its companion.
    pub enum_cases: Vec<NodeId>,
    pub class_tycon: NodeId,
    pub class_type_ref: NodeId,
    pub creator: Option<NodeId>,
}

impl ClassParts {
    pub fn is(&self, flag: flags::FlagSet) -> bool {
        self.mods.is(flag)
    }

    pub fn is_case_class(&self) -> bool {
        self.is(flags::CASE) && !self.is(flags::MODULE)
    }

    pub fn arity(&self) -> usize {
        self.constr_vparamss.first().map_or(0, Vec::len)
    }
}

impl Desugar<'_> {
    /// The parts of class definition `cdef`.
    pub fn class_parts(&mut self, cdef: NodeId) -> ClassParts {
        let Tree::TypeDef { name, rhs: template, mods } = *self.ast.tree(cdef) else {
            unreachable!("a class")
        };
        let Tree::Template { constr, parents_or_derived, body: stats, derived, .. } =
            *self.ast.tree(template)
        else {
            unreachable!("a template")
        };
        let name = TName { is_type: true, ..self.normalize_name(name, template) };
        let mods = self.ast.modifiers(mods).clone();
        let items = self.ast.items(parents_or_derived).to_vec();
        let (parents, derived) = items.split_at(items.len() - derived.unwrap_or(0) as usize);
        let parents = parents.to_vec();
        let own = derived.iter().take_while(|&&d| !matches!(self.ast.tree(d), Tree::UseRef { .. })).count();
        let (derived, uses) = (derived[..own].to_vec(), derived[own..].to_vec());
        let expanded = self.def_def(constr, true);
        let (constr1, mut default_getters) = self.decompose(expanded);
        let clauses = self.clauses_of(constr1);
        let (original_tparams, original_vparamss) = match clauses.split_first() {
            Some((first, rest)) if self.is_type_clause(*first) => {
                (self.ast.items(*first).to_vec(), rest.to_vec())
            }
            _ => (Vec::new(), clauses),
        };
        let original_vparamss: Vec<Vec<NodeId>> =
            original_vparamss.iter().map(|&l| self.ast.items(l).to_vec()).collect();
        let enum_tparams = if mods.flags.is_all_of(flags::ENUM_CASE) {
            self.enum_case_tparams(&original_tparams, &original_vparamss, &parents)
        } else {
            Vec::new()
        };
        let implied_tparams = [&enum_tparams[..], &original_tparams].concat();
        if mods.is(flags::TRAIT) {
            for &vparam in original_vparamss.iter().flatten() {
                if let Tree::ValDef { tpt, .. } = *self.ast.tree(vparam)
                    && matches!(self.ast.tree(self.unsplice(tpt)), Tree::ByNameTypeTree { .. })
                {
                    self.error(vparam, "implementation restriction: traits cannot have by name parameters");
                }
            }
        }
        let constr_tparams: Vec<NodeId> =
            implied_tparams.iter().map(|&t| self.meth_type_param(t, Keep::WitnessOnly)).collect();
        let is_case_class = mods.is(flags::CASE) && !mods.is(flags::MODULE);
        let implicit_first = original_vparamss.first().is_some_and(|ps| {
            ps.iter().any(|&p| {
                self.ast.mods_of(p).is_some_and(|m| self.ast.modifiers(m).is_one_of(flags::GIVEN_OR_IMPLICIT))
            })
        });
        let constr_vparamss = if original_vparamss.is_empty() || is_case_class && implicit_first {
            if is_case_class {
                let msg = if original_vparamss.is_empty() {
                    "A case class must have at least one parameter list"
                } else {
                    "A case class must have at least one leading non-implicit parameter list"
                };
                let span = self.name_span(cdef, name);
                self.diags.error_at(span.start(), span.end(), span.point(), msg);
            }
            vec![Vec::new()]
        } else {
            original_vparamss
                .iter()
                .map(|ps| ps.iter().map(|&p| self.meth_value_param(p, Keep::All, true)).collect())
                .collect()
        };
        let derived_tparams: Vec<NodeId> = constr_tparams
            .iter()
            .zip(&implied_tparams)
            .map(|(&t, &implied)| {
                let d = self.derived_type_param(t);
                let annots =
                    self.ast.mods_of(implied).map(|m| self.ast.modifiers(m).annotations).unwrap_or_default();
                self.with_annotations(d, annots)
            })
            .collect();
        let derived_vparamss: Vec<Vec<NodeId>> = constr_vparamss
            .iter()
            .map(|ps| {
                ps.iter()
                    .map(|&p| {
                        let d = self.derived_term_param(p);
                        let none = self.list(&[]);
                        self.with_annotations(d, none)
                    })
                    .collect()
            })
            .collect();
        let joined = self.join_params(&constr_tparams, &constr_vparamss);
        let constr = self.with_clauses(constr1, &joined);
        let constr_mods = self.ast.modifiers(self.ast.mods_of(constr1).expect("modifiers")).clone();
        let constr_mods = self.add_impl_use(constr_mods, &uses, true);
        let constr = self.ast.with_mods(constr, constr_mods);
        if !enum_tparams.is_empty() {
            default_getters =
                default_getters.iter().map(|&g| self.with_enum_tparams(g, &enum_tparams)).collect();
        }
        let mut body = Vec::new();
        for stat in self.ast.items(stats).to_vec() {
            body.push(match *self.ast.tree(stat) {
                Tree::DefDef { name, .. } if self.text(name) == "<init>" => {
                    let (ctor, getters) = self.expand_constructor(stat, &constr_tparams, constr1);
                    if !getters.is_empty() {
                        default_getters = getters;
                    }
                    ctor
                }
                _ => stat,
            });
        }
        let is_enum = mods.is(flags::ENUM) && !mods.is(flags::CASE) && !mods.is(flags::MODULE);
        let (body, enum_cases) = if is_enum { self.enum_body(cdef, name, &body) } else { (body, Vec::new()) };
        let class_tycon =
            self.ast.node(Tree::DerivedTypeTree { kind: DerivedKind::TypeRef, watched: Some(name) });
        let class_type_ref = self.applied_ref(cdef, &constr_tparams, class_tycon);
        ClassParts {
            cdef,
            name,
            mods,
            template,
            parents,
            derived,
            uses,
            constr1,
            constr,
            default_getters,
            original_tparams,
            implied_tparams,
            original_vparamss,
            constr_tparams,
            constr_vparamss,
            derived_tparams,
            derived_vparamss,
            body,
            enum_cases,
            class_tycon,
            class_type_ref,
            creator: None,
        }
    }
}
