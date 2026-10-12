//! The desugaring dotty's Namer and Typer apply to untyped trees before typing, as one pass over
//! a compilation unit (a port of `ast/Desugar.scala`).

mod build;
mod case_class;
mod case_lambda;
mod class_companions;
mod class_def;
mod class_params;
mod class_parts;
mod class_syms;
mod companions;
mod constructors;
mod context_bounds;
mod creator;
mod defaults;
mod defs;
mod enum_cases;
mod enum_lookups;
mod enums;
mod evidence;
mod exprs;
mod ext_methods;
mod for_expr;
mod generators;
mod given_name;
mod map;
mod members;
mod module_def;
pub mod package_def;
mod pat_def;
mod pat_split;
mod stats;
mod uses;
mod val_def;
mod variables;
mod walk;

use crate::ast::{Ast, NodeId, TName, Tree};
use crate::config::Config;
use crate::diag::Diagnostics;
use crate::names::{FreshNames, Names};
use crate::source::SourceFile;
use class_syms::{ClassSym, Scope};
use rustc_hash::FxHashMap;

pub use given_name::given_type_name;

/// What the trees being desugared are (dotty's `Mode`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Expr,
    Type,
    Pattern,
}

/// The kind of definition that owns the trees being desugared.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner {
    Package,
    Class { is_trait: bool, is_package_object: bool },
    Term { is_constructor: bool },
}

impl Owner {
    /// Whether the owner is a class or a package (dotty's `isClass`).
    pub fn is_class(self) -> bool {
        !matches!(self, Owner::Term { .. })
    }
}

pub struct Desugar<'a> {
    ast: &'a mut Ast,
    names: &'a mut Names,
    diags: &'a mut Diagnostics,
    fresh: &'a mut FreshNames,
    source: &'a SourceFile,
    /// The settings, with the source version the unit's language imports chose.
    cfg: Config,
    mode: Mode,
    owner: Owner,
    classes: Vec<ClassSym>,
    /// The class of each class definition met.
    class_ids: FxHashMap<NodeId, usize>,
    scope: Scope,
}

impl<'a> Desugar<'a> {
    pub fn new(
        ast: &'a mut Ast,
        names: &'a mut Names,
        diags: &'a mut Diagnostics,
        fresh: &'a mut FreshNames,
        source: &'a SourceFile,
        cfg: Config,
    ) -> Self {
        Desugar {
            ast,
            names,
            diags,
            fresh,
            source,
            cfg,
            mode: Mode::Expr,
            owner: Owner::Package,
            classes: Vec::new(),
            class_ids: FxHashMap::default(),
            scope: Scope::default(),
        }
    }

    /// The desugared compilation unit `root`.
    pub fn unit(&mut self, root: NodeId) -> NodeId {
        let stats = self.ast.list(&[root]);
        let stats = self.transform_stats(stats);
        match *self.ast.items(stats) {
            [t] => t,
            _ => self.ast.node(Tree::Thicket { trees: stats }),
        }
    }

    fn in_mode<T>(&mut self, mode: Mode, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = std::mem::replace(&mut self.mode, mode);
        let result = f(self);
        self.mode = saved;
        result
    }

    fn in_owner<T>(&mut self, owner: Owner, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = std::mem::replace(&mut self.owner, owner);
        let result = f(self);
        self.owner = saved;
        result
    }

    fn term(&mut self, text: &str) -> TName {
        TName { name: self.names.intern(text), is_type: false }
    }

    fn type_name(&mut self, text: &str) -> TName {
        TName { name: self.names.intern(text), is_type: true }
    }

    fn text(&self, name: TName) -> &str {
        self.names.text(name.name)
    }

    fn error(&mut self, at: NodeId, message: &str) {
        let span = self.ast.span(at);
        self.diags.error_at(span.start(), span.end(), span.point(), message);
    }
}
