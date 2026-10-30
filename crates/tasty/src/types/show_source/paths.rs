// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! The prefixes of references and `this` types as `SourceCode.printType` shows them.

use super::{Highlight, REFUSED, Shown, SourcePrinter};
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId};

impl SourcePrinter<'_> {
    /// A term reference `name` through prefix `p`.
    pub fn term_ref(&mut self, p: TypeId, name: &str) -> Shown {
        if self.full && !self.is_elided(p) && !matches!(self.u.types().get(p), Type::NoPrefix) {
            self.print(p)?;
            if name == "package" {
                return Ok(());
            }
            self.out.push('.');
        }
        self.paint(Highlight::Type, name);
        Ok(())
    }

    /// The prefix of a type reference, `.` or `#` included.
    pub fn type_prefix(&mut self, p: TypeId) -> Shown {
        if !self.full || self.is_elided(p) {
            return Ok(());
        }
        match *self.u.types().get(p) {
            Type::NoPrefix => Err(REFUSED),
            Type::Sym(_, s) | Type::Package(s) if self.u.symbol(s).kind == Kind::Package => {
                self.package(s);
                Ok(())
            }
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class => {
                self.print(p)?;
                self.out.push('#');
                Ok(())
            }
            _ => {
                self.print(p)?;
                self.out.push('.');
                Ok(())
            }
        }
    }

    /// The prefix dotty gives a reference to `s` without one: its package, its object or its
    /// class's `this`; none for a local or a type parameter.
    pub fn owner_prefix(&mut self, s: SymbolId) -> Shown {
        let Some(owner) = self.u.symbol(s).owner.filter(|_| self.full) else { return Ok(()) };
        if self.u.symbol(s).kind == Kind::TypeParam || self.u.symbol(owner).kind.is_term() {
            return Ok(());
        }
        match self.u.symbol(owner).kind {
            Kind::Package => {
                self.package(owner);
                Ok(())
            }
            Kind::Class if self.u.is_module_class(owner) => {
                let module = self.u.source_module(owner).ok_or(REFUSED)?;
                self.owner_prefix(module)?;
                self.paint(Highlight::Type, &self.u.symbol(module).name.clone());
                self.out.push('.');
                Ok(())
            }
            _ => Err(REFUSED),
        }
    }

    /// Package `s` as a prefix, its class's `this`: its full name and `.`, nothing for the root.
    fn package(&mut self, s: SymbolId) {
        if s != self.u.root() {
            self.package_name(s, false);
            self.out.push('.');
        }
    }

    /// `C.this` (dotty's `ThisType`): a package by its name, an object by its path.
    pub fn this(&mut self, c: TypeId) -> Shown {
        let class = match *self.u.types().get(c) {
            Type::Package(p) => p,
            _ => self.u.class_symbol(c).ok_or(REFUSED)?,
        };
        match self.u.symbol(class).kind {
            Kind::Package if class == self.u.root() => Err(REFUSED),
            Kind::Package if self.full => {
                self.package_name(class, false);
                Ok(())
            }
            _ if self.u.is_module_class(class) => {
                let module = self.u.source_module(class).ok_or(REFUSED)?;
                self.owner_prefix(module)?;
                self.paint(Highlight::Type, &self.u.symbol(module).name.clone());
                Ok(())
            }
            Kind::Package => {
                self.paint(Highlight::Type, &self.u.symbol(class).name.clone());
                Ok(())
            }
            _ => {
                let name = self.u.symbol(class).name.clone();
                self.out.push_str(&name);
                self.paint(Highlight::Type, ".this");
                Ok(())
            }
        }
    }

    /// Whether prefix `p` is the root's or the empty package's `this`, which dotty omits.
    fn is_elided(&mut self, p: TypeId) -> bool {
        let root = self.u.root();
        match *self.u.types().get(p) {
            Type::This(c) => match *self.u.types().get(c) {
                Type::Package(s) => s == root,
                _ => self.u.class_symbol(c) == Some(root),
            },
            Type::Package(s) => s == root,
            _ => false,
        }
    }
}
