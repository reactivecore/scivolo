// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/interactive/Completion.scala
//! The members a selection offers (dotty's `directMemberCompletions`: `accessibleMembers` with
//! `completionsFilter` and `isValidCompletionSymbol` in term mode).

use crate::ctx::Ctx;
use crate::typer::Typer;
use crate::typer::did_you_mean::{OBJECT_MEMBERS, is_jdk};
use tasty::tags::{
    ARTIFACT, FIELDaccessor, LOCAL, MUTABLE, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified,
};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The names starting with `prefix` of the members of `qual` that a selection in `ctx`
    /// offers; `None` where the universe may not know them all.
    pub fn member_completions(&mut self, qual: TypeId, prefix: &str, ctx: Ctx) -> Option<Vec<String>> {
        let this = matches!(self.u.types().get(qual), Type::This(_));
        let site = self.widen_if_unstable(qual);
        let owners = self.member_owners(site)?;
        let mut names: Vec<String> = Vec::new();
        for &owner in &owners {
            for d in self.u.decls(owner).ok()? {
                let name = &self.u.symbol(d).name;
                if name.starts_with(prefix) && !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names.retain(|name| self.offers(&owners, name, this, ctx));
        Some(names)
    }

    /// The package or the base classes whose declarations are the members of `site`; `None` for
    /// the root package, whose members dotty takes from its classpath and the JDK.
    fn member_owners(&mut self, site: TypeId) -> Option<Vec<SymbolId>> {
        let package = match *self.u.types().get(site) {
            Type::Package(p) => Some(p),
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Package => Some(s),
            _ => None,
        };
        if let Some(p) = package {
            if p == self.u.root() || is_jdk(&self.u.full_name(p)) {
                return None;
            }
            let decls = self.u.decls(p).ok()?;
            let objects = decls.into_iter().filter(|&d| self.is_named_package_object(d));
            return Some(std::iter::once(p).chain(objects).collect());
        }
        let widened = self.u.widen(site);
        let t = self.u.dealias(widened);
        let class = match *self.u.types().get(t) {
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class => Some(s),
            Type::Applied(..) | Type::This(_) | Type::TypeRef(..) => self.u.class_symbol(t),
            _ => None,
        }?;
        if matches!(&*self.u.full_name(class), "scala.Nothing" | "scala.Null") {
            return None;
        }
        let mut bases = self.known_bases(class)?;
        if self.syms.flags.contains_key(&class) {
            bases.retain(|&b| !self.u.full_name(b).starts_with("scala.deriving.Mirror"));
        }
        Some(bases)
    }

    /// Whether a selection whose members `owners` declare, the first the site's class or package,
    /// offers `name` from where `ctx` is, the prefix being a `this` if `this`: a member term as
    /// dotty's typer sees it, or a constructor proxy.
    fn offers(&mut self, owners: &[SymbolId], name: &str, this: bool, ctx: Ctx) -> bool {
        let mut terms = Vec::new();
        for &owner in owners {
            let decls = self.u.decl(owner, name).unwrap_or_default();
            let inherited = |s: SymbolId| owner == owners[0] || !self.u.symbol(s).flags.has(PRIVATE);
            let decls = decls.into_iter().filter(|&d| !self.u.is_type_symbol(d) && inherited(d));
            terms.extend(decls.filter(|&d| !self.is_lowered(d)));
        }
        if terms.iter().any(|&s| self.is_offered(s, this, ctx)) {
            return true;
        }
        terms.is_empty() && owners.iter().any(|&o| self.has_proxy(o, name, this, ctx))
    }

    /// Whether `s` is a member a phase after the typer gave a script class.
    fn is_lowered(&self, s: SymbolId) -> bool {
        let owner = self.u.symbol(s).owner;
        owner.is_some_and(|o| self.syms.flags.contains_key(&o)) && !self.syms.flags.contains_key(&s)
    }

    /// Whether term `s` is offered (dotty's `isValidCompletionSymbol` and `isAccessibleFrom`):
    /// not `Object` members the JDK lacks, nor Scala Native's packages.
    fn is_offered(&mut self, s: SymbolId, this: bool, ctx: Ctx) -> bool {
        let sym = self.u.symbol(s);
        let flags = sym.flags;
        let setter = flags.has(MUTABLE) && flags.has(FIELDaccessor) && sym.name.ends_with("_=");
        let hidden =
            sym.owner == Some(self.defn.object) && !OBJECT_MEMBERS.split_whitespace().any(|m| m == sym.name);
        let valid = sym.name != "<init>" && !sym.derived && !setter && !flags.has(ARTIFACT) && !hidden;
        let native = sym.kind == Kind::Package && self.u.full_name(s).starts_with("scala.scalanative");
        let valid = valid && !native;
        let package_object = self.u.module_class(s).is_some_and(|c| self.is_named_package_object(c));
        valid && !package_object && self.is_visible(s, this, ctx)
    }

    /// Whether class `owner` declares a class `name` whose constructor proxy is offered: the
    /// class gets proxies, no base class of `owner` declares a term `name`, and the proxy,
    /// public unless the class is protected or qualified private, is accessible.
    fn has_proxy(&mut self, owner: SymbolId, name: &str, this: bool, ctx: Ctx) -> bool {
        let decls = self.u.decl(owner, name).unwrap_or_default();
        let Some(class) = decls.into_iter().find(|&d| self.u.symbol(d).kind == Kind::Class) else {
            return false;
        };
        let bases = match self.u.symbol(owner).kind {
            Kind::Package => vec![owner],
            _ => self.u.linearization(owner),
        };
        let term_exists = bases.into_iter().any(|b| {
            let decls = self.u.decl(b, name).unwrap_or_default();
            decls.iter().any(|&d| self.u.symbol(d).kind.is_term())
        });
        let flags = self.u.symbol(class).flags;
        let within = flags.has(PRIVATEqualified)
            || flags.has(PROTECTEDqualified)
            || self.syms.private_within.contains_key(&class);
        let protected = flags.has(PROTECTED) && !(this && self.is_accessible(class, ctx));
        let source = !self.is_lowered(class) && !self.u.symbol(class).derived;
        source && self.needs_constructor_proxies(class) && !term_exists && !within && !protected
    }

    /// Whether member `s` is accessible through a prefix that is a `this` if `this`: protected
    /// and `[this]`-qualified members only through `this`.
    fn is_visible(&mut self, s: SymbolId, this: bool, ctx: Ctx) -> bool {
        let flags = self.u.symbol(s).flags;
        let restricted =
            [PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified].iter().any(|&f| flags.has(f))
                || self.syms.private_within.contains_key(&s);
        if !restricted {
            return true;
        }
        let through_this = flags.has(LOCAL) || flags.has(PROTECTED) || flags.has(PROTECTEDqualified);
        (this || !through_this) && self.is_accessible(s, ctx)
    }
}
