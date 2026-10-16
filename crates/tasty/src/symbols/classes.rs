//! Classes' parents, linearization and inherited members.

use super::{FileId, Kind, SymbolId, Universe};
use crate::tags::{
    APPLY, BLOCK, DEFDEF, EMPTYCLAUSE, NEW, PARAM, PRIVATE, SELECT, SELECTin, SELFDEF, SPLITCLAUSE,
    TYPEAPPLY, TYPEDEF, TYPEPARAM, VALDEF,
};
use crate::types::{Decoder, Type, TypeId};
use crate::{Ast, NodeId};
use rustc_hash::FxHashSet;

/// How many aliases and bounds `class_symbol` follows.
const MAX_ALIASES: u32 = 64;

impl Universe {
    /// The parent types of class `c`, in declaration order.
    pub fn parents(&mut self, c: SymbolId) -> Vec<TypeId> {
        if let Some(p) = self.parents.get(&c) {
            return p.clone();
        }
        let parents = match self.symbol(c).tree {
            Some((file, id)) if self.symbol(c).kind == Kind::Class => self.decode_parents(file, id),
            _ => Vec::new(),
        };
        self.parents.insert(c, parents.clone());
        parents
    }

    fn decode_parents(&mut self, file: FileId, typedef: NodeId) -> Vec<TypeId> {
        let Universe { files, by_tree, packages, types, .. } = self;
        let loaded = files[file.0 as usize].decoded();
        let ast = &loaded.ast;
        let Some(&template) = ast.kids(typedef).first() else { return Vec::new() };
        let mut d = Decoder::new((file, ast, &loaded.names), by_tree, packages, types);
        let parents: Vec<TypeId> = ast
            .kids(template)
            .iter()
            .copied()
            .skip_while(|&k| matches!(ast.tag(k), TYPEPARAM | PARAM | EMPTYCLAUSE))
            .take_while(|&k| !matches!(ast.tag(k), SELFDEF | SPLITCLAUSE | DEFDEF | VALDEF | TYPEDEF))
            .map(|parent| parent_type(&mut d, ast, parent))
            .collect();
        parents.into_iter().map(|p| self.beta_reduced(p)).collect()
    }

    /// `t`, or the body of type lambda `l` with its parameters replaced if `t` is `l[args]`.
    pub fn beta_reduced(&mut self, t: TypeId) -> TypeId {
        let Type::Applied(tycon, args) = self.types().get(t).clone() else { return t };
        let Type::Lambda { binder, params, result } = self.types().get(tycon).clone() else { return t };
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        self.subst(result, binder, &names, &args)
    }

    /// Class `c` and its base classes in linearization order, `c` first.
    pub fn linearization(&mut self, c: SymbolId) -> Vec<SymbolId> {
        self.linearized(c).to_vec()
    }

    /// The linearization of class `c`, computed on first use.
    fn linearized(&mut self, c: SymbolId) -> &[SymbolId] {
        if !self.linearizations.contains_key(&c) {
            self.linearizations.insert(c, Vec::new());
            let mut lin: Vec<SymbolId> = Vec::new();
            let mut seen: FxHashSet<SymbolId> = FxHashSet::default();
            for parent in self.parents(c) {
                let Some(p) = self.class_symbol(parent) else { continue };
                let plin: Vec<SymbolId> =
                    self.linearized(p).iter().copied().filter(|b| !seen.contains(b)).collect();
                seen.extend(&plin);
                lin.splice(0..0, plin);
            }
            lin.insert(0, c);
            self.linearizations.insert(c, lin);
        }
        &self.linearizations[&c]
    }

    /// Whether class `c` is `base` or derives from it.
    pub fn derives_from(&mut self, c: SymbolId, base: SymbolId) -> bool {
        c == base || self.linearized(c).contains(&base)
    }

    /// The members of class `c` named `name` that `keep` accepts: those of the first class in
    /// its linearization that declares any, private ones of base classes left out.
    pub fn member_where(
        &mut self,
        c: SymbolId,
        name: &str,
        keep: impl Fn(&Self, SymbolId) -> bool,
    ) -> Vec<SymbolId> {
        let mut i = 0;
        while let Some(&base) = self.linearized(c).get(i) {
            i += 1;
            let found: Vec<SymbolId> = self
                .decl(base, name)
                .unwrap_or_default()
                .into_iter()
                .filter(|&m| (base == c || !self.symbol(m).flags.has(PRIVATE)) && keep(self, m))
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    /// The members of class `c` named `name`.
    pub fn member(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        self.member_where(c, name, |_, _| true)
    }

    /// The type parameters of class or method `c`, in order.
    pub fn type_params(&self, c: SymbolId) -> Vec<SymbolId> {
        let decls = &self.symbol(c).decls;
        decls.iter().copied().filter(|&d| self.symbol(d).kind == Kind::TypeParam).collect()
    }

    /// The class type parameter reference `t` names: its class and index.
    pub fn class_param(&mut self, t: TypeId) -> Option<(SymbolId, usize)> {
        let s = match self.types().get(t).clone() {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.resolve_ref(t)?,
            _ => return None,
        };
        let owner = self.symbol(s).owner?;
        if self.symbol(s).kind != Kind::TypeParam || self.symbol(owner).kind != Kind::Class {
            return None;
        }
        Some((owner, self.type_params(owner).iter().position(|&p| p == s)?))
    }

    /// The class type `t` refers to, through aliases and upper bounds, if the universe has it.
    pub fn class_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        self.class_symbol_within(t, MAX_ALIASES)
    }

    fn class_symbol_within(&mut self, t: TypeId, depth: u32) -> Option<SymbolId> {
        let depth = depth.checked_sub(1)?;
        let s = match self.types().get(t).clone() {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.resolve_ref(t)?,
            Type::Applied(inner, _)
            | Type::Annotated(inner, _)
            | Type::Alias(inner)
            | Type::Bounds(_, inner)
            | Type::Lambda { result: inner, .. } => return self.class_symbol_within(inner, depth),
            _ => return None,
        };
        match self.symbol(s).kind {
            Kind::Class => Some(s),
            Kind::Type | Kind::TypeParam => {
                let info = self.info(s);
                self.class_symbol_within(info, depth)
            }
            _ => None,
        }
    }
}

/// The type of a parent in a template: a type tree, or the class of a constructor call such as
/// `new C[T](x)`, whose `new` or else whose type application carries the type arguments.
fn parent_type(d: &mut Decoder, ast: &Ast, id: NodeId) -> TypeId {
    let id = ast.deref(id);
    let k = ast.kids(id);
    match ast.tag(id) {
        TYPEAPPLY if k.len() > 1 => {
            let fun = parent_type(d, ast, k[0]);
            if matches!(d.get(fun), Type::Applied(..)) {
                return fun;
            }
            let args = k[1..].iter().map(|&a| d.ty(a)).collect();
            d.intern(Type::Applied(fun, args))
        }
        APPLY | TYPEAPPLY | SELECT | SELECTin | BLOCK if !k.is_empty() => parent_type(d, ast, k[0]),
        NEW if !k.is_empty() => d.ty(k[0]),
        _ => d.ty(id),
    }
}
