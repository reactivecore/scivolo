//! The scopes of a capture-checked file in which dotty's typer may give an inferred type capture
//! sets that ours drops: those that refer to a definition whose type has capture sets, or have a
//! type tree with them.

mod definitions;
mod pure_elements;
mod type_arg_calls;

use super::inferred_captures::overridden_infos;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use crate::typer::capture_types::{has_captures, is_retains_like};
use definitions::{Def, definition, is_class_type};
use names::nme;
use pure_elements::pure_elements;
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::{SymbolId, TypeId, Universe};
use type_arg_calls::type_arg_calls;

/// What the types of a scope may give its inferred types.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Risk {
    /// A capture set: a definition referred to, a type tree or an expected type has one.
    pub captures: bool,
    /// A capture set on a type other than a class's, which inference may put on any type.
    pub on_abstract: bool,
}

impl Risk {
    fn merge(&mut self, other: Risk) {
        self.captures |= other.captures;
        self.on_abstract |= other.on_abstract;
    }
}

/// The risk of each scope of a file, a definition but a closure's method or its parameter, and of
/// the file outside them; the definitions with inferred types in a scope with capture sets, whose
/// types dotty's typer may give capture sets.
pub struct CaptureRisks {
    scopes: FxHashMap<SymbolId, Risk>,
    outer: Risk,
    unknown: FxHashSet<SymbolId>,
    of_types: FxHashMap<TypeId, Risk>,
    pure_elements: FxHashSet<TNodeId>,
    type_arg_calls: FxHashMap<TNodeId, TNodeId>,
}

/// A step of the walk over a file's trees: a tree entered, a scope left.
enum Step {
    Enter(TNodeId),
    Exit { sym: SymbolId, class: bool, inferred: bool },
}

impl CaptureRisks {
    /// Determines the risks of the scopes of file `root`, until the definitions with inferred types
    /// in scopes with capture sets, which make the scopes referring to them have capture sets, are
    /// all found.
    pub fn new(u: &mut Universe, tpd: &Tpd, root: TNodeId) -> Self {
        let mut risks = CaptureRisks {
            scopes: FxHashMap::default(),
            outer: Risk::default(),
            unknown: FxHashSet::default(),
            of_types: FxHashMap::default(),
            pure_elements: pure_elements(u, tpd, root),
            type_arg_calls: type_arg_calls(tpd, root),
        };
        loop {
            let unknown = risks.walk(u, tpd, root);
            if unknown == risks.unknown {
                return risks;
            }
            risks.unknown = unknown;
        }
    }

    /// The risk of scope `sym`, `None` if `sym` is no scope.
    pub fn scope(&self, sym: SymbolId) -> Option<Risk> {
        self.scopes.get(&sym).copied()
    }

    /// The risk of the file outside its definitions.
    pub fn outer(&self) -> Risk {
        self.outer
    }

    /// Whether inferred type tree `tpt` is a repeated argument's element type a parameter fixes
    /// without capture sets (`pure_elements`).
    pub fn is_pure_element(&self, tpt: TNodeId) -> bool {
        self.pure_elements.contains(&tpt)
    }

    /// The call inferred type argument tree `tpt` is a type argument of (`type_arg_calls`).
    pub fn call_of(&self, tpt: TNodeId) -> Option<TNodeId> {
        self.type_arg_calls.get(&tpt).copied()
    }

    /// Whether dotty's typer may give definition `sym` an inferred type with capture sets.
    pub fn is_unknown(&self, sym: SymbolId) -> bool {
        self.unknown.contains(&sym)
    }

    /// Records the risk of each scope of `root`, a class's added to the enclosing scope's; the
    /// definitions with inferred types in scopes with capture sets.
    fn walk(&mut self, u: &mut Universe, tpd: &Tpd, root: TNodeId) -> FxHashSet<SymbolId> {
        let mut unknown = FxHashSet::default();
        let mut open: Vec<(Risk, Vec<SymbolId>)> = vec![(Risk::default(), Vec::new())];
        let mut stack = vec![Step::Enter(root)];
        while let Some(step) = stack.pop() {
            match step {
                Step::Exit { sym, class, inferred } => {
                    let (risk, pending) = open.pop().expect("an open scope");
                    self.scopes.insert(sym, risk);
                    if risk.captures {
                        unknown.extend(pending);
                        unknown.extend(inferred.then_some(sym));
                    }
                    if class {
                        open.last_mut().expect("an enclosing scope").0.merge(risk);
                    }
                }
                Step::Enter(n) => {
                    let own = self.own_risk(u, tpd, n);
                    let current = open.last_mut().expect("an enclosing scope");
                    match definition(u, tpd, n) {
                        Some((sym, Def::Scope { class, inferred })) => {
                            let mut risk = own;
                            if !class && overridden_infos(u, sym).into_iter().any(|t| has_captures(u, t)) {
                                risk.captures = true;
                            }
                            open.push((risk, Vec::new()));
                            stack.push(Step::Exit { sym, class, inferred });
                        }
                        Some((sym, Def::Nested { inferred })) => {
                            current.0.merge(own);
                            current.1.extend(inferred.then_some(sym));
                        }
                        None => current.0.merge(own),
                    }
                    stack.extend(tpd.children(n).into_iter().rev().map(Step::Enter));
                }
            }
        }
        self.outer = open.pop().expect("the file's scope").0;
        unknown
    }

    /// The capture sets tree `n` brings in: of its type, the type the typer recorded for it, and
    /// the info of the definition other than a closure's method it refers to, which its scope's
    /// trees give.
    fn own_risk(&mut self, u: &mut Universe, tpd: &Tpd, n: TNodeId) -> Risk {
        let mut risk = Risk::default();
        for t in [tpd.get(n).tpe, tpd.captured_type(n)].into_iter().flatten() {
            risk.merge(self.of_type(u, t));
        }
        let referenced = tpd.sym(n).filter(|&s| u.symbol(s).name != nme::ANON_FUN);
        if let (T::Ident { .. } | T::Select { .. }, Some(s)) = (tpd.tree(n), referenced) {
            if self.unknown.contains(&s) {
                return Risk { captures: true, on_abstract: true };
            }
            if u.symbol(s).kind.is_term() {
                let info = u.info(s);
                risk.merge(self.of_type(u, info));
            }
        }
        risk
    }

    fn of_type(&mut self, u: &mut Universe, t: TypeId) -> Risk {
        if let Some(&risk) = self.of_types.get(&t) {
            return risk;
        }
        let mut risk = Risk::default();
        u.map_type(t, &mut |u, part| {
            if let Some((inner, annot)) = u.annotated(part)
                && is_retains_like(u, annot)
            {
                risk.captures = true;
                risk.on_abstract |= !is_class_type(u, inner);
            }
            None
        });
        self.of_types.insert(t, risk);
        risk
    }
}
