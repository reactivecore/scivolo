//! Selections of a member the qualifier's type lacks (dotty's `NotAMember`), reported where
//! dotty finds no member, conversion or extension either and adds no addendum.

use super::Typer;
use super::not_found::{path_to, uncertain_edge};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::tree::NumberKind;
use syntax::ast::{Attrs, NodeId, TName, Tree};
use tasty::tags::{OBJECT, OPAQUE};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// The classes whose members dotty's selection finds otherwise or tries further.
const SPECIAL_SITES: [&str; 5] =
    ["scala.Dynamic", "scala.Selectable", "scala.Tuple", "scala.Nothing", "scala.Null"];

impl Typer<'_> {
    /// `NotAMember` for selection `id` of `name` from term `qual` in `ctx`, whose type has no
    /// such member, an error tree; `None` where dotty may still find, convert or
    /// extend it, words the message otherwise, or types trees that typing skips after the error.
    pub fn not_a_member(&mut self, id: NodeId, qual: TNodeId, name: TName, ctx: Ctx) -> Option<TNodeId> {
        let this = matches!(self.tpd.tree(qual), T::This { .. });
        let type_qual = !this && self.tpd.sym(qual).is_some_and(|q| self.u.is_type_symbol(q))
            || matches!(
                self.tpd.tree(qual),
                T::TypeTree { .. }
                    | T::SingletonTypeTree { .. }
                    | T::AppliedTypeTree { .. }
                    | T::ByNameTypeTree { .. }
            );
        if type_qual {
            return None;
        }
        let qual_tpe = self.tpd.tpe(qual);
        let qual_tpe = self.widen_if_unstable(qual_tpe);
        let text = self.text(name.name).to_owned();
        let plain = text.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && text.chars().any(|c| c != '_');
        if !plain
            || matches!(&*text, "values" | "valueOf")
            || !matches!(self.ast.tree(id), Tree::Select { .. })
            || self.ast.attrs(id).contains(Attrs::MULTI_LINE_INFIX)
            || ctx.mode.is(Mode::PATTERN)
            || self.u.symbol(ctx.owner).name == "<init>"
            || !self.certain_path(id, ctx)
        {
            return None;
        }
        let (class, site) = self.member_site(qual_tpe)?;
        if !name.is_type && self.may_supply((class, site), &text, ctx) {
            return None;
        }
        let path = self.zonk(qual_tpe);
        let hint = self.member_hint(class, path, &text, name.is_type, ctx)?;
        let shown = match self.u.symbol(class).flags.has(OBJECT) {
            true => format!("object {}", self.u.show_user(site).strip_suffix(".type")?),
            false => self.u.show_user(site),
        };
        Some(self.error(id, super::errors::not_a_member(&text, name.is_type, &shown, &hint)))
    }

    /// Whether the trees from the outermost definition enclosing `id` down to it are typed as
    /// dotty types them, function parts of applications to literals included.
    fn certain_path(&mut self, id: NodeId, ctx: Ctx) -> bool {
        let Some(root) = self.enclosing_definition(id, ctx) else { return false };
        let mut path = Vec::new();
        if !path_to(self.ast, root, id, &mut path) {
            return false;
        }
        path.windows(2).all(|w| match *self.ast.tree(w[0]) {
            Tree::Apply { fun, args } if fun == w[1] => {
                self.ast.items(args).iter().all(|&a| self.is_literal(a))
            }
            _ => !uncertain_edge(self.ast, w),
        })
    }

    /// Whether `t` is a literal, a decimal number one of at most nine digits.
    fn is_literal(&self, t: NodeId) -> bool {
        match self.ast.tree(t) {
            Tree::Literal { .. } => true,
            Tree::Number { digits, kind: NumberKind::Whole(10) } => {
                digits.len() <= 9 && digits.bytes().all(|b| b.is_ascii_digit())
            }
            _ => false,
        }
    }

    /// The class of the members of `qual_tpe` and its widened type, if a class type whose members
    /// dotty finds as the universe does and whose type arguments are concrete.
    fn member_site(&mut self, qual_tpe: TypeId) -> Option<(SymbolId, TypeId)> {
        let t = self.zonk(qual_tpe);
        let site = self.u.widen(t);
        let dealiased = self.u.dealias(site);
        let class = self.direct_class(dealiased)?;
        if self.is_named_package_object(class) {
            return None;
        }
        for special in SPECIAL_SITES {
            if self.resolve_full_name(special, true).is_some_and(|s| self.u.derives_from(class, s)) {
                return None;
            }
        }
        let mut parts = Vec::new();
        self.u.map_type(site, &mut |_, part| {
            parts.push(part);
            None
        });
        let shown = |t: &mut Self, part| is_concrete_part(t.u, part) && t.shown_as_dotty(part);
        parts.into_iter().all(|part| shown(self, part)).then_some((class, site))
    }

    /// Whether `show_user` shows type reference `part` as dotty's error messages do: by its name
    /// alone, or through objects from the root package.
    pub fn shown_as_dotty(&mut self, part: TypeId) -> bool {
        let sym = match self.u.types().get(part).clone() {
            Type::Sym(_, s) => s,
            Type::TypeRef(p, n) => match self.u.resolve(p, &n, true) {
                Some(s) => s,
                None => return false,
            },
            _ => return true,
        };
        if !matches!(self.u.symbol(sym).kind, Kind::Class | Kind::Type) {
            return true;
        }
        let text = self.u.show_user(part);
        if !text.strip_suffix(".type").unwrap_or(&text).contains('.') {
            return true;
        }
        let mut owner = self.u.symbol(sym).owner;
        while let Some(o) = owner {
            let s = self.u.symbol(o);
            match s.kind {
                Kind::Package => return o == self.u.root(),
                Kind::Class if s.flags.has(OBJECT) => owner = s.owner,
                _ => return false,
            }
        }
        false
    }
}

/// Whether type `part` is a class, an alias, a package, an object, `this` or a constant, applied
/// or not.
pub fn is_concrete_part(u: &mut Universe, part: TypeId) -> bool {
    let sym = match u.types().get(part).clone() {
        Type::Sym(_, s) => s,
        Type::TypeRef(p, n) => match u.resolve(p, &n, true) {
            Some(s) => s,
            None => return false,
        },
        Type::TermRef(p, n) => match u.resolve(p, &n, false) {
            Some(s) if matches!(u.symbol(s).kind, Kind::Package | Kind::Val) => s,
            _ => return false,
        },
        Type::Applied(..) | Type::Package(_) | Type::This(_) | Type::NoPrefix => return true,
        Type::Constant(..) | Type::Str(_) => return true,
        _ => return false,
    };
    match u.symbol(sym).kind {
        Kind::Class | Kind::Package => true,
        Kind::Val => u.symbol(sym).flags.has(OBJECT),
        Kind::Type => {
            let info = u.info(sym);
            !u.symbol(sym).flags.has(OPAQUE) && !matches!(u.types().get(info), Type::Bounds(..))
        }
        _ => false,
    }
}
