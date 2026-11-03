//! The JDK's declarations of classes javalib defines (`libs/jdk-stubs/JdkDeclarations.scala`),
//! entered over javalib's classes: each member they declare replaces javalib's of its name.

use super::entries::Entry;
use super::{FileId, Kind, Symbol, SymbolId, Universe};
use crate::tags::{BYTEconst, FINAL, OBJECT, PRIVATE, PRIVATEqualified, SHORTconst, SYNTHETIC};
use crate::types::{Type, TypeId};

/// The directory of the JDK's declarations in the library, beside the packages.
pub const JDK_DECLARATIONS: &str = "jdk-declarations/";

#[derive(Clone, Copy)]
enum Entered {
    /// javalib's symbol of the declaration.
    Merged(SymbolId),
    /// A symbol new to javalib's class.
    New(SymbolId),
    Skipped,
}

impl Universe {
    /// Loads the JDK's declarations of the classes of file `path` over its symbols, unless
    /// javalib's sources are compiled or `path` is a JDK jar's.
    pub fn load_jdk_declarations(&mut self, path: &str) -> Result<(), super::Error> {
        let jdk_jar = self.library.jar_of(path).is_some_and(|j| self.jdk_jars.contains(&j));
        if self.javalib_sources || jdk_jar {
            return Ok(());
        }
        let declared = format!("{JDK_DECLARATIONS}{path}");
        match self.read_entries(&declared) {
            Ok((file, entries)) => {
                self.enter_declarations(file, entries);
                Ok(())
            }
            Err(super::Error::Io(_, e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Enters `entries` of file `file` over javalib's symbols: a class or object javalib has, of
    /// a renamed class under its JDK name, is merged into its, a type parameter into javalib's at
    /// its position with its bounds, a member replaces javalib's of its name; constructors,
    /// synthetic members and what javalib lacks are skipped.
    fn enter_declarations(&mut self, file: FileId, entries: Vec<Entry>) {
        let mut ids: Vec<Entered> = Vec::with_capacity(entries.len());
        let mut type_params: Vec<SymbolId> = Vec::new();
        let mut replaced: Vec<(SymbolId, String)> = Vec::new();
        for e in entries {
            let owner = e.owner.map_or(Entered::Merged(self.root()), |o| ids[o as usize]);
            let entered = match (owner, e.kind) {
                (Entered::Skipped, _) => Entered::Skipped,
                (_, None) => Entered::Merged(self.enter_package(&e.name)),
                (Entered::New(owner), Some(kind)) => {
                    Entered::New(self.define_declared(owner, kind, file, &e))
                }
                (Entered::Merged(owner), Some(kind)) => match kind {
                    Kind::Class | Kind::Val if kind == Kind::Class || e.flags.has(OBJECT) => {
                        let (owner, name) = self.jdk_renamed(owner, e.name.clone());
                        self.javalib_decl(owner, &name, kind)
                    }
                    Kind::TypeParam => {
                        let n = type_params.iter().filter(|&&c| c == owner).count();
                        type_params.push(owner);
                        let params = self.symbol(owner).decls.iter().copied();
                        let mut params = params.filter(|&d| self.symbol(d).kind == Kind::TypeParam);
                        let param = params.nth(n);
                        if let Some(p) = param {
                            self.symbols[p.0 as usize].tree = Some((file, e.tree));
                        }
                        param.map_or(Entered::Skipped, Entered::Merged)
                    }
                    Kind::Param => Entered::Skipped,
                    _ if e.name == "<init>" || e.flags.has(SYNTHETIC) => Entered::Skipped,
                    _ => {
                        if !replaced.iter().any(|(c, n)| *c == owner && *n == e.name) {
                            self.hide_decls(owner, &e.name);
                            replaced.push((owner, e.name.clone()));
                        }
                        Entered::New(self.define_declared(owner, kind, file, &e))
                    }
                },
            };
            if let Entered::Merged(s) = entered
                && e.kind.is_some()
            {
                self.by_tree.insert((file, e.tree), s);
            }
            ids.push(entered);
        }
    }

    /// javalib's declaration of `owner` named `name` of kind `kind`, made public as the JDK's.
    fn javalib_decl(&mut self, owner: SymbolId, name: &str, kind: Kind) -> Entered {
        let decls = self.symbol(owner).decls.iter().copied();
        let mut named = decls.filter(|&d| self.symbol(d).name == name && self.symbol(d).kind == kind);
        let Some(d) = named.next() else { return Entered::Skipped };
        let flags = &mut self.symbols[d.0 as usize].flags;
        *flags = flags.without(PRIVATE).without(PRIVATEqualified);
        Entered::Merged(d)
    }

    /// Removes the terms and types `owner` declares named `name`.
    fn hide_decls(&mut self, owner: SymbolId, name: &str) {
        let decls = self.symbol(owner).decls.clone();
        for d in decls {
            let s = self.symbol(d);
            if s.name == name && !matches!(s.kind, Kind::Class | Kind::Package) {
                self.unenter(d);
            }
        }
    }

    /// Type `t` of a member of file `file` with its repeated parameters `Seq[T] @Repeated` made
    /// Java varargs, `Array[T] @Repeated`, if the file holds JDK declarations.
    pub fn with_declared_varargs(&mut self, file: FileId, t: TypeId) -> TypeId {
        if !self.files[file.0 as usize].path.starts_with(JDK_DECLARATIONS) {
            return t;
        }
        self.map_type(t, &mut |u, part| {
            let Type::Annotated(seq, annot) = *u.types().get(part) else { return None };
            let Type::Applied(_, ref args) = *u.types().get(seq) else { return None };
            let args = args.clone();
            let repeated =
                u.class_symbol(annot).is_some_and(|c| u.full_name(c) == "scala.annotation.internal.Repeated");
            if !repeated || args.len() != 1 {
                return None;
            }
            let array = u.class_ref("scala.Array");
            let array = u.intern_type(Type::Applied(array, args));
            Some(u.intern_type(Type::Annotated(array, annot)))
        })
    }

    /// Type `t` of value `sym` of file `file` made the constant of its `Byte` or `Short` literal
    /// right-hand side if the file holds JDK declarations and `sym` is final, as Java's
    /// `static final` fields with a constant are.
    pub fn with_declared_constant(&mut self, sym: SymbolId, file: FileId, t: TypeId) -> TypeId {
        let s = self.symbol(sym);
        let declared = self.files[file.0 as usize].path.starts_with(JDK_DECLARATIONS);
        if !declared || s.kind != Kind::Val || !s.flags.has(FINAL) {
            return t;
        }
        let Some((file, rhs)) = self.body(sym) else { return t };
        let n = self.file(file).0.node(rhs);
        match n.tag {
            BYTEconst | SHORTconst => self.intern_type(Type::Constant(n.tag, n.a)),
            _ => t,
        }
    }

    fn define_declared(&mut self, owner: SymbolId, kind: Kind, file: FileId, e: &Entry) -> SymbolId {
        self.define(Symbol {
            name: e.name.clone(),
            kind,
            owner: Some(owner),
            flags: e.flags,
            target: e.target.clone(),
            is_extern: e.is_extern,
            derived: e.derived,
            decls: Vec::new(),
            tree: Some((file, e.tree)),
        })
    }
}
