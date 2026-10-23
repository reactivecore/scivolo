// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/Extractors.scala
//! Trees as `Printer.TreeStructure` shows them (dotty's `Extractors.visitTree`), without the
//! types they hold.

use super::{Code, REFUSED, Shown};
use syntax::ast::constant::Constant as C;
use typer::tpd::{TNodeId, Tree as T};

const STRUCTURE: &str = "Printer.TreeStructure of a tree the frontend does not print";

impl Code<'_> {
    pub fn structure(&mut self, t: TNodeId) -> Shown {
        let t = self.r(t);
        let is_type = self.is_type_tree(t);
        let named = |c: &Self, what: &str, n| format!("{what}(\"{}\"", c.name(n));
        match self.tree_of(t) {
            T::Ident { name } if self.name(name) == "_" => self.push("Wildcard()"),
            T::Ident { name } if is_type => self.push(&format!("{})", named(self, "TypeIdent", name))),
            T::Ident { name } => self.push(&format!("{})", named(self, "Ident", name))),
            T::Select { qualifier, name } => {
                self.push(if is_type { "TypeSelect(" } else { "Select(" });
                self.structure(qualifier)?;
                self.push(&format!(", \"{}\")", self.name(name)));
            }
            T::This { qual } => {
                let name = match self.tree_of(qual) {
                    T::Ident { name } => self.name(name),
                    _ => return Err(STRUCTURE),
                };
                match name.is_empty() {
                    true => self.push("This(None)"),
                    false => self.push(&format!("This(Some(\"{name}\"))")),
                }
            }
            T::Apply { fun, args } => self.node("Apply", &[fun], &[args])?,
            T::TypeApply { fun, args } => self.node("TypeApply", &[fun], &[args])?,
            T::Literal { value } => {
                self.push("Literal(");
                self.structure_constant(&value)?;
                self.push(")");
            }
            T::New { tpt } => self.node("New", &[tpt], &[])?,
            T::Typed { expr, tpt } => {
                let pattern =
                    matches!(self.tree_of(expr), T::UnApply { .. } | T::Bind { .. } | T::Alternative { .. });
                self.node(if pattern { "TypedOrTest" } else { "Typed" }, &[expr, tpt], &[])?
            }
            T::NamedArg { name, arg } => {
                self.push(&format!("{}, ", named(self, "NamedArg", name)));
                self.structure(arg)?;
                self.push(")");
            }
            T::Assign { lhs, rhs } => self.node("Assign", &[lhs, rhs], &[])?,
            T::Block { stats, expr } => {
                self.push("Block(");
                self.list(stats)?;
                self.push(", ");
                self.structure(expr)?;
                self.push(")");
            }
            T::If { cond, thenp, elsep, .. } => self.node("If", &[cond, thenp, elsep], &[])?,
            T::Closure { meth, tpt: None, .. } => {
                self.push("Closure(");
                self.structure(meth)?;
                self.push(", None)");
            }
            T::Match { selector: Some(s), cases, .. } => self.node("Match", &[s], &[cases])?,
            T::Match { selector: None, cases, .. } => self.node("SummonFrom", &[], &[cases])?,
            T::WhileDo { cond, body } => self.node("While", &[cond, body], &[])?,
            T::Try { expr, cases, finalizer } => {
                self.push("Try(");
                self.structure(expr)?;
                self.push(", ");
                self.list(cases)?;
                self.push(", ");
                self.option(finalizer)?;
                self.push(")");
            }
            T::SeqLiteral { elems, elemtpt } => {
                self.push("Repeated(");
                self.list(elems)?;
                self.push(", ");
                self.structure(elemtpt)?;
                self.push(")");
            }
            T::Inlined { call: None, bindings, expansion } => {
                self.push("Inlined(None, ");
                self.list(bindings)?;
                self.push(", ");
                self.structure(expansion)?;
                self.push(")");
            }
            T::ValDef { name, tpt, rhs } => {
                self.push(&format!("{}, ", named(self, "ValDef", name)));
                self.structure(tpt)?;
                self.push(", ");
                self.option(rhs)?;
                self.push(")");
            }
            T::DefDef { name, paramss, tpt, rhs } => {
                self.push(&format!("{}, ", named(self, "DefDef", name)));
                let clauses = self.h.tpd.clause_items(paramss).to_vec();
                self.push(if clauses.is_empty() { "Nil" } else { "List(" });
                for (i, &c) in clauses.iter().enumerate() {
                    let ty =
                        self.items(c).first().is_some_and(|&p| matches!(self.tree_of(p), T::TypeDef { .. }));
                    self.push(if i > 0 { ", " } else { "" });
                    self.push(if ty { "TypeParamClause(" } else { "TermParamClause(" });
                    self.list(c)?;
                    self.push(")");
                }
                self.push(if clauses.is_empty() { ", " } else { "), " });
                self.structure(tpt)?;
                self.push(", ");
                self.option(rhs)?;
                self.push(")");
            }
            T::TypeDef { name, rhs } if !matches!(self.tree_of(rhs), T::Template { .. }) => {
                self.push(&format!("{}, ", named(self, "TypeDef", name)));
                self.structure(rhs)?;
                self.push(")");
            }
            T::TypeTree { .. } if is_type => self.push("Inferred()"),
            T::SingletonTypeTree { reference } => self.node("Singleton", &[reference], &[])?,
            T::AppliedTypeTree { tpt, args } => self.node("Applied", &[tpt], &[args])?,
            T::ByNameTypeTree { result } => self.node("ByName", &[result], &[])?,
            T::Annotated { arg, annot } => self.node("Annotated", &[arg, annot], &[])?,
            T::TypeBoundsTree { lo, hi, alias: None } => self.node("TypeBoundsTree", &[lo, hi], &[])?,
            T::CaseDef { pat, guard, body } => {
                self.push("CaseDef(");
                self.structure(pat)?;
                self.push(", ");
                self.option(guard)?;
                self.push(", ");
                self.structure(body)?;
                self.push(")");
            }
            T::Bind { name, body } => {
                self.push(&format!("{}, ", named(self, "Bind", name)));
                self.structure(body)?;
                self.push(")");
            }
            T::UnApply { fun, implicits, patterns } => {
                self.node("Unapply", &[fun], &[implicits, patterns])?
            }
            T::Alternative { trees } => self.node("Alternatives", &[], &[trees])?,
            _ => return Err(STRUCTURE),
        }
        Ok(())
    }

    /// `what(` the trees `trees`, then the lists `lists`, `)`.
    fn node(&mut self, what: &str, trees: &[TNodeId], lists: &[typer::tpd::List]) -> Shown {
        self.push(what);
        self.push("(");
        for (i, &t) in trees.iter().enumerate() {
            self.push(if i > 0 { ", " } else { "" });
            self.structure(t)?;
        }
        for (i, &l) in lists.iter().enumerate() {
            self.push(if i > 0 || !trees.is_empty() { ", " } else { "" });
            self.list(l)?;
        }
        self.push(")");
        Ok(())
    }

    fn list(&mut self, l: typer::tpd::List) -> Shown {
        let items = self.items(l);
        if items.is_empty() {
            self.push("Nil");
            return Ok(());
        }
        self.push("List(");
        for (i, &t) in items.iter().enumerate() {
            self.push(if i > 0 { ", " } else { "" });
            self.structure(t)?;
        }
        self.push(")");
        Ok(())
    }

    fn option(&mut self, t: Option<TNodeId>) -> Shown {
        let Some(t) = t else {
            self.push("None");
            return Ok(());
        };
        self.push("Some(");
        self.structure(t)?;
        self.push(")");
        Ok(())
    }

    fn structure_constant(&mut self, c: &C) -> Shown {
        let text = match c {
            C::Unit => "UnitConstant()".to_owned(),
            C::Null => "NullConstant()".to_owned(),
            C::Boolean(b) => format!("BooleanConstant({b})"),
            C::Byte(n) => format!("ByteConstant({n})"),
            C::Short(n) => format!("ShortConstant({n})"),
            C::Int(n) => format!("IntConstant({n})"),
            C::Long(n) => format!("LongConstant({n}L)"),
            C::Float(x) => format!(
                "FloatConstant({}f)",
                super::literals::java_decimal(*x as f64, format!("{x:?}")).ok_or(REFUSED)?
            ),
            C::Double(x) => format!(
                "DoubleConstant({}d)",
                super::literals::java_decimal(*x, format!("{x:?}")).ok_or(REFUSED)?
            ),
            &C::Char(u) => format!("CharConstant('{}')", char::from_u32(u as u32).unwrap_or('\u{fffd}')),
            C::String(units) => format!("StringConstant(\"{}\")", String::from_utf16_lossy(units)),
        };
        self.push(&text);
        Ok(())
    }
}
