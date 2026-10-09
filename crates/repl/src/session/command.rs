//! The commands as a session runs them; `:type` types its input in a wrapper object of its own,
//! which is neither run nor imported by later inputs.

use super::{RESETTING, Session};
use crate::commands::{self, Command};
use crate::input::{self, Input};
use crate::report;
use crate::show_methods::SUFFIX;
use tasty::tags::{FIELDaccessor, MUTABLE, OBJECT};
use tasty::{Kind, SymbolId, Type};
use typer::Typed;
use typer::tpd::{TNodeId, Tree};

/// The prefix of the names of the wrapper objects of `:type`.
const PREFIX: &str = "rs$line$type$";

impl Session {
    /// The lines command `c` prints.
    pub fn command(&mut self, c: Command) -> Vec<String> {
        match c {
            Command::Quit => {
                self.exit = Some(0);
                Vec::new()
            }
            Command::Help => vec![commands::HELP.to_owned()],
            Command::Reset(_) => {
                self.reset();
                vec![RESETTING.to_owned()]
            }
            Command::TypeOf(expr) => self.type_of(&expr),
            Command::Silent => {
                self.quiet = !self.quiet;
                Vec::new()
            }
            Command::Imports => vec![report::unsupported(":imports")],
            Command::Load(path) => self.load(&path),
            Command::Unsupported(name) => {
                vec![commands::retired(name).map_or_else(|| report::unsupported(name), str::to_owned)]
            }
            Command::Ambiguous(name, matching) => vec![commands::ambiguous(&name, &matching)],
            Command::Unknown(name) => vec![commands::unknown(&name)],
        }
    }

    /// The lines `:type expr` prints: the type of the last statement of `expr`, or its errors.
    /// It takes no `res` index.
    fn type_of(&mut self, expr: &str) -> Vec<String> {
        if expr.is_empty() {
            return vec![":type <expression>".to_owned()];
        }
        let object = self.fresh.next(&mut self.names, PREFIX);
        let object = self.names.text(object).to_owned();
        let width = self.width as usize;
        let mut parsed = match input::classify(&object, expr, &mut self.names, &mut self.fresh, width) {
            Input::Stats(parsed) => *parsed,
            Input::Errors(errors) => return errors,
            Input::Empty => return vec!["Error: last of empty list".to_owned()],
            Input::Command(_) => return vec![report::unsupported("a command as expression")],
        };
        let res = self.res;
        let typed = self.typed(&mut parsed, &object);
        let expression = (self.res > res).then(|| format!("res{}", self.res - 1));
        self.res = res;
        let (typed, template) = match typed {
            Ok(typed) => typed,
            Err(out) => return out,
        };
        self.last_type(&typed, template, expression.as_deref()).into_iter().collect()
    }

    /// The widened type of the last definition of wrapper template `template`, setters and show
    /// methods aside; of its right-hand side if it is value `expression`, which an expression
    /// became.
    fn last_type(&mut self, typed: &Typed, template: TNodeId, expression: Option<&str>) -> Option<String> {
        let Tree::Template { body, .. } = *typed.tpd.tree(template) else { return None };
        let defs = typed.tpd.items(body).iter().rev().filter_map(|&s| Some((s, typed.tpd.sym(s)?)));
        let (last, sym) = defs.into_iter().find(|&(_, sym)| !self.generated(sym))?;
        let is_expression = expression == Some(self.u.symbol(sym).name.as_str());
        let t = match *typed.tpd.tree(last) {
            Tree::ValDef { rhs: Some(rhs), .. } if is_expression => typed.tpd.tpe(rhs),
            _ if matches!(self.u.symbol(sym).kind, Kind::Class | Kind::Type) => {
                self.u.intern_type(Type::Sym(None, sym))
            }
            _ => self.u.info(sym),
        };
        let t = self.u.widen(t);
        let class = match self.u.types().get(t).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
            _ => None,
        };
        Some(match class {
            Some(c) if self.is_static_module_class(c) => self.u.show_module_class(c),
            _ => self.u.show_user(t),
        })
    }

    fn is_static_module_class(&self, c: SymbolId) -> bool {
        let s = self.u.symbol(c);
        s.kind == Kind::Class && s.flags.has(OBJECT) && self.u.is_static_symbol(c)
    }

    /// Whether `sym` is a show method or a variable's setter.
    fn generated(&self, sym: SymbolId) -> bool {
        let s = self.u.symbol(sym);
        s.name.ends_with(SUFFIX)
            || (s.kind == Kind::Def && s.flags.has(FIELDaccessor) && s.flags.has(MUTABLE))
    }
}
