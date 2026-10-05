//! The typer's side of script symbols: their dotty flags, what completes their infos, their
//! annotations; and the TASTy modifiers the universe keeps for them.

use crate::ctx::Ctx;
use rustc_hash::FxHashMap;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{NodeId, Span};
use tasty::tags as t;
use tasty::{Binder, FileId, Flags, Kind, Symbol, SymbolId, Universe};

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

#[derive(Default)]
pub struct Syms {
    pub flags: FxHashMap<SymbolId, FlagSet>,
    pub completions: FxHashMap<SymbolId, Completion>,
    /// The classes of the annotations of a symbol, with the annotations' spans.
    pub annotations: FxHashMap<SymbolId, Vec<(SymbolId, Span)>>,
    /// The symbol each definition tree defines.
    pub by_tree: FxHashMap<NodeId, SymbolId>,
    /// Whether a script class initializes nothing itself (dotty's `NoInits`).
    pub no_inits: FxHashMap<SymbolId, bool>,
    /// The package a package-level `private` definition is private to (dotty's `privateWithin`).
    pub private_within: FxHashMap<SymbolId, SymbolId>,
    /// The script methods whose definitions state their result types.
    pub declared_results: rustc_hash::FxHashSet<SymbolId>,
    /// dotty's `<special-ops>.throw`, once defined.
    pub throw_method: Option<SymbolId>,
    /// dotty's `scala.<repeated>`, the class of repeated arguments, once defined.
    pub repeated_class: Option<SymbolId>,
    /// The anonymous methods of closures and their parameters, their infos made fully defined
    /// once the unit is typed.
    pub closure_syms: Vec<SymbolId>,
    /// The repeated parameters of script methods, typed `Seq[T]`, with their types as written,
    /// `Seq[T] @Repeated`.
    pub repeated_params: FxHashMap<SymbolId, tasty::TypeId>,
    /// The value parameters of each script method, by clause.
    pub param_clauses: FxHashMap<SymbolId, Vec<Vec<SymbolId>>>,
    /// The primary constructor of each script class.
    pub primary_ctors: FxHashMap<SymbolId, SymbolId>,
    /// The result type of each script method as its definition states it, before its type
    /// parameters become parameter references of its info.
    pub def_results: FxHashMap<SymbolId, tasty::TypeId>,
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
        let is_term = !matches!(kind, Kind::Class | Kind::Type | Kind::TypeParam);
        let s = u.define(Symbol {
            name: name.to_owned(),
            kind,
            owner: Some(owner),
            flags: pickled(flags, is_term),
            target: None,
            is_extern: false,
            decls: Vec::new(),
            tree: None,
        });
        self.flags.insert(s, flags);
        s
    }

    /// Gives script symbol `s` dotty flags `flags`.
    pub fn set_flags(&mut self, u: &mut Universe, s: SymbolId, flags: FlagSet) {
        let is_term = !is_type_symbol(u, s);
        u.set_flags(s, pickled(flags, is_term));
        self.flags.insert(s, flags);
    }

    /// A binder no other method or type lambda has.
    pub fn fresh_binder(&mut self) -> Binder {
        self.binders += 1;
        Binder(FileId(u32::MAX), self.binders)
    }
}

pub fn is_type_symbol(u: &Universe, s: SymbolId) -> bool {
    matches!(u.symbol(s).kind, Kind::Class | Kind::Type | Kind::TypeParam)
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
