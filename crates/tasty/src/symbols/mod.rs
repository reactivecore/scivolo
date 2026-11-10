//! The symbols of a library: packages known up front, each top-level file's classes and members
//! entered when a name it defines is first looked up.

mod aligned;
mod annotations;
mod any_members;
mod base_types;
mod bodies;
mod builtin;
mod children;
mod classes;
mod classfiles;
mod companions;
mod define;
pub mod defn;
mod enter;
mod entries;
mod function_classes;
mod gadt_bounds;
mod higher_kinds;
mod impure_functions;
mod info;
mod inits;
mod java_nulls;
mod java_params;
mod java_strings;
mod java_stubs;
mod java_wildcards;
mod jdk;
mod jdk_declarations;
mod joined_bases;
mod joint;
mod loaded;
mod lookup;
mod members;
mod normal_form;
mod opaques;
mod open;
mod overrides;
mod packages;
mod predicates;
mod resolve;
mod runtime_stubs;
mod same_run;
mod seen;
mod selves;
mod symbol;
mod table;
mod varargs;
mod variance;

use crate::library::Library;
use crate::subtype::Unions;
use crate::types::{Binder, TypeId, Types};
use crate::{Ast, NodeId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::io;

pub use bodies::BodyInfo;
pub use builtin::{ANY, NOTHING};
pub use companions::{module_class_name, module_name};
pub use defn::Defn;
pub use function_classes::MAX_IMPLEMENTED_ARITY;
pub use jdk::{defined_name, jdk_name};
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
    /// The type under explicit nulls of each symbol whose type was asked for in that mode.
    nullified_infos: FxHashMap<SymbolId, TypeId>,
    /// The GADT bounds of type parameters (dotty's `GadtConstraint`), compared frozen.
    gadt_bounds: FxHashMap<SymbolId, (TypeId, TypeId)>,
    parents: FxHashMap<SymbolId, Vec<TypeId>>,
    linearizations: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// Per class and base class, whether the class's parents give the base different types.
    disagreeing_bases: FxHashMap<(SymbolId, SymbolId), bool>,
    /// The abstract methods and values among the symbols defined outside TASTy.
    deferred: FxHashSet<SymbolId>,
    /// The classes defined outside TASTy that have initialization code.
    with_inits: FxHashSet<SymbolId>,
    /// Whether a unit replaces classes of the Scala library.
    builds_scala_library: bool,
    /// The classes defined outside TASTy in a unit that behaves as the Scala 2 library.
    scala2_classes: FxHashSet<SymbolId>,
    /// The methods lifted out of terms into classes.
    lifted: FxHashSet<SymbolId>,
    /// The Java classes of the Scala library entered without TASTy, their objects and objects'
    /// classes.
    java_stubs: FxHashSet<SymbolId>,
    /// The JDK whose class files give the JDK classes the library lacks, if one is used.
    jdk: Option<classfile::Jdk>,
    /// The classes, objects and objects' classes entered from the JDK's class files.
    classfile_classes: FxHashSet<SymbolId>,
    /// The descriptor of each member entered from them.
    classfile_descriptors: FxHashMap<SymbolId, String>,
    /// The library's classes the JDK's class files were asked for the members they lack.
    jdk_member_classes: FxHashSet<SymbolId>,
    /// The given self type of each class whose self type was asked for or set.
    self_types: FxHashMap<SymbolId, Option<TypeId>>,
    /// The annotation classes of the symbols defined outside TASTy.
    annotations: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// The string literal that is the only argument of each such annotation, by symbol and class.
    annotation_strings: FxHashMap<(SymbolId, SymbolId), Vec<Option<String>>>,
    /// The `scala.annotation.meta` classes meta-annotating each such annotation, by symbol and
    /// class.
    annotation_metas: FxHashMap<(SymbolId, SymbolId), Vec<SymbolId>>,
    /// The class or package `q` of `private[q]` or `protected[q]` of the symbols defined outside
    /// TASTy.
    access_qualifiers: FxHashMap<SymbolId, SymbolId>,
    /// The right-hand sides of the opaque types defined outside TASTy.
    opaque_aliases: FxHashMap<SymbolId, TypeId>,
    /// The constructor proxy object made for each class outside its owner's declarations.
    constructor_proxies: FxHashMap<SymbolId, SymbolId>,
    /// Whether those aliases are seen through every prefix (after dotty's `ElimOpaque`).
    opaques_elided: bool,
    /// The symbol whose definition comparisons are made in (dotty's `ctx.owner`), if known.
    comparison_owner: Option<SymbolId>,
    null_mode: crate::NullMode,
    /// How many comparisons succeeded through a reference lifted to an object's `this`.
    opaque_lifts: u64,
    /// Whether the sources compiled are javalib's, whose TASTy classes are then Scala classes.
    javalib_sources: bool,
    /// Whether javalib's members that JDK 17 lacks are left out where only the JDK API's are seen.
    jdk_17_view: bool,
    /// The indices of the jars whose classes stand for the JDK's.
    jdk_jars: std::ops::Range<usize>,
    /// The jar whose classes are compiled in the same run as the sources, if one is.
    same_run_jar: Option<usize>,
    /// How many `resolve` calls are in progress.
    resolving: u32,
    /// The intersections and unions whose base types `base_type` is merging, with the base class.
    merging_bases: FxHashSet<(TypeId, SymbolId)>,
    defn: Defn,
    /// How many binders `fresh_binder` has made.
    binders: u32,
    /// The declared variances of the type lambdas made outside TASTy that declare them.
    lambda_variances: FxHashMap<Binder, Vec<i8>>,
    unions: Unions,
    joint_members: joint::JointMembers,
    /// How many steps comparisons have taken computing atoms and `Nothing`-ness.
    pub subtype_steps: u64,
    /// How many match type reductions are in progress, over all comparisons.
    pub reducing: u32,
    /// How many more reductions the outermost reduction in progress may start.
    pub reductions_left: u32,
    /// Whether a reduction was refused: over a budget, or reaching its own match type again.
    pub reductions_exhausted: bool,
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
    /// `$` and a `_$` for each term between it and its enclosing class (dotty's
    /// `fullNameSeparated(ExpandPrefixName)`).
    pub fn expanded_prefix(&self, s: SymbolId) -> String {
        let sym = self.symbol(s);
        let mut name = module_name(&sym.name).unwrap_or(&sym.name).to_owned();
        let mut owner = sym.owner;
        while let Some(o) = owner.filter(|&o| self.symbol(o).kind.is_term()) {
            name.insert_str(0, "_$");
            owner = self.symbol(o).owner;
        }
        match owner {
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

    /// Whether name `i` of loaded file `file` is of a kind other than `Utf8`.
    pub fn is_derived_name(&self, file: FileId, i: usize) -> bool {
        self.files[file.0 as usize].decoded().derived.get(i).copied().unwrap_or_default()
    }

    /// Whether `s` comes from the Scala 2 standard library (dotty's `Scala2x` flag).
    pub fn is_scala2(&self, s: SymbolId) -> bool {
        match self.symbol(s).tree {
            Some((file, _)) => self.files[file.0 as usize].scala2,
            None => self.scala2_classes.contains(&s),
        }
    }

    /// Whether `s` comes from TASTy written before Scala 3.4 (dotty's
    /// `methCompiledBeforeDeprecation`).
    pub fn compiled_before_3_4(&self, s: SymbolId) -> bool {
        let Some((file, _)) = self.symbol(s).tree else { return false };
        crate::Header::parse(&self.files[file.0 as usize].bytes).is_ok_and(|h| h.minor < 4)
    }

    /// Records that class `c`, defined outside TASTy, behaves as the Scala 2 library's.
    pub fn set_scala2(&mut self, c: SymbolId) {
        self.scala2_classes.insert(c);
    }

    /// Records that method `m` was lifted out of a term into a class.
    pub fn set_lifted(&mut self, m: SymbolId) {
        self.lifted.insert(m);
    }

    /// Whether method `m` was lifted out of a term into a class.
    pub fn is_lifted(&self, m: SymbolId) -> bool {
        self.lifted.contains(&m)
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

    /// The id of name `text` in the name table of the types.
    pub fn intern_name(&mut self, text: &str) -> crate::NameId {
        self.types.intern_name(text)
    }

    /// The name of symbol `s`, interned.
    pub fn interned_name(&mut self, s: SymbolId) -> crate::NameId {
        self.types.intern_name(&self.symbols[s.0 as usize].name)
    }

    /// The constant type of string `text`.
    pub fn str_type(&mut self, text: &str) -> TypeId {
        let s = self.types.intern_name(text);
        self.types.intern(crate::Type::Str(s))
    }

    /// The unions built so far and the atoms of unions.
    pub fn unions(&mut self) -> &mut Unions {
        &mut self.unions
    }
}
