//! Constructors of the trees desugaring makes (dotty's `untpd` helpers).

use super::Desugar;
use crate::ast::constant::Constant;
use crate::ast::flags::FlagSet;
use crate::ast::modifiers::Modifiers;
use crate::ast::tree::ModsId;
use crate::ast::{Attrs, List, NodeId, TName, Tree};

impl Desugar<'_> {
    pub fn list(&mut self, items: &[NodeId]) -> List {
        self.ast.list(items)
    }

    pub fn ident(&mut self, name: TName) -> NodeId {
        self.ast.node(Tree::Ident { name })
    }

    pub fn select(&mut self, qualifier: NodeId, name: TName) -> NodeId {
        self.ast.node(Tree::Select { qualifier, name })
    }

    pub fn apply(&mut self, fun: NodeId, args: &[NodeId]) -> NodeId {
        let args = self.list(args);
        self.ast.node(Tree::Apply { fun, args })
    }

    /// `_root_.scala.name`.
    pub fn scala_dot(&mut self, name: TName) -> NodeId {
        let (root, scala) = (self.term("_root_"), self.term("scala"));
        let root = self.ident(root);
        let scala = self.select(root, scala);
        self.select(scala, name)
    }

    pub fn literal(&mut self, value: Constant) -> NodeId {
        self.ast.node(Tree::Literal { value })
    }

    pub fn type_tree(&mut self) -> NodeId {
        self.ast.node(Tree::TypeTree)
    }

    /// A reference to the compiler-known symbol `full_name` (dotty's `ref(defn.X)`).
    pub fn sym_ref(&mut self, full_name: &str, is_type: bool) -> NodeId {
        let sym = TName { name: self.names.intern(full_name), is_type };
        self.ast.node(Tree::TypedSplice { sym })
    }

    pub fn synthetic_unit(&mut self) -> NodeId {
        let unit = self.literal(Constant::Unit);
        self.ast.add_attrs(unit, Attrs::SYNTHETIC_UNIT);
        unit
    }

    /// `new tpt` as an annotation without arguments (dotty's `New(tpt)`).
    pub fn new_annot(&mut self, class: &str) -> NodeId {
        let tpt = self.sym_ref(class, true);
        self.ast.node(Tree::New { tpt })
    }

    pub fn mods(&mut self, flags: FlagSet) -> ModsId {
        self.ast.mods(Modifiers::with_flags(flags))
    }

    /// `trees` as one tree: the tree itself or a `Thicket` (dotty's `flatTree`).
    pub fn flat_tree(&mut self, trees: &[NodeId]) -> NodeId {
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
    pub fn flatten(&self, trees: &[NodeId]) -> Vec<NodeId> {
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
}
