// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Definitions (dotty's `pickleDef`, `pickleParams`, `pickleModifiers`, `pickleFlags`) and the
//! references to them by address (`registerDef`, `pickleSymRef`).

use super::names::local_name;
use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::tpd::{Clauses, TNodeId, Tree as T};
use tasty::tags::*;
use tasty::{Flags, SymbolId};

/// The modifiers of every symbol in the order dotty writes them, then those of terms only, then
/// those of types only.
const COMMON: [u8; 22] = [
    PRIVATE,
    PROTECTED,
    FINAL,
    CASE,
    OVERRIDE,
    INLINE,
    INLINEPROXY,
    MACRO,
    STATIC,
    OBJECT,
    ENUM,
    LOCAL,
    SYNTHETIC,
    ARTIFACT,
    TRANSPARENT,
    INFIX,
    INVISIBLE,
    ERASED,
    EXPORTED,
    GIVEN,
    IMPLICIT,
    TRACKED,
];
const TERM: [u8; 9] =
    [LAZY, MUTABLE, FIELDaccessor, CASEaccessor, HASDEFAULT, STABLE, EXTENSION, PARAMsetter, PARAMalias];
const TYPE: [u8; 8] = [SEALED, ABSTRACT, TRAIT, COVARIANT, CONTRAVARIANT, OPAQUE, OPEN, INTO];

impl TreePickler<'_> {
    /// Records the symbols that definitions among `stats` define, before trees refer to them
    /// (dotty's `preRegister`).
    pub fn pre_register(&mut self, stats: &[TNodeId]) {
        for &s in stats {
            let defines = matches!(self.tpd.tree(s), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. });
            if let Some(sym) = self.tpd.sym(s).filter(|_| defines) {
                self.defs.entry(sym).or_insert(None);
            }
        }
    }

    /// Whether `s` is defined in the quote.
    pub fn is_defined_here(&self, s: SymbolId) -> bool {
        self.defs.contains_key(&s)
    }

    /// Sets the address of `s` to the current one, filling the references written before.
    pub fn register_def(&mut self, s: SymbolId) {
        let at = self.buf.buf.current_addr();
        self.defs.insert(s, Some(at));
        for r in self.forward.remove(&s).unwrap_or_default() {
            self.buf.fill_ref(r, at, false);
        }
    }

    /// The address of definition `s`, filled in later if it is not pickled yet.
    pub fn sym_addr(&mut self, s: SymbolId) {
        if self.keying {
            return self.buf.buf.write_bytes(&s.0.to_le_bytes());
        }
        match self.defs.get(&s) {
            Some(&Some(at)) => self.buf.write_ref(at),
            _ => {
                let r = self.buf.reserve_ref(false);
                self.forward.entry(s).or_default().push(r);
            }
        }
    }

    /// A `ValDef`, `DefDef` or `TypeDef` tree `id`.
    pub fn definition(&mut self, id: TNodeId) -> Result<(), Refusal> {
        let sym = self.tpd.sym(id).ok_or("a definition without a symbol")?;
        match self.tpd.tree(id).clone() {
            T::ValDef { tpt, rhs, .. } => self.def(VALDEF, sym, None, tpt, rhs),
            T::DefDef { paramss, tpt, rhs, .. } => self.def(DEFDEF, sym, Some(paramss), tpt, rhs),
            T::TypeDef { rhs, .. } => self.def(TYPEDEF, sym, None, rhs, None),
            _ => Err("a definition of another kind"),
        }
    }

    fn def(
        &mut self,
        tag: u8,
        sym: SymbolId,
        paramss: Option<Clauses>,
        tpt: TNodeId,
        rhs: Option<TNodeId>,
    ) -> Result<(), Refusal> {
        self.defs.entry(sym).or_insert(None);
        self.register_def(sym);
        if tag == VALDEF && self.u.symbol(sym).flags.has(OBJECT) {
            self.share_module_class(tpt, rhs);
        }
        self.with_length(tag, &mut |p| {
            p.name(&local_name(p.u, sym));
            if let Some(paramss) = paramss {
                p.paramss(sym, paramss)?;
            }
            p.tree(tpt)?;
            if let Some(rhs) = rhs {
                p.tree(rhs)?;
            }
            p.modifiers(sym)
        })
    }

    /// The parameter clauses `paramss` of a method (dotty's `pickleParamss`).
    fn paramss(&mut self, method: SymbolId, paramss: Clauses) -> Result<(), Refusal> {
        let clauses: Vec<Vec<TNodeId>> =
            self.tpd.clause_items(paramss).iter().map(|&c| self.tpd.items(c).to_vec()).collect();
        self.method_params(method, &clauses);
        let is_type = |p: &Self, c: &[TNodeId]| matches!(p.tpd.tree(c[0]), T::TypeDef { .. });
        for (i, clause) in clauses.iter().enumerate() {
            if clause.is_empty() {
                self.byte(EMPTYCLAUSE);
                continue;
            }
            self.params(clause)?;
            let next = clauses.get(i + 1).filter(|n| !n.is_empty());
            if next.is_some_and(|n| is_type(self, clause) == is_type(self, n)) {
                self.byte(SPLITCLAUSE);
            }
        }
        Ok(())
    }

    /// Parameters `params` (dotty's `pickleParams`).
    pub fn params(&mut self, params: &[TNodeId]) -> Result<(), Refusal> {
        self.pre_register(params);
        for &param in params {
            self.buf.register_tree(param.index() as u32);
            let sym = self.tpd.sym(param).ok_or("a parameter without a symbol")?;
            match self.tpd.tree(param).clone() {
                T::ValDef { tpt, .. } => {
                    self.param_tpts.insert(sym, tpt);
                    self.def(PARAM, sym, None, tpt, None)?
                }
                T::TypeDef { rhs, .. } => self.def(TYPEPARAM, sym, None, rhs, None)?,
                _ => return Err("a parameter of another kind"),
            }
        }
        Ok(())
    }

    /// The modifiers of `sym` (dotty's `pickleModifiers`), without annotations.
    fn modifiers(&mut self, sym: SymbolId) -> Result<(), Refusal> {
        let s = self.u.symbol(sym);
        let (flags, term, setter) = (s.flags, s.kind.is_term(), s.name.ends_with("_="));
        if flags.has(PRIVATEqualified) || flags.has(PROTECTEDqualified) {
            return Err("a qualified private or protected definition");
        }
        let trees = self.annotation_trees(sym);
        let classes: Vec<SymbolId> = match self.file {
            Some(_) => trees.iter().filter_map(|&t| self.u.class_symbol(self.tpd.tpe(t))).collect(),
            None => self.u.annotations(sym).to_vec(),
        };
        if classes.len() != trees.len() {
            return Err("an annotated definition");
        }
        let flags = self.access_qualifier(sym, flags)?;
        self.flags(if term && !setter { flags.without(PARAMsetter) } else { flags }, term);
        for (class, annot) in classes.into_iter().zip(trees) {
            self.param_annotation_anew(sym, annot);
            let class = type_ref(self.u, class);
            self.with_length(ANNOTATION, &mut |p| {
                p.ty(class, false)?;
                p.tree(annot)
            })?;
        }
        Ok(())
    }

    /// Modifiers `flags` of a term if `term`, else of a type (dotty's `pickleFlags`).
    pub fn flags(&mut self, flags: Flags, term: bool) {
        let own = if term { &TERM[..] } else { &TYPE[..] };
        for &tag in COMMON.iter().chain(own) {
            if tag == MUTABLE && term && flags.has(ABSTRACT) {
                self.byte(ABSTRACT);
                self.byte(OVERRIDE);
            }
            if flags.has(tag) && !(tag == OVERRIDE && term && flags.has(ABSTRACT)) {
                self.byte(tag);
            }
        }
    }
}
