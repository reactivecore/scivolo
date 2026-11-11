// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Whether two members match (dotty's `matchesLoosely`), and nullary methods overriding
//! parameterless members (`matchNullaryLoosely`).

use super::Typer;
use crate::symbols::Completion;
use syntax::ast::flags as f;
use tasty::nme;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether members `a` and `b` of `site` have the same run-time name and match (dotty's
    /// `matches`).
    pub fn same_member(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        self.u.symbol(a).run_name() == self.u.symbol(b).run_name() && self.matches_loosely(site, a, b)
    }

    /// Whether member `s` is concrete: a definition of the unit with a right-hand side, as
    /// `def f = 1`; a library member with a body, of `Any` or `Object`, or concrete in its class
    /// file.
    pub fn is_concrete(&self, s: SymbolId) -> bool {
        match self.syms.flags.get(&s) {
            Some(flags) => !flags.is(f::DEFERRED),
            None => {
                let builtins = self.u.defn();
                let builtin = |o| o == builtins.any || o == builtins.object;
                self.u.body(s).is_some()
                    || self.u.symbol(s).owner.is_some_and(builtin)
                    || self.u.is_concrete_classfile_member(s)
            }
        }
    }

    /// Whether members `a` and `b` of `site` match: as methods, or a method without parameters
    /// and a member that is not a method nor a Java field.
    pub fn matches_loosely(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        let (ka, kb) = (self.u.symbol(a).kind, self.u.symbol(b).kind);
        match (ka, kb) {
            (Kind::Def, Kind::Def) => self.u.matches(site, a, b) || self.is_nullary(a) && self.is_nullary(b),
            (Kind::Def, _) => self.is_nullary(a) && !self.is_java_member(b),
            (_, Kind::Def) => self.is_nullary(b) && !self.is_java_member(a),
            _ => true,
        }
    }

    /// Whether method `s` has neither type parameters nor value parameters, its clauses empty.
    pub fn is_nullary(&mut self, s: SymbolId) -> bool {
        self.empty_clauses(s).is_some()
    }

    /// Whether method definition `tree` has no parameters, its clauses empty.
    pub fn is_nullary_def(&self, tree: syntax::ast::NodeId) -> bool {
        self.def_clauses(tree).iter().all(Vec::is_empty)
    }

    /// The number of clauses of member `s` if all are empty, a value's none.
    fn empty_clauses(&mut self, s: SymbolId) -> Option<usize> {
        if let Some(&(Completion::Pending { tree, .. } | Completion::Completing(tree))) =
            self.syms.completions.get(&s)
        {
            return self.is_nullary_def(tree).then(|| self.def_clauses(tree).len());
        }
        let (mut info, mut n) = (self.u.info(s), 0);
        loop {
            match *self.u.ty(info) {
                Type::Method { params, result, .. } if self.u.params_of(params).is_empty() => {
                    (info, n) = (result, n + 1)
                }
                Type::Method { .. } | Type::Poly { .. } => return None,
                _ => return Some(n),
            }
        }
    }

    /// Whether member `d` of class `cls` may not override member `m`, which it matches: one has
    /// more empty clauses than the other and neither matches nullary members loosely.
    pub fn overrides_other_nullary(&mut self, cls: SymbolId, d: SymbolId, m: SymbolId) -> bool {
        let (Some(nd), Some(nm)) = (self.empty_clauses(d), self.empty_clauses(m)) else { return false };
        nd != nm && !self.match_nullary_loosely(d, cls) && !self.match_nullary_loosely(m, cls)
    }

    /// dotty's `matchNullaryLoosely` for member `s` of `owner`: `s` or a member it overrides is a
    /// member of `Any`, of a Java class (the Scala library's whatever their source) or of the
    /// Scala 2 library, or `Object.clone`.
    fn match_nullary_loosely(&mut self, s: SymbolId, owner: SymbolId) -> bool {
        let object = self.u.defn().object;
        let test = |t: &mut Self, x: SymbolId| {
            let owner = t.u.symbol(x).owner;
            let clone = owner == Some(object) && t.u.symbol(x).name == nme::CLONE;
            let name = owner.map(|o| t.u.full_name(o)).unwrap_or_default();
            clone || super::java_classes::is_scala_library_java(&name) || t.is_loose_member(x)
        };
        test(self, s) || self.overridden_members(s, owner).into_iter().any(|m| test(self, m))
    }
}
