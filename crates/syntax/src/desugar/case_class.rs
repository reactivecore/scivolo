//! The synthetic methods of case classes and their companions: `copy`, `_N`, `apply`,
//! `unapply` and `toString` (from dotty's `classDef`).

use super::Desugar;
use super::class_parts::ClassParts;
use crate::ast::constant::Constant;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, TName, Tree};

impl Desugar<'_> {
    /// `copy` and the `_N` selectors of case class `c`.
    pub fn case_class_meths(&mut self, c: &mut ClassParts) -> Vec<NodeId> {
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
                            self.ast.derived(p, Tree::ValDef { name, tpt, rhs: None, mods })
                        })
                        .collect(),
                );
            }
            let joined = self.join_params(&c.derived_tparams.clone(), &clauses);
            let mods = self.synthetic_access_mods(c);
            let creator = self.creator_expr(c);
            let name = self.term("copy");
            meths.push(self.method(name, &joined, None, creator, mods));
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
        for (i, &param) in c.derived_vparamss[0].clone().iter().enumerate() {
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
        meths
    }

    /// `apply`, `unapply` and `toString` of the companion of case class `c`.
    pub fn case_companion_meths(&mut self, c: &mut ClassParts) -> Vec<NodeId> {
        let mut meths = Vec::new();
        if !c.is(flags::ABSTRACT) {
            let mods = self.synthetic_access_mods(c);
            let paramss: Vec<Vec<NodeId>> = c
                .derived_vparamss
                .clone()
                .iter()
                .zip(c.constr_vparamss.clone())
                .map(|(aps, cps)| {
                    aps.iter()
                        .zip(cps)
                        .map(|(&ap, cp)| {
                            let default = self.flags_of_def(cp).and(flags::HAS_DEFAULT);
                            let Some(m) = self.ast.mods_of(ap) else { return ap };
                            let m = self.ast.modifiers(m).clone().or(default);
                            self.ast.with_mods(ap, m)
                        })
                        .collect()
                })
                .collect();
            let joined = self.join_params(&c.derived_tparams.clone(), &paramss);
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
        } else {
            (self.ident(param_name), None)
        };
        let joined = self.join_params(&c.derived_tparams.clone(), &[vec![param]]);
        let mods = self.mods(flags::SYNTHETIC);
        meths.push(self.method(name, &joined, res, rhs, mods));
        let text: Vec<u16> = crate::names::to_utf16(self.text(c.name));
        let lit = self.literal(Constant::String(text.into()));
        let mods = self.mods(flags::OVERRIDE.or(flags::SYNTHETIC));
        let name = self.term("toString");
        meths.push(self.method(name, &[], None, lit, mods));
        meths
    }

    /// `Synthetic` with the access of the primary constructor.
    fn synthetic_access_mods(&mut self, c: &ClassParts) -> crate::ast::tree::ModsId {
        let constr = self.ast.modifiers(self.ast.mods_of(c.constr1).expect("modifiers")).clone();
        let access = if self.cfg.source_version.migrate_to_3() { flags::EMPTY } else { flags::ACCESS_FLAGS };
        let flags = flags::SYNTHETIC.or(constr.flags.and(access));
        self.ast.mods(Modifiers { flags, private_within: constr.private_within, ..Modifiers::default() })
    }

    fn flags_of_def(&self, def: NodeId) -> FlagSet {
        self.ast.mods_of(def).map_or(flags::EMPTY, |m| self.ast.modifiers(m).flags)
    }

    /// `def name(paramss): tpt = rhs`, an inferred type without `tpt`.
    pub fn method(
        &mut self,
        name: TName,
        paramss: &[crate::ast::List],
        tpt: Option<NodeId>,
        rhs: NodeId,
        mods: crate::ast::tree::ModsId,
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
