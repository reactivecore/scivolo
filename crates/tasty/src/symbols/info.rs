//! The declared types of symbols, decoded from their trees on first use, and the trees of their
//! right-hand sides.

use super::{FileId, Kind, SymbolId, Universe};
use crate::tags::{OPAQUE, PRIVATEqualified, PROTECTEDqualified, TEMPLATE};
use crate::types::{Decoder, Type, TypeId};
use crate::{NodeId, View, view};

impl Universe {
    /// The declared type of `sym`: a method's type, a value's type, a type member's alias or
    /// bounds, a type parameter's bounds; `Other(TEMPLATE)` for classes and packages.
    pub fn info(&mut self, sym: SymbolId) -> TypeId {
        if let Some(&t) = self.infos.get(&sym) {
            return t;
        }
        let (kind, tree) = (self.symbol(sym).kind, self.symbol(sym).tree);
        let t = match (kind, tree) {
            (Kind::Package | Kind::Class, _) | (_, None) => self.types.intern(Type::Other(TEMPLATE)),
            (_, Some((file, id))) => {
                let t = self.with_decoder(file, |d| d.definition_type(id));
                self.with_java_varargs(sym, t)
            }
        };
        self.infos.insert(sym, t);
        t
    }

    /// The declared type of `sym` if it is set or decoded already.
    pub fn known_info(&self, sym: SymbolId) -> Option<TypeId> {
        self.infos.get(&sym).copied()
    }

    /// The right-hand side of method or value `sym`: its file and tree, for `inline` methods
    /// the body to expand; `None` for abstract members and symbols without TASTy.
    pub fn body(&self, sym: SymbolId) -> Option<(FileId, NodeId)> {
        let (file, id) = self.symbol(sym).tree?;
        let (ast, _) = self.file(file);
        let rhs = match view(ast, id) {
            View::DefDef { parts, .. } => parts.body.get(1).copied(),
            View::ValDef { rhs, .. } => rhs,
            _ => None,
        };
        rhs.map(|r| (file, r))
    }

    /// The lower and upper bound of opaque type `sym`, when its definition states them.
    pub fn opaque_bounds(&mut self, sym: SymbolId) -> Option<(TypeId, TypeId)> {
        let (file, id) = self.symbol(sym).tree?;
        if !self.symbol(sym).flags.has(OPAQUE) {
            return None;
        }
        self.with_decoder(file, |d| d.opaque_bounds(id))
    }

    /// The upper bound of `t`, an opaque type lambda applied to arguments, when its definition
    /// states bounds.
    pub fn applied_opaque_upper(&mut self, t: TypeId) -> Option<TypeId> {
        let Type::Applied(tycon, args) = self.types().get(t).clone() else { return None };
        let (prefix, s) = match *self.types().get(tycon) {
            Type::Sym(p, s) => (p, s),
            _ => (None, self.referenced(tycon)?),
        };
        let (_, hi) = self.opaque_bounds(s)?;
        let hi = match (prefix, self.symbol(s).owner) {
            (Some(p), Some(o)) if self.symbol(o).kind == Kind::Class => self.as_seen_from(hi, p, o),
            _ => hi,
        };
        let Type::Lambda { binder, params, result } = self.types().get(hi).clone() else { return None };
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        Some(self.subst(result, binder, &names, &args))
    }

    /// The class or package `q` of `sym`'s `private[q]` or `protected[q]` (dotty's
    /// `privateWithin`).
    pub fn access_qualifier(&mut self, sym: SymbolId) -> Option<SymbolId> {
        let (file, id) = self.symbol(sym).tree?;
        let (ast, _) = self.file(file);
        let mods = match view(ast, id) {
            View::TypeDef { mods, .. } | View::ValDef { mods, .. } | View::Param { mods, .. } => mods,
            View::DefDef { parts, .. } => parts.mods,
            _ => return None,
        };
        let m = *mods.iter().find(|&&m| matches!(ast.tag(m), PRIVATEqualified | PROTECTEDqualified))?;
        let q = *ast.kids(m).first()?;
        let t = self.type_at(file, q);
        match *self.types.get(t) {
            Type::Package(p) => Some(p),
            _ => self.class_symbol(t),
        }
    }

    /// The type of the type, type tree or path at `id` of loaded file `file`.
    pub fn type_at(&mut self, file: FileId, id: NodeId) -> TypeId {
        self.with_decoder(file, |d| d.ty(id))
    }

    fn with_decoder<R>(&mut self, file: FileId, f: impl FnOnce(&mut Decoder) -> R) -> R {
        let Universe { files, by_tree, packages, types, .. } = self;
        let loaded = files[file.0 as usize].decoded();
        let mut d = Decoder::new((file, &loaded.ast, &loaded.names), by_tree, packages, types);
        f(&mut d)
    }
}
