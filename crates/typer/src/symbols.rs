// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! The typer's side of script symbols: their dotty flags, what completes their infos, their
//! annotations; and the TASTy modifiers the universe keeps for them.

use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use rustc_hash::FxHashMap;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{NodeId, Span};
use tasty::tags as t;
use tasty::{Binder, FileId, Flags, Kind, Symbol, SymbolId, Universe};

/// The file of the binders of `Symbols::fresh_binder`.
pub const SCRIPT_BINDERS: FileId = FileId(u32::MAX);

/// How far the info of a script symbol is known (dotty's completers).
#[derive(Clone, Copy, Debug)]
pub enum Completion {
    /// Its definition `tree` is still to be typed in context `ctx`.
    Pending {
        tree: NodeId,
        ctx: Ctx,
    },
    /// Its definition is being typed.
    Completing(NodeId),
    Done,
}

#[derive(Clone, Default)]
pub struct Syms {
    pub flags: FxHashMap<SymbolId, FlagSet>,
    pub completions: FxHashMap<SymbolId, Completion>,
    /// The span of the `@main` annotation of each method that has one; the universe keeps the
    /// classes of all annotations.
    pub main_annotations: FxHashMap<SymbolId, Span>,
    /// The annotations of the unit's symbols still to be typed, each with its context.
    pub annotation_trees: Vec<(SymbolId, NodeId, Ctx)>,
    /// The typed annotations of the unit's symbols, in order (dotty's `Symbol.annotations`).
    pub annotations: FxHashMap<SymbolId, Vec<TNodeId>>,
    /// The symbol each definition tree defines.
    pub by_tree: FxHashMap<NodeId, SymbolId>,
    /// The definition tree of each member and local of the unit.
    pub def_trees: FxHashMap<SymbolId, NodeId>,
    /// Whether a script class initializes nothing itself (dotty's `NoInits`).
    pub no_inits: FxHashMap<SymbolId, bool>,
    /// The package a package-level `private` definition is private to (dotty's `privateWithin`).
    pub private_within: FxHashMap<SymbolId, SymbolId>,
    /// The synthetic case-class methods that members of the user replaced, unentered.
    pub retracted: rustc_hash::FxHashSet<SymbolId>,
    /// The script methods whose definitions state their result types.
    pub declared_results: rustc_hash::FxHashSet<SymbolId>,
    /// dotty's `<special-ops>.throw`, once defined.
    pub throw_method: Option<SymbolId>,
    /// dotty's `scala.<repeated>`, the class of repeated arguments, once defined.
    pub repeated_class: Option<SymbolId>,
    /// dotty's `ImplicitScrutineeTypeRef`, the selector type of `summonFrom`, once defined.
    pub implicit_scrutinee: Option<tasty::TypeId>,
    /// The anonymous methods of closures, their parameters and lifted values, their infos made
    /// fully defined once the unit is typed.
    pub closure_syms: Vec<SymbolId>,
    /// The repeated parameters of script methods, typed `Seq[T]`, with their types as written,
    /// `Seq[T] @Repeated`.
    pub repeated_params: FxHashMap<SymbolId, tasty::TypeId>,
    /// The value parameters of each script method, by clause.
    pub param_clauses: FxHashMap<SymbolId, Vec<Vec<SymbolId>>>,
    /// The primary constructor of each script class.
    pub primary_ctors: FxHashMap<SymbolId, SymbolId>,
    /// The self symbol of each script class that names or types its self.
    pub selves: FxHashMap<SymbolId, SymbolId>,
    /// The self definitions whose types are still to be typed, by class, each with its context.
    pub pending_selves: FxHashMap<SymbolId, (NodeId, Ctx)>,
    /// The result type of each script method as its definition states it, before its type
    /// parameters become parameter references of its info.
    pub def_results: FxHashMap<SymbolId, tasty::TypeId>,
    /// The script symbols whose units ended before their infos were complete.
    pub incomplete: rustc_hash::FxHashSet<SymbolId>,
    binders: u32,
}

impl Syms {
    /// Defines script symbol `name` of `kind` with dotty flags `flags` in `owner`.
    pub fn define(
        &mut self,
        u: &mut Universe,
        name: &str,
        kind: Kind,
        owner: SymbolId,
        flags: FlagSet,
    ) -> SymbolId {
        let is_term = !kind.is_type();
        let s = u.define(Symbol {
            name: name.to_owned(),
            kind,
            owner: Some(owner),
            flags: pickled(flags, is_term),
            target: None,
            is_extern: false,
            derived: is_derived(name),
            decls: Vec::new(),
            tree: None,
        });
        u.set_deferred(s, is_term && flags.is(f::DEFERRED));
        self.flags.insert(s, flags);
        s
    }

    /// Gives script symbol `s` dotty flags `flags`.
    pub fn set_flags(&mut self, u: &mut Universe, s: SymbolId, flags: FlagSet) {
        let is_term = !u.is_type_symbol(s);
        u.set_flags(s, pickled(flags, is_term));
        u.set_deferred(s, is_term && flags.is(f::DEFERRED));
        self.flags.insert(s, flags);
    }

    /// Drops what refers to the trees of the unit just typed; its symbols count as completed, those
    /// not completed as incomplete, the infos of the non-classes among them erroneous.
    pub fn end_unit(&mut self, u: &mut Universe) {
        let error = u.intern_type(tasty::Type::Error);
        for (s, c) in self.completions.drain() {
            if !matches!(c, Completion::Done) {
                self.incomplete.insert(s);
                if u.symbol(s).kind != Kind::Class {
                    u.set_info(s, error);
                }
            }
        }
        self.by_tree.clear();
        self.pending_selves.clear();
        self.def_trees.clear();
        self.closure_syms.clear();
        self.annotation_trees.clear();
        self.annotations.clear();
    }

    /// Whether `s` is defined by the script.
    pub fn is_script(&self, s: SymbolId) -> bool {
        self.flags.contains_key(&s)
    }

    /// A binder no other method or type lambda has, of file `SCRIPT_BINDERS`.
    pub fn fresh_binder(&mut self) -> Binder {
        self.binders += 1;
        Binder(SCRIPT_BINDERS, self.binders)
    }
}

/// Whether script name `name` is one desugaring derives: a default getter's, or a fresh `$1$`
/// or `_$1`.
fn is_derived(name: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let unique = name.strip_prefix('$').and_then(|n| n.strip_suffix('$')).is_some_and(digits);
    name.contains("$default$") || unique || name.strip_prefix("_$").is_some_and(digits)
}

/// The TASTy modifiers `TreePickler.pickleFlags` writes for dotty flags `flags`.
pub fn pickled(flags: FlagSet, is_term: bool) -> Flags {
    let common = [
        (f::PRIVATE, t::PRIVATE),
        (f::PROTECTED, t::PROTECTED),
        (f::CASE, t::CASE),
        (f::OVERRIDE, t::OVERRIDE),
        (f::INLINE, t::INLINE),
        (f::MACRO, t::MACRO),
        (f::JAVA_STATIC, t::STATIC),
        (f::MODULE, t::OBJECT),
        (f::ENUM, t::ENUM),
        (f::LOCAL, t::LOCAL),
        (f::SYNTHETIC, t::SYNTHETIC),
        (f::ARTIFACT, t::ARTIFACT),
        (f::TRANSPARENT, t::TRANSPARENT),
        (f::INFIX, t::INFIX),
        (f::INVISIBLE, t::INVISIBLE),
        (f::ERASED, t::ERASED),
        (f::EXPORTED, t::EXPORTED),
        (f::GIVEN, t::GIVEN),
        (f::IMPLICIT, t::IMPLICIT),
        (f::TRACKED, t::TRACKED),
    ];
    let terms = [
        (f::MUTABLE, t::MUTABLE),
        (f::ACCESSOR, t::FIELDaccessor),
        (f::CASE_ACCESSOR, t::CASEaccessor),
        (f::HAS_DEFAULT, t::HASDEFAULT),
        (f::EXTENSION_METHOD, t::EXTENSION),
        (f::PARAM_ACCESSOR, t::PARAMsetter),
        (f::SUPER_PARAM_ALIAS, t::PARAMalias),
    ];
    let types = [
        (f::SEALED, t::SEALED),
        (f::ABSTRACT, t::ABSTRACT),
        (f::TRAIT, t::TRAIT),
        (f::COVARIANT, t::COVARIANT),
        (f::CONTRAVARIANT, t::CONTRAVARIANT),
        (f::OPAQUE, t::OPAQUE),
        (f::OPEN, t::OPEN),
        (f::INTO, t::INTO),
    ];
    let flags = if is_term { flags.to_term_flags() } else { flags.to_type_flags() };
    let mut out = Flags::default();
    let kind_specific: &[(FlagSet, u8)] = if is_term { &terms } else { &types };
    for &(flag, tag) in common.iter().chain(kind_specific) {
        if flags.is(flag) {
            out = out.with(tag);
        }
    }
    let not_module = !flags.is(f::MODULE);
    if flags.is(f::FINAL) && not_module {
        out = out.with(t::FINAL);
    }
    if is_term {
        if flags.is(f::LAZY) && not_module {
            out = out.with(t::LAZY);
        }
        if flags.is(f::ABS_OVERRIDE) {
            out = out.with(t::ABSTRACT).with(t::OVERRIDE);
        }
        if flags.is_all_of(f::METHOD.or(f::STABLE_REALIZABLE)) {
            out = out.with(t::STABLE);
        }
    }
    out
}
