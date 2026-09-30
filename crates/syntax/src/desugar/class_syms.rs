//! The classes the desugaring is inside: their full names, type parameters and companions, as
//! far as enums need them (the oracle's class symbols).

use super::{Desugar, Owner};
use crate::ast::flags::{self, FlagSet};
use crate::ast::{NodeId, TName, Tree};

/// The kinds of enum cases, in order (dotty's `CaseKind`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum CaseKind {
    Simple,
    Object,
    Class,
}

/// The enum cases seen so far in a class body (dotty's `EnumCaseCount`).
#[derive(Clone, Debug)]
pub struct EnumCount {
    pub ordinal: u32,
    pub min: CaseKind,
    pub max: CaseKind,
    /// The value cases with their ordinals, the last first.
    pub cases: Vec<(u32, TName)>,
}

pub struct ClassSym {
    pub full_name: String,
    /// The type parameters with their variance.
    pub tparams: Vec<(TName, FlagSet)>,
    pub is_module: bool,
    pub companion: Option<usize>,
    pub enum_count: Option<EnumCount>,
}

/// The class and the number of methods between it and the trees being desugared.
#[derive(Clone, Copy, Default)]
pub struct Scope {
    pub class: Option<usize>,
    pub terms: u32,
}

impl Desugar<'_> {
    /// The full name of a class named `name` defined in the current scope (dotty's `fullName`,
    /// with packages left out as the oracle's owners leave them out).
    pub fn child_full_name(&self, name: &str) -> String {
        let class = self.scope.class.map(|c| self.classes[c].full_name.as_str());
        match (class, self.scope.terms) {
            (None, 0) => name.to_string(),
            (Some(c), 0) => format!("{c}.{name}"),
            (c, t) => format!("{}.{}{name}", c.unwrap_or("<empty>"), "_$".repeat(t as usize)),
        }
    }

    /// The class of class definition `td`, made in the current scope on first use.
    pub fn class_sym(&mut self, td: NodeId) -> usize {
        if let Some(&c) = self.class_ids.get(&td) {
            return c;
        }
        let Tree::TypeDef { name, rhs, mods } = *self.ast.tree(td) else { unreachable!("a class") };
        let mut tparams = Vec::new();
        if let Tree::Template { constr, body, .. } = *self.ast.tree(rhs)
            && let Some(first) = self.clauses_of(constr).first().copied()
            && self.is_type_clause(first)
        {
            for &tp in self.ast.items(first) {
                let Some(tp_name) = self.def_name(tp) else { continue };
                let accessor = self.ast.items(body).iter().copied().find(|&s| {
                    matches!(*self.ast.tree(s), Tree::TypeDef { name, mods, .. }
                        if name == tp_name && self.ast.modifiers(mods).is(flags::PARAM))
                });
                let flags =
                    self.ast.modifiers(self.ast.mods_of(accessor.unwrap_or(tp)).expect("modifiers")).flags;
                tparams.push((tp_name, flags.and(flags::COVARIANT.or(flags::CONTRAVARIANT))));
            }
        }
        let full_name = self.child_full_name(self.text(name));
        let is_module = self.ast.modifiers(mods).is(flags::MODULE.to_type_flags());
        self.new_class(full_name, tparams, is_module, Some(td))
    }

    pub fn new_class(
        &mut self,
        full_name: String,
        tparams: Vec<(TName, FlagSet)>,
        is_module: bool,
        def: Option<NodeId>,
    ) -> usize {
        self.classes.push(ClassSym { full_name, tparams, is_module, companion: None, enum_count: None });
        let c = self.classes.len() - 1;
        if let Some(td) = def {
            self.class_ids.insert(td, c);
        }
        c
    }

    /// Make the classes of the class definitions in statements `flat`, each module class linked
    /// to the class of the same name.
    pub fn register_classes(&mut self, flat: &[NodeId]) {
        let classes: Vec<NodeId> = flat
            .iter()
            .copied()
            .filter(|&t| matches!(*self.ast.tree(t), Tree::TypeDef { rhs, .. } if matches!(self.ast.tree(rhs), Tree::Template { .. })))
            .collect();
        for &td in &classes {
            self.class_sym(td);
        }
        for &td in &classes {
            let c = self.class_sym(td);
            if !self.classes[c].is_module {
                continue;
            }
            let name = self.text(self.name_of_def(td)).to_string();
            let base = name.strip_suffix('$').unwrap_or(&name);
            if let Some(&other) = classes.iter().find(|&&o| self.text(self.name_of_def(o)) == base) {
                let o = self.class_sym(other);
                self.classes[c].companion = Some(o);
                self.classes[o].companion = Some(c);
            }
        }
    }

    /// Run `f` inside class `class` with owner `owner`.
    pub fn in_class<T>(&mut self, class: usize, owner: Owner, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = std::mem::replace(&mut self.scope, Scope { class: Some(class), terms: 0 });
        let result = self.in_owner(owner, f);
        self.scope = saved;
        result
    }

    /// Run `f` inside a method or value.
    pub fn in_term<T>(&mut self, is_constructor: bool, f: impl FnOnce(&mut Self) -> T) -> T {
        self.scope.terms += 1;
        let result = self.in_owner(Owner::Term { is_constructor }, f);
        self.scope.terms -= 1;
        result
    }

    /// The enum class of cases in the current scope: the class, or the companion of a module
    /// class (dotty's `enumClass`).
    pub fn enum_class(&self) -> Option<usize> {
        let c = self.scope.class.filter(|_| self.scope.terms == 0)?;
        if self.classes[c].is_module { self.classes[c].companion } else { Some(c) }
    }
}
