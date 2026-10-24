//! The symbols of a library: packages known up front, each top-level file's classes and members
//! entered when a name it defines is first looked up.

mod any_members;
mod bodies;
mod builtin;
mod classes;
mod companions;
mod define;
mod enter;
mod entries;
mod function_classes;
mod impure_functions;
mod info;
mod inits;
mod java_strings;
mod java_stubs;
mod jdk;
mod joined_bases;
mod loaded;
mod lookup;
mod members;
mod opaques;
mod open;
mod overrides;
mod packages;
mod predicates;
mod resolve;
mod seen;
mod symbol;
mod table;
mod varargs;
mod variance;

use crate::library::Library;
use crate::subtype::Unions;
use crate::types::{TypeId, Types};
use crate::{Ast, NodeId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::io;

pub use bodies::BodyInfo;
pub use builtin::{ANY, Builtins, NOTHING};
pub use companions::{module_class_name, module_name};
pub use function_classes::MAX_IMPLEMENTED_ARITY;
pub use jdk::defined_name;
pub use lookup::{decode, encode};
pub use packages::{PACKAGE_TABLE, package_table, package_table_version};
pub use predicates::is_package_object_name;
pub use symbol::{FileId, Flags, Kind, Symbol, SymbolId};
pub use table::{SYMBOL_TABLE_VERSION, symbol_table};

pub struct Universe {
    library: Library,
    symbols: Vec<Symbol>,
    packages: FxHashMap<String, SymbolId>,
    /// Per package, the files not loaded yet: the file name without `.tasty`, and its path.
    pending: FxHashMap<SymbolId, Vec<(String, String)>>,
    files: Vec<loaded::Loaded>,
    /// The symbol each definition tree defines.
    by_tree: FxHashMap<(FileId, NodeId), SymbolId>,
    /// Per owner whose declarations were looked up by name, its declarations by name.
    by_name: FxHashMap<SymbolId, FxHashMap<String, Vec<SymbolId>>>,
    /// The symbol each `TypeRef` or `TermRef` resolved to.
    resolved: FxHashMap<TypeId, SymbolId>,
    generation: u64,
    erased_methods: FxHashMap<(SymbolId, bool), (Vec<crate::Erased>, crate::Erased)>,
    types: Types,
    /// The declared type of each symbol whose type was asked for.
    infos: FxHashMap<SymbolId, TypeId>,
    parents: FxHashMap<SymbolId, Vec<TypeId>>,
    linearizations: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// Per class and base class, whether the class's parents give the base different types.
    disagreeing_bases: FxHashMap<(SymbolId, SymbolId), bool>,
    /// The abstract methods and values among the symbols defined outside TASTy.
    deferred: FxHashSet<SymbolId>,
    /// The classes defined outside TASTy that have initialization code.
    with_inits: FxHashSet<SymbolId>,
    /// The given self types of the classes defined outside TASTy that declare one.
    self_types: FxHashMap<SymbolId, TypeId>,
    /// The annotation classes of the symbols defined outside TASTy.
    annotations: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// The string literal that is the only argument of such an annotation, by symbol and class.
    annotation_strings: FxHashMap<(SymbolId, SymbolId), String>,
    /// The right-hand sides of the opaque types defined outside TASTy.
    opaque_aliases: FxHashMap<SymbolId, TypeId>,
    /// Whether those aliases are seen through every prefix (after dotty's `ElimOpaque`).
    opaques_elided: bool,
    /// The symbol whose definition comparisons are made in (dotty's `ctx.owner`), if known.
    comparison_owner: Option<SymbolId>,
    /// How many comparisons succeeded through a reference lifted to an object's `this`.
    opaque_lifts: u64,
    /// How many `resolve` calls are in progress.
    resolving: u32,
    builtins: Builtins,
    /// How many binders `fresh_binder` has made.
    binders: u32,
    unions: Unions,
    /// How many steps comparisons have taken computing atoms and `Nothing`-ness.
    pub subtype_steps: u64,
    /// Whether `erase` erases as Scala 2 does, for the signature of a Scala 2 symbol.
    scala2_erasure: bool,
}

#[derive(Debug)]
pub enum Error {
    Io(String, io::Error),
    Corrupt(String, crate::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(path, e) => write!(f, "{path}: {e}"),
            Error::Corrupt(path, e) => write!(f, "{path}: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl Universe {
    pub fn root(&self) -> SymbolId {
        SymbolId(0)
    }

    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0 as usize]
    }

    /// The id of the next symbol defined.
    pub fn next_symbol(&self) -> SymbolId {
        SymbolId(self.symbols.len() as u32)
    }

    /// The package of qualified name `name`, e.g. `scala.collection`, if the library has it.
    pub fn package(&self, name: &str) -> Option<SymbolId> {
        self.packages.get(name).copied()
    }

    /// The qualified name of `id`, e.g. `scala.collection.immutable.List`.
    pub fn full_name(&self, id: SymbolId) -> String {
        let s = self.symbol(id);
        match s.owner {
            Some(o) if o != self.root() => format!("{}.{}", self.full_name(o), s.name),
            _ => s.name.clone(),
        }
    }

    /// The names of `s` and its owners up to its package, joined by `$`, an object's without its
    /// `$` (dotty's `fullNameSeparated(ExpandPrefixName)`).
    pub fn expanded_prefix(&self, s: SymbolId) -> String {
        let sym = self.symbol(s);
        let name = module_name(&sym.name).unwrap_or(&sym.name);
        match sym.owner {
            Some(o) if self.symbol(o).kind == Kind::Package => match self.full_name(o).as_str() {
                "" | "<empty>" => name.to_owned(),
                package => format!("{}${name}", package.replace('.', "$")),
            },
            Some(o) => format!("{}${name}", self.expanded_prefix(o)),
            None => name.to_owned(),
        }
    }

    /// How many files are loaded.
    pub fn loaded_files(&self) -> usize {
        self.files.len()
    }

    /// The trees and the names, shown as text, of loaded file `file`, decoded on first use.
    pub fn file(&self, file: FileId) -> (&Ast, &[String]) {
        let f = self.files[file.0 as usize].decoded();
        (&f.ast, &f.names)
    }

    /// Whether `s` comes from the Scala 2 standard library (dotty's `Scala2x` flag).
    pub fn is_scala2(&self, s: SymbolId) -> bool {
        self.symbol(s).tree.is_some_and(|(file, _)| self.files[file.0 as usize].scala2)
    }

    /// Whether `erase` erases as Scala 2 does.
    pub fn scala2_erasure(&self) -> bool {
        self.scala2_erasure
    }

    /// Makes `erase` erase as Scala 2 does if `on`, returning the previous mode.
    pub fn set_scala2_erasure(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.scala2_erasure, on)
    }

    /// The arena of the types decoded so far.
    pub fn types(&self) -> &Types {
        &self.types
    }

    pub fn intern_type(&mut self, t: crate::Type) -> TypeId {
        self.types.intern(t)
    }

    /// The unions built so far and the atoms of unions.
    pub fn unions(&mut self) -> &mut Unions {
        &mut self.unions
    }
}
