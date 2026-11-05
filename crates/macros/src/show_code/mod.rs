// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Trees as `Printer.TreeCode` and `Expr.show` show them (dotty's `SourceCode.printTree` with
//! full names), the holes of quotes filled.

mod blocks;
mod cases;
mod classes;
mod control;
mod def_structure;
mod defs;
mod highlight;
mod imports;
mod literals;
mod local_names;
mod modifiers;
mod parents;
pub(crate) mod printed;
mod references;
mod structure;
mod type_string;
mod type_structure;
mod types;

use crate::host::Host;
use printed::Printed;
use std::collections::HashMap;
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree as T};

type Shown = Result<(), &'static str>;

const REFUSED: &str = "Printer.TreeCode of a tree the frontend does not print";

impl Host {
    /// Tree `t` as `Printer.TreeCode` shows it, `Printer.TreeAnsiCode` if `ansi`, or
    /// `Printer.TreeStructure` if `structure`.
    pub fn show_code(&mut self, t: TNodeId, structure: bool, ansi: bool) -> Result<Printed, &'static str> {
        let fills = self.hole_fills().into_iter().filter_map(|(h, f)| Some((h, f?))).collect();
        let spliced = self.spliced_types();
        let mut p = Code {
            h: self,
            out: Printed::default(),
            ansi,
            indent: 0,
            names: HashMap::new(),
            index: HashMap::new(),
            fills,
            spliced,
        };
        match structure {
            true => p.structure(t)?,
            false => p.tree(t)?,
        }
        Ok(p.out)
    }
}

struct Code<'h> {
    h: &'h mut Host,
    out: Printed,
    ansi: bool,
    indent: usize,
    /// The names local symbols print as, unique within the tree.
    names: HashMap<SymbolId, String>,
    index: HashMap<String, u32>,
    /// The trees filling the quotes' hole placeholders.
    fills: HashMap<TNodeId, TNodeId>,
    /// The spliced type symbols of the quotes with the types they stand for.
    spliced: Vec<(SymbolId, TypeId)>,
}

impl Code<'_> {
    /// `t`, or the tree filling it if it is a hole placeholder.
    fn r(&self, mut t: TNodeId) -> TNodeId {
        while let Some(&f) = self.fills.get(&t) {
            t = f;
        }
        t
    }

    fn tree_of(&self, t: TNodeId) -> T {
        self.h.tpd.tree(self.r(t)).clone()
    }

    fn items(&self, l: typer::tpd::List) -> Vec<TNodeId> {
        self.h.tpd.items(l).iter().map(|&t| self.r(t)).collect()
    }

    fn name(&self, n: syntax::ast::TName) -> String {
        self.h.names.text(n.name).to_owned()
    }

    fn push(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn line_break(&mut self) {
        self.out.push('\n');
        self.out.push_str(&"  ".repeat(self.indent));
    }

    /// `body` in braces on its own indented lines (dotty's `inBlock`).
    fn in_block(&mut self, body: impl FnOnce(&mut Self) -> Shown) -> Shown {
        self.push(" {");
        self.indent += 1;
        self.line_break();
        body(self)?;
        self.indent -= 1;
        self.line_break();
        self.push("}");
        Ok(())
    }

    fn trees(&mut self, trees: &[TNodeId], sep: &str) -> Shown {
        for (i, &t) in trees.iter().enumerate() {
            if i > 0 {
                self.push(sep);
            }
            self.tree(t)?;
        }
        Ok(())
    }

    fn tree(&mut self, t: TNodeId) -> Shown {
        let t = self.r(t);
        if self.is_type_tree(t) {
            return self.type_tree(t);
        }
        match self.tree_of(t) {
            T::Ident { .. } | T::Select { .. } | T::This { .. } | T::Super { .. } => self.reference(t)?,
            T::Literal { value } => self.constant(&value)?,
            T::ClassLiteral { tpe } => {
                self.push("classOf[");
                self.ty(tpe)?;
                self.push("]");
            }
            T::New { .. } => {
                self.push("new ");
                self.ty(self.h.tpd.tpe(t))?;
            }
            T::NamedArg { name, arg } => {
                self.push(&format!("{} = ", self.name(name)));
                self.tree(arg)?;
            }
            T::Apply { fun, args } => self.apply(fun, &self.items(args))?,
            T::TypeApply { fun, args } => {
                self.qual_tree(fun)?;
                let new_applied = matches!(self.tree_of(fun), T::Select { qualifier, .. }
                    if matches!(self.tree_of(qualifier), T::New { tpt } if matches!(self.tree_of(tpt), T::AppliedTypeTree { .. })));
                if !new_applied {
                    self.push("[");
                    self.trees(&self.items(args), ", ")?;
                    self.push("]");
                }
            }
            T::Typed { expr, tpt } => self.typed(expr, tpt)?,
            T::Assign { lhs, rhs } => {
                self.tree(lhs)?;
                self.push(" = ");
                self.tree(rhs)?;
            }
            T::Block { .. } if self.lambda(t).is_some() => self.print_lambda(t)?,
            T::Block { stats, expr } => {
                let module = |c: &Self, s: TNodeId| {
                    matches!(c.h.tpd.tree(s), T::ValDef { .. })
                        && c.h.tpd.sym(s).is_some_and(|v| c.h.u.symbol(v).flags.has(tasty::tags::OBJECT))
                };
                let stats: Vec<_> = self.items(stats).into_iter().filter(|&s| !module(self, s)).collect();
                self.flat_block(&stats, expr)?;
            }
            T::Inlined { bindings, expansion, .. } => self.flat_block(&self.items(bindings), expansion)?,
            T::If { .. } | T::Match { .. } | T::Try { .. } | T::Return { .. } | T::WhileDo { .. } => {
                self.control(t)?
            }
            T::SeqLiteral { elems, .. } => self.trees(&self.items(elems), ", ")?,
            T::Closure { meth, .. } => self.tree(meth)?,
            T::TypeDef { .. } if self.h.tpd.class_template(t).is_some() => self.class_def(t)?,
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } => self.definition(t)?,
            T::CaseDef { .. } => self.case_def(t)?,
            T::Import { .. } => self.import(t)?,
            T::Bind { .. } | T::UnApply { .. } | T::Alternative { .. } => self.pattern(t)?,
            _ => return Err(REFUSED),
        }
        Ok(())
    }

    /// Type `t` as `printType` shows it, the spliced types substituted.
    fn ty(&mut self, t: TypeId) -> Shown {
        let spliced = std::mem::take(&mut self.spliced);
        let t = self.h.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => spliced.iter().find(|&&(x, _)| x == s).map(|&(_, t)| t),
            _ => None,
        });
        self.spliced = spliced;
        let shown = self.h.u.show_source(t, true, self.ansi)?;
        self.push(&shown);
        Ok(())
    }
}
