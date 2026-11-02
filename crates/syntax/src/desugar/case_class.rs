// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! The synthetic methods of case classes and their companions: `copy`, `_N`, `apply`,
//! `unapply` and `toString` (from dotty's `classDef`).

use super::Desugar;
use super::class_parts::ClassParts;
use super::class_syms::CaseKind;
use crate::ast::constant::Constant;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::ModsId;
use crate::ast::{List, NodeId, TName, Tree};
use crate::names::to_utf16;

impl Desugar<'_> {
    /// `copy`, an enum case's `ordinal` and the `_N` selectors of case class `c`; and the lookup
    /// methods of the enum if `c` is its last case.
    pub fn case_class_meths(&mut self, c: &mut ClassParts) -> (Vec<NodeId>, Vec<NodeId>) {
        let (enum_meths, scaffolding) = if c.mods.flags.is_all_of(flags::ENUM_CASE) {
            let lookups = self.defines_enum_lookups(c.cdef);
            let (ordinal, scaffolding) = self.next_ordinal(c.name, CaseKind::Class, lookups);
            let int = self.sym_ref("scala.Int", true);
            let lit = self.literal(Constant::Int(ordinal as i32));
            let mods = self.mods(flags::SYNTHETIC);
            let name = self.term("ordinal");
            (vec![self.method(name, &[], Some(int), lit, mods)], scaffolding)
        } else {
            (Vec::new(), Vec::new())
        };
        let mut meths = Vec::new();
        let repeated = c.constr_vparamss.iter().flatten().any(|&p| match *self.ast.tree(p) {
            Tree::ValDef { tpt, .. } => self.is_repeated(tpt),
            _ => false,
        });
        if !c.is(flags::ABSTRACT) && !repeated {
            let first: Vec<NodeId> = c.derived_vparamss[0]
                .iter()
                .map(|&p| {
                    let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else { return p };
                    let rhs = Some(self.ref_of_def(p));
                    self.ast.derived(p, Tree::ValDef { name, tpt, rhs, mods })
                })
                .collect();
            let mut clauses = vec![first];
            for ps in &c.derived_vparamss[1..] {
                clauses.push(
                    ps.iter()
                        .map(|&p| {
                            let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else { return p };
                            let d = self.ast.derived(p, Tree::ValDef { name, tpt, rhs: None, mods });
                            if d == p { self.ast.copy_node(p) } else { d }
                        })
                        .collect(),
                );
            }
            let joined = self.join_params(&c.derived_tparams, &clauses);
            let mods = self.synthetic_access_mods(c);
            let creator = self.creator_expr(c);
            let name = self.term("copy");
            meths.push(self.method(name, &joined, None, creator, mods));
        }
        meths.extend(enum_meths);
        if self.cfg.source_version.is_scala2() {
            return (meths, scaffolding);
        }
        let names_in_body: Vec<TName> = c
            .body
            .iter()
            .filter_map(|&s| match *self.ast.tree(s) {
                Tree::ValDef { name, .. } => Some(name),
                Tree::DefDef { name, paramss, .. } if paramss.is_empty() => Some(name),
                _ => None,
            })
            .filter(|&n| is_selector_name(self.text(n)))
            .collect();
        for (i, &param) in c.derived_vparamss[0].iter().enumerate() {
            let sel = self.term(&format!("_{}", i + 1));
            let Tree::ValDef { name, tpt, .. } = *self.ast.tree(param) else { continue };
            if sel == name || names_in_body.contains(&sel) {
                continue;
            }
            let this = self.ast.node(Tree::This { qual: None });
            let rhs = self.select(this, name);
            let mods = self.mods(flags::SYNTHETIC);
            meths.push(self.method(sel, &[], Some(tpt), rhs, mods));
        }
        (meths, scaffolding)
    }

    /// `apply`, `unapply` and `toString` of the companion of case class `c`.
    pub fn case_companion_meths(&mut self, c: &mut ClassParts) -> Vec<NodeId> {
        let mut meths = Vec::new();
        if !c.is(flags::ABSTRACT) {
            let mods = self.synthetic_access_mods(c);
            let paramss: Vec<Vec<NodeId>> = c
                .derived_vparamss
                .iter()
                .zip(&c.constr_vparamss)
                .map(|(aps, cps)| {
                    aps.iter()
                        .zip(cps)
                        .map(|(&ap, &cp)| {
                            let default = self.flags_of_def(cp).and(flags::HAS_DEFAULT);
                            let Some(m) = self.ast.mods_of(ap) else { return ap };
                            let m = self.ast.modifiers(m).clone().or(default);
                            self.ast.with_mods(ap, m)
                        })
                        .collect()
                })
                .collect();
            let tparams = self.fresh_type_params(&c.derived_tparams);
            let joined = self.join_params(&tparams, &paramss);
            let creator = self.creator_expr(c);
            let name = self.term("apply");
            meths.push(self.method(name, &joined, None, creator, mods));
        }
        let repeated = c.constr_vparamss[0].iter().any(|&p| match *self.ast.tree(p) {
            Tree::ValDef { tpt, .. } => self.is_repeated(tpt),
            _ => false,
        });
        let name = self.term(if repeated { "unapplySeq" } else { "unapply" });
        let param_name = self.term("x$1");
        let param_mods = self.mods(flags::SYNTHETIC_TERM_PARAM);
        let param = self.ast.node(Tree::ValDef {
            name: param_name,
            tpt: c.class_type_ref,
            rhs: None,
            mods: param_mods,
        });
        let (rhs, res) = if c.arity() == 0 {
            (self.literal(Constant::Boolean(true)), Some(self.literal(Constant::Boolean(true))))
        } else if self.cfg.source_version.is_scala2() {
            (self.scala2_unapply_rhs(c, param_name), None)
        } else {
            (self.ident(param_name), None)
        };
        let tparams = self.fresh_type_params(&c.derived_tparams);
        let joined = self.join_params(&tparams, &[vec![param]]);
        let mods = self.mods(flags::SYNTHETIC);
        meths.push(self.method(name, &joined, res, rhs, mods));
        let text: Vec<u16> = to_utf16(self.text(c.name));
        let lit = self.literal(Constant::String(text.into()));
        let mods = self.mods(flags::OVERRIDE.or(flags::SYNTHETIC));
        let name = self.term("toString");
        meths.push(self.method(name, &[], None, lit, mods));
        meths
    }

    /// `Option(x.p)`, or `Option(scala.TupleN(x.p1, ..., x.pN))`, of the parameters of case class
    /// `c` and `unapply`'s parameter `x` (dotty's `scala2LibCompatUnapplyRhs`).
    fn scala2_unapply_rhs(&mut self, c: &ClassParts, x: TName) -> NodeId {
        let names: Vec<TName> = c.derived_vparamss[0]
            .iter()
            .filter_map(|&p| match *self.ast.tree(p) {
                Tree::ValDef { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        let members: Vec<NodeId> = names
            .into_iter()
            .map(|name| {
                let x = self.ident(x);
                self.select(x, name)
            })
            .collect();
        let value = match members[..] {
            [one] => one,
            _ => {
                let (scala, tuple) = (self.term("scala"), self.term(&format!("Tuple{}", members.len())));
                let scala = self.ident(scala);
                let tuple = self.select(scala, tuple);
                self.apply(tuple, &members)
            }
        };
        let option = self.term("Option");
        let option = self.scala_dot(option);
        self.apply(option, &[value])
    }

    /// `Synthetic` with the access of the primary constructor.
    fn synthetic_access_mods(&mut self, c: &ClassParts) -> ModsId {
        let constr = self.ast.modifiers(self.ast.mods_of(c.constr1).expect("modifiers")).clone();
        let access = if self.cfg.source_version.migrate_to_3() { flags::EMPTY } else { flags::ACCESS_FLAGS };
        let mut flags = flags::SYNTHETIC.or(constr.flags.and(access));
        if self.cfg.source_version.is_scala2() {
            flags = flags.and_not(flags::PRIVATE);
        }
        self.ast.mods(Modifiers { flags, private_within: constr.private_within, ..Modifiers::default() })
    }

    fn flags_of_def(&self, def: NodeId) -> FlagSet {
        self.ast.mods_of(def).map_or(flags::EMPTY, |m| self.ast.modifiers(m).flags)
    }

    /// `def name(paramss): tpt = rhs`, an inferred type without `tpt`.
    pub fn method(
        &mut self,
        name: TName,
        paramss: &[List],
        tpt: Option<NodeId>,
        rhs: NodeId,
        mods: ModsId,
    ) -> NodeId {
        let paramss = self.ast.clauses(paramss);
        let tpt = tpt.unwrap_or_else(|| self.type_tree());
        self.ast.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs), mods })
    }
}

/// Whether `name` is `_N` (dotty's `isSelectorName`).
fn is_selector_name(name: &str) -> bool {
    name.strip_prefix('_').is_some_and(|d| d.chars().all(|c| c.is_ascii_digit()))
}
