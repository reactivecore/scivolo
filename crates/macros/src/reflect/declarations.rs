// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The declarations of classes by kind (`Symbol.declaredMethods`, `declaredFields`,
//! `declaredTypes` and their single forms), type members, and the static classes, methods and
//! packages `Symbol.requiredClass` and its siblings look up.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;
use tasty::{Kind, SymbolId};

impl Host {
    /// Runs declaration operation `op` on `args` and `text`: what is declared (`methods`,
    /// `fields`, `types`, `all`) and the name looked for, empty for all.
    pub fn run_declaration(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        let s = || symbol(arg(args, 0)?);
        match op {
            "declared" => {
                let (what, name) = text.split_once('\0').unwrap_or((text, ""));
                let s = s()?;
                let decls = if self.u.is_class(s) { self.u.symbol(s).decls.clone() } else { Vec::new() };
                let found = decls.into_iter().filter(|&d| {
                    self.is_declared(d, what) && (name.is_empty() || self.u.symbol(d).name == name)
                });
                numbers(found.map(|d| d.0 as i32).collect())
            }
            "requiredClass" => {
                if self.is_package_alias(text) {
                    let any = self.u.class_ref("scala.Any");
                    return numbers(vec![
                        self.u.class_symbol(any).ok_or("a library without scala.Any")?.0 as i32,
                    ]);
                }
                let c = self.u.class_ref(text);
                let c = self.u.class_symbol(c).or_else(|| self.static_module_class(text));
                numbers(vec![c.ok_or("Symbol.requiredClass of a class the frontend lacks")?.0 as i32])
            }
            "requiredMethod" => {
                let (class, name) =
                    text.rsplit_once('.').ok_or("Symbol.requiredMethod of a name without a class")?;
                let c = match self.package_object_value(class) {
                    Some(v) => {
                        let t = self.u.info(v);
                        let t = self.u.widen(t);
                        self.u.class_symbol(t)
                    }
                    None => {
                        let c = self.u.class_ref(class);
                        self.u.class_symbol(c)
                    }
                };
                let c = c.ok_or("Symbol.requiredMethod in a class the frontend lacks")?;
                let methods = self.u.member_where(c, name, |u, m| u.symbol(m).kind == Kind::Def);
                let m = methods.first().copied();
                numbers(vec![m.ok_or("Symbol.requiredMethod of a method the frontend lacks")?.0 as i32])
            }
            "requiredPackage" => match self.u.package(text) {
                Some(p) => numbers(vec![p.0 as i32]),
                None => Err("Symbol.requiredPackage of a package the frontend lacks"),
            },
            "typeMember" => {
                let s = s()?;
                let found = match self.u.is_class(s) {
                    true => self.u.member_where(s, text, |u, m| u.symbol(m).kind.is_type()),
                    false => Vec::new(),
                };
                numbers(vec![found.first().map_or(-1, |m| m.0 as i32)])
            }
            _ => Err(UNKNOWN),
        }
    }

    /// Whether `full` names a type alias of a package or its package object.
    fn is_package_alias(&mut self, full: &str) -> bool {
        let (pkg, name) = full.rsplit_once('.').unwrap_or(("", full));
        let Some(p) = self.u.package(pkg) else { return false };
        let objects = self.u.decl(p, "package").unwrap_or_default();
        let object = objects.into_iter().find(|&d| self.u.is_module(d)).and_then(|m| self.u.module_class(m));
        let decls =
            [Some(p), object].into_iter().flatten().flat_map(|o| self.u.decl(o, name).unwrap_or_default());
        let decls: Vec<SymbolId> = decls.collect();
        decls.iter().any(|&d| self.u.symbol(d).kind == Kind::Type && !self.u.is_class(d))
            && !decls.iter().any(|&d| self.u.is_class(d))
    }

    /// The value named `full` of the package object of its package, if there is one.
    fn package_object_value(&mut self, full: &str) -> Option<SymbolId> {
        let (pkg, name) = full.rsplit_once('.')?;
        let p = self.u.package(pkg)?;
        let objects = self.u.decl(p, "package").unwrap_or_default();
        let object =
            objects.into_iter().find(|&d| self.u.is_module(d)).and_then(|m| self.u.module_class(m))?;
        let vals = self.u.decl(object, name).unwrap_or_default();
        vals.into_iter().find(|&v| self.u.symbol(v).kind == Kind::Val)
    }

    /// The module class of the static object named `full`, if there is one.
    fn static_module_class(&mut self, full: &str) -> Option<SymbolId> {
        let (pkg, name) = full.rsplit_once('.').unwrap_or(("", full));
        let p = self.u.package(pkg)?;
        let decls = self.u.decl(p, name).unwrap_or_default();
        let module = decls.into_iter().find(|&d| self.u.is_module(d))?;
        self.u.module_class(module)
    }

    /// Whether declaration `d` is of kind `what`: a method other than a constructor, a field,
    /// a type, or any of them; never a local dummy.
    fn is_declared(&self, d: SymbolId, what: &str) -> bool {
        let s = self.u.symbol(d);
        if s.name.starts_with("<local ") {
            return false;
        }
        match what {
            "methods" => s.kind == Kind::Def && s.name != "<init>",
            "fields" => s.kind.is_term() && s.kind != Kind::Def,
            "types" => s.kind.is_type(),
            _ => true,
        }
    }
}
