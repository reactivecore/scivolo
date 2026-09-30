//! Identifiers: finding what a name refers to in the context chain (dotty's `findRef`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{Kind, SymbolId, Type, TypeId};

/// What a name refers to: its prefix (`None` for a local) and the symbols it denotes, several
/// for an overloaded method.
pub struct Found {
    pub prefix: Option<TypeId>,
    pub syms: Vec<SymbolId>,
}

impl Typer<'_> {
    pub fn is_type_symbol(&self, s: SymbolId) -> bool {
        crate::symbols::is_type_symbol(self.u, s)
    }

    /// Whether `c` is a package, or an object whose owners are all packages or objects.
    pub fn is_static_owner(&self, c: SymbolId) -> bool {
        let s = self.u.symbol(c);
        match s.kind {
            Kind::Package => true,
            Kind::Class if s.flags.has(tasty::tags::OBJECT) => {
                s.owner.is_none_or(|o| self.is_static_owner(o))
            }
            _ => false,
        }
    }

    /// The symbols named `name` visible in `ctx`, innermost level first; a package's members that
    /// this unit does not define rank below the imports outside it.
    pub fn find_ref(&mut self, name: &str, is_type: bool, ctx: Ctx) -> Option<Found> {
        let mut deferred = None;
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            let found = match self.levels.get(l) {
                Level::Scope(entries) => {
                    let syms: Vec<SymbolId> = entries
                        .iter()
                        .rev()
                        .filter(|(n, s)| n == name && self.is_type_symbol(*s) == is_type)
                        .map(|&(_, s)| s)
                        .take(1)
                        .collect();
                    (!syms.is_empty()).then_some(Found { prefix: None, syms })
                }
                &Level::Members(owner) => {
                    let prefix = crate::defn::owner_prefix(self.u, owner);
                    let found = self.members_found(prefix, name, is_type);
                    let in_unit = |f: &Found| f.syms.iter().any(|s| self.syms.flags.contains_key(s));
                    match found {
                        Some(f) if self.u.symbol(owner).kind == tasty::Kind::Package && !in_unit(&f) => {
                            deferred = deferred.or(Some(f));
                            None
                        }
                        found => found,
                    }
                }
                &Level::Wildcard { qual } => self.members_found(qual, name, is_type),
            };
            if found.is_some() {
                return found;
            }
            level = self.levels.outer(l);
        }
        deferred
    }

    fn members_found(&mut self, prefix: TypeId, name: &str, is_type: bool) -> Option<Found> {
        let syms: Vec<SymbolId> = self
            .type_members(prefix, name)
            .into_iter()
            .filter(|&s| self.is_type_symbol(s) == is_type)
            .collect();
        (!syms.is_empty()).then_some(Found { prefix: Some(prefix), syms })
    }

    pub fn typed_ident(&mut self, id: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let text = self.text(name.name).to_owned();
        if ctx.mode.is(Mode::PATTERN) && text == "_" {
            return self.wildcard_pattern(id, name, pt);
        }
        let Some(found) = self.find_ref(&text, name.is_type, ctx) else {
            return self.unsupported(id, "unresolved name");
        };
        let sym = match self.choose(&found.syms, pt) {
            Ok(sym) => sym,
            Err(what) => return self.unsupported(id, what),
        };
        if !self.is_accessible(sym, ctx) {
            return self.unsupported(id, "inaccessible member");
        }
        if let Some(p) = found.prefix
            && let Type::This(c) = self.u.types().get(p).clone()
            && self.u.class_symbol(c).is_some_and(|c| !self.is_static_owner(c))
        {
            return self.unsupported(id, "member of an enclosing class");
        }
        let tpe = self.ref_type(found.prefix, sym);
        let span = self.ast.span(id);
        self.tpd.node(T::Ident { name }, span, Some(tpe), Some(sym))
    }

    /// The type of a reference to `sym` through `prefix` (dotty's `TermRef`/`TypeRef`), a
    /// member of a package object through the object (`makePackageObjPrefixExplicit`).
    pub fn ref_type(&mut self, prefix: Option<TypeId>, sym: SymbolId) -> TypeId {
        if self.u.symbol(sym).kind == Kind::Package {
            return self.u.intern_type(Type::Package(sym));
        }
        let prefix = match prefix.map(|p| self.u.types().get(p).clone()) {
            Some(Type::Package(_)) => self.package_object_prefix(sym).or(prefix),
            Some(Type::This(c)) if self.u.class_symbol(c).is_some_and(|c| self.is_package_object(c)) => {
                self.package_object_prefix(sym).or(prefix)
            }
            _ => prefix,
        };
        self.u.intern_type(Type::Sym(prefix, sym))
    }

    fn is_package_object(&self, c: SymbolId) -> bool {
        let s = self.u.symbol(c);
        s.owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Package)
            && (s.name == "package$" || s.name.ends_with("$package$"))
    }

    /// The reference to the package object that declares `sym`, if one does.
    fn package_object_prefix(&mut self, sym: SymbolId) -> Option<TypeId> {
        let obj_class = self.u.symbol(sym).owner?;
        let pkg = self.u.symbol(obj_class).owner?;
        if self.u.symbol(obj_class).kind != Kind::Class || self.u.symbol(pkg).kind != Kind::Package {
            return None;
        }
        let name = self.u.symbol(obj_class).name.strip_suffix('$')?.to_owned();
        let module =
            self.u.decl(pkg, &name).ok()?.into_iter().find(|&s| self.u.symbol(s).kind == Kind::Val)?;
        Some(crate::defn::type_ref(self.u, module))
    }

    /// The alternative of `syms` that `pt` selects: the only one, or the only method taking as
    /// many arguments as `pt` supplies.
    pub fn choose(&mut self, syms: &[SymbolId], pt: Pt) -> Result<SymbolId, &'static str> {
        if let [s] = syms {
            return Ok(*s);
        }
        let Pt::Args(n) = pt else { return Err("overloaded reference") };
        let fitting: Vec<SymbolId> = syms
            .iter()
            .copied()
            .filter(|&s| {
                self.complete(s);
                let info = self.u.info(s);
                self.u.param_clauses(info).first().is_some_and(|(_, ps)| ps.len() == n)
            })
            .collect();
        match fitting.as_slice() {
            [s] => Ok(*s),
            _ => Err("overload resolution"),
        }
    }
}
