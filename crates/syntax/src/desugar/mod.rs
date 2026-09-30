//! The desugaring dotty's Namer and Typer apply to untyped trees before typing, as one pass over
//! a compilation unit (a port of `ast/Desugar.scala`).

mod case_lambda;
mod defs;
mod exprs;
mod for_expr;
mod generators;
mod given_name;
mod map;
mod pat_def;
mod pat_split;
mod stats;
mod val_def;
mod variables;
mod walk;

use crate::ast::constant::Constant;
use crate::ast::flags::FlagSet;
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::ModsId;
use crate::ast::{Ast, Attrs, List, NodeId, TName, Tree};
use crate::config::Config;
use crate::diag::Diagnostics;
use crate::names::{FreshNames, Names};
use crate::source::SourceFile;

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
    Term,
}

impl Owner {
    /// Whether the owner is a class or a package (dotty's `isClass`).
    pub fn is_class(self) -> bool {
        self != Owner::Term
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
        Desugar { ast, names, diags, fresh, source, cfg, mode: Mode::Expr, owner: Owner::Package }
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

    fn list(&mut self, items: &[NodeId]) -> List {
        self.ast.list(items)
    }

    fn ident(&mut self, name: TName) -> NodeId {
        self.ast.node(Tree::Ident { name })
    }

    fn select(&mut self, qualifier: NodeId, name: TName) -> NodeId {
        self.ast.node(Tree::Select { qualifier, name })
    }

    fn apply(&mut self, fun: NodeId, args: &[NodeId]) -> NodeId {
        let args = self.list(args);
        self.ast.node(Tree::Apply { fun, args })
    }

    fn literal(&mut self, value: Constant) -> NodeId {
        self.ast.node(Tree::Literal { value })
    }

    fn type_tree(&mut self) -> NodeId {
        self.ast.node(Tree::TypeTree)
    }

    /// A reference to the compiler-known symbol `full_name` (dotty's `ref(defn.X)`).
    fn sym_ref(&mut self, full_name: &str, is_type: bool) -> NodeId {
        let sym = TName { name: self.names.intern(full_name), is_type };
        self.ast.node(Tree::TypedSplice { sym })
    }

    fn synthetic_unit(&mut self) -> NodeId {
        let unit = self.literal(Constant::Unit);
        self.ast.add_attrs(unit, Attrs::SYNTHETIC_UNIT);
        unit
    }

    /// `new tpt` as an annotation without arguments (dotty's `New(tpt)`).
    fn new_annot(&mut self, class: &str) -> NodeId {
        let tpt = self.sym_ref(class, true);
        self.ast.node(Tree::New { tpt })
    }

    fn mods(&mut self, flags: FlagSet) -> ModsId {
        self.ast.mods(Modifiers::with_flags(flags))
    }

    /// `trees` as one tree: the tree itself or a `Thicket` (dotty's `flatTree`).
    fn flat_tree(&mut self, trees: &[NodeId]) -> NodeId {
        let flat = self.flatten(trees);
        match flat[..] {
            [t] => t,
            _ => {
                let trees = self.list(&flat);
                self.ast.node(Tree::Thicket { trees })
            }
        }
    }

    /// `trees` with thickets spliced in and empty trees dropped (dotty's `flatten`).
    fn flatten(&self, trees: &[NodeId]) -> Vec<NodeId> {
        let mut out = Vec::with_capacity(trees.len());
        for &t in trees {
            match self.ast.tree(t) {
                Tree::Thicket { trees } => out.extend(self.flatten(self.ast.items(*trees))),
                Tree::Empty => {}
                _ => out.push(t),
            }
        }
        out
    }

    fn error(&mut self, at: NodeId, message: &str) {
        let span = self.ast.span(at);
        self.diags.error_at(span.start(), span.end(), span.point(), message);
    }
}
