// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Trees as `Printer.TreeCode` and `Expr.show` show them (dotty's `SourceCode.printTree` with
//! full names), the holes of quotes filled.

mod blocks;
mod cases;
mod def_structure;
mod defs;
mod literals;
mod structure;
mod type_structure;
mod types;

use crate::host::Host;
use std::collections::HashMap;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree as T};

type Shown = Result<(), &'static str>;

const REFUSED: &str = "Printer.TreeCode of a tree the frontend does not print";

impl Host {
    /// Tree `t` as `Printer.TreeCode` shows it, or `Printer.TreeStructure` if `structure`.
    pub fn show_code(&mut self, t: TNodeId, structure: bool) -> Result<String, &'static str> {
        let fills = self.hole_fills().into_iter().filter_map(|(h, f)| Some((h, f?))).collect();
        let spliced = self.spliced_types();
        let mut p = Code {
            h: self,
            out: String::new(),
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
    out: String,
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
            T::Ident { name } if self.name(name) == "_" => self.push("_"),
            T::Ident { .. } => match self.h.tpd.sym(t).filter(|&s| self.is_local(s)) {
                Some(s) => {
                    let name = self.spliced_name(s);
                    self.push(&name);
                }
                None => self.ty(self.h.tpd.tpe(t))?,
            },
            T::Select { qualifier, name } => {
                self.qual_tree(qualifier)?;
                let name = self.name(name);
                if name != "<init>" && name != "package" {
                    self.push(".");
                    self.push(&name);
                }
            }
            T::Literal { value } => self.constant(&value)?,
            T::ClassLiteral { tpe } => {
                self.push("classOf[");
                self.ty(tpe)?;
                self.push("]");
            }
            T::This { qual } => {
                if let T::Ident { name } = self.tree_of(qual) {
                    let name = self.name(name);
                    if !name.is_empty() {
                        self.push(name.strip_suffix('$').unwrap_or(&name));
                        self.push(".");
                    }
                }
                self.push("this");
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
            T::Super { qual, mix } => {
                if let T::This { qual } = self.tree_of(qual)
                    && let T::Ident { name } = self.tree_of(qual)
                    && !self.name(name).is_empty()
                {
                    self.push(&format!("{}.", self.name(name)));
                }
                self.push("super");
                if let T::Ident { name } = self.tree_of(mix)
                    && !self.name(name).is_empty()
                {
                    self.push(&format!("[{}]", self.name(name)));
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
            T::If { cond, thenp, elsep, .. } => {
                self.push("if (");
                self.tree(cond)?;
                self.push(") ");
                self.tree(thenp)?;
                self.push(" else ");
                self.tree(elsep)?;
            }
            T::Match { selector, cases, .. } => {
                match selector {
                    Some(s) => {
                        self.qual_tree(s)?;
                        self.push(" match");
                    }
                    None => self.push("summonFrom "),
                }
                self.cases(&self.items(cases))?;
            }
            T::Try { expr, cases, finalizer } => {
                self.push("try ");
                self.tree(expr)?;
                let cases = self.items(cases);
                if !cases.is_empty() {
                    self.push(" catch");
                    self.cases(&cases)?;
                }
                if let Some(f) = finalizer {
                    self.push(" finally ");
                    self.tree(f)?;
                }
            }
            T::Return { expr, .. } => {
                self.push("return ");
                self.tree(expr)?;
            }
            T::WhileDo { cond, body } => {
                self.push("while (");
                self.tree(cond)?;
                self.push(") ");
                self.tree(body)?;
            }
            T::SeqLiteral { elems, .. } => self.trees(&self.items(elems), ", ")?,
            T::Closure { meth, .. } => self.tree(meth)?,
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } => self.definition(t)?,
            T::CaseDef { .. } => self.case_def(t)?,
            T::Bind { .. } | T::UnApply { .. } | T::Alternative { .. } => self.pattern(t)?,
            _ => return Err(REFUSED),
        }
        Ok(())
    }

    /// A qualifier, in parentheses if it is a control expression.
    fn qual_tree(&mut self, t: TNodeId) -> Shown {
        match self.tree_of(t) {
            T::If { .. } | T::Match { .. } | T::WhileDo { .. } | T::Try { .. } | T::Return { .. } => {
                self.push("(");
                self.tree(t)?;
                self.push(")");
                Ok(())
            }
            _ => self.tree(t),
        }
    }

    /// Whether `s` is owned by a term (dotty's `splicedName` applies).
    fn is_local(&self, s: SymbolId) -> bool {
        self.h
            .u
            .symbol(s)
            .owner
            .is_some_and(|o| !matches!(self.h.u.symbol(o).kind, Kind::Class | Kind::Package))
    }

    /// The name local `s` prints as: its own, subscripted from its second symbol on.
    fn spliced_name(&mut self, s: SymbolId) -> String {
        if let Some(n) = self.names.get(&s) {
            return n.clone();
        }
        let base = self.h.u.symbol(s).name.clone();
        let index = self.index.entry(base.clone()).or_insert(1);
        let name = match *index {
            1 => base.clone(),
            i => {
                let digits: String = i
                    .to_string()
                    .chars()
                    .map(|d| char::from_u32(d as u32 - '0' as u32 + 0x2080).unwrap_or(d))
                    .collect();
                format!("`{base}{digits}`")
            }
        };
        *index += 1;
        self.names.insert(s, name.clone());
        name
    }

    /// Type `t` as `printType` shows it, the spliced types substituted.
    fn ty(&mut self, t: TypeId) -> Shown {
        let spliced = std::mem::take(&mut self.spliced);
        let t = self.h.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => spliced.iter().find(|&&(x, _)| x == s).map(|&(_, t)| t),
            _ => None,
        });
        self.spliced = spliced;
        let shown = self.h.u.show_source(t)?;
        self.push(&shown);
        Ok(())
    }
}
