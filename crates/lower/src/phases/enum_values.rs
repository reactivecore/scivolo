//! The members dotty's `SyntheticMembers` gives the class of an enum value, made in
//! `$new(_$ordinal, $name)` or by a case with a body: `readResolve`, `productPrefix`, `toString`,
//! `ordinal`, `hashCode`.

use super::case_members::existing_def;
use super::synthetic::private_method;
use super::walk::subtrees;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{CASE, ENUM, OVERRIDE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Whether `class` is the anonymous class of an enum value.
pub fn is_enum_value(l: &mut Lower, class: SymbolId) -> bool {
    let first = l.u.parents(class).first().and_then(|&p| l.u.class_symbol(p));
    l.u.symbol(class).name.starts_with("$anon") && first.is_some_and(|c| has(l.u, c, ENUM))
}

/// The synthetic members of enum value class `class` in unit `root`, at `span`'s point.
pub fn enum_value_members(l: &mut Lower, root: TNodeId, class: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    if !is_enum_value(l, class) {
        return Ok(Vec::new());
    }
    let owner = l.u.symbol(class).owner.expect("a local class's owner");
    let source = if l.u.symbol(owner).name == "$new" {
        let (Some(name), Some(ordinal)) =
            (param(l, root, owner, "$name"), param(l, root, owner, "_$ordinal"))
        else {
            return unsupported(span, "an enum value constructor without its parameters");
        };
        Source::Params { name, ordinal }
    } else {
        let Some(ordinal) = child_index(l, owner) else {
            return unsupported(span, "an enum value outside its enum's cases");
        };
        Source::Literals { name: l.u.symbol(owner).name.clone(), ordinal }
    };
    let focus = Span::new(span.point(), span.point(), span.point());
    let mut out = vec![read_resolve(l, class, owner, focus)?];
    let cc = l.defs.case_classes;
    for sym in [cc.product_prefix, cc.to_string, cc.enum_ordinal, cc.hash_code] {
        if existing_def(l, class, sym).is_some() {
            continue;
        }
        let flags = Flags::default().with(SYNTHETIC).with(OVERRIDE);
        let info = l.u.info(sym);
        let (_, def) = l.method_like(class, sym, flags, info, focus, |l, _, _| match sym {
            _ if sym == cc.enum_ordinal => source.ordinal(l),
            _ if sym == cc.hash_code => {
                let name = source.name(l);
                let tpe = l.u.widen(l.tpd.tpe(name));
                let string = l.u.class_symbol(tpe);
                let hash_code = string.and_then(|s| l.u.decl(s, "hashCode").ok()).unwrap_or_default();
                let hash_code = hash_code.into_iter().find(|&m| l.u.symbol(m).kind == Kind::Def);
                l.call(name, hash_code.unwrap_or(cc.hash_code), &[])
            }
            _ => source.name(l),
        })?;
        l.tpd.get_mut(def).span = Span::NONE;
        out.push(l.with_span(def, focus));
    }
    Ok(out)
}

/// Where an enum value's name and ordinal come from (dotty's `identifierRef` and `ordinalRef`):
/// the parameters of `$new`, or literals for a case with a body.
enum Source {
    Params { name: SymbolId, ordinal: SymbolId },
    Literals { name: String, ordinal: i32 },
}

impl Source {
    fn name(&self, l: &mut Lower) -> TNodeId {
        match self {
            Source::Params { name, .. } => l.ident(*name, Span::NONE),
            Source::Literals { name, .. } => l.string_literal(name),
        }
    }

    fn ordinal(&self, l: &mut Lower) -> TNodeId {
        match *self {
            Source::Params { ordinal, .. } => l.ident(ordinal, Span::NONE),
            Source::Literals { ordinal, .. } => l.int_literal(ordinal),
        }
    }
}

/// The index of enum case `value` among its enum's cases, classes and values in order.
fn child_index(l: &Lower, value: SymbolId) -> Option<i32> {
    let companion = l.u.symbol(value).owner?;
    let mut cases = l.u.symbol(companion).decls.iter().copied().filter(|&d| {
        let kind = l.u.symbol(d).kind;
        let class_or_value =
            kind == Kind::Class && !l.u.is_module_class(d) || kind == Kind::Val && !l.u.is_module(d);
        class_or_value && has(l.u, d, CASE) && has(l.u, d, ENUM)
    });
    cases.position(|d| d == value).map(|i| i as i32)
}

/// The parameter `name` of method `m` in unit `root`.
fn param(l: &Lower, root: TNodeId, m: SymbolId, name: &str) -> Option<SymbolId> {
    let def = subtrees(l.tpd, root)
        .into_iter()
        .find(|&t| matches!(l.tpd.tree(t), Tree::DefDef { .. }) && l.tpd.sym(t) == Some(m))?;
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(def) else { return None };
    let clauses = l.tpd.clause_items(paramss).to_vec();
    let params = clauses.into_iter().flat_map(|c| l.tpd.items(c).to_vec());
    params.filter_map(|p| l.tpd.sym(p)).find(|&p| l.u.symbol(p).name == name)
}

/// `private def readResolve(): AnyRef = E.fromOrdinal(this.ordinal)` for the class of a value of
/// the enum whose companion defines `owner`, `$new` or the case's value.
fn read_resolve(l: &mut Lower, class: SymbolId, owner: SymbolId, focus: Span) -> Result<TNodeId> {
    let companion = l.u.symbol(owner).owner.expect("an enum value's owner");
    let Some(module) = l.u.source_module(companion) else {
        return unsupported(focus, "an enum companion without its value");
    };
    let from_ordinal = l.u.decl(companion, "fromOrdinal").unwrap_or_default();
    let Some(from_ordinal) = from_ordinal.into_iter().find(|&m| l.u.symbol(m).kind == Kind::Def) else {
        return unsupported(focus, "an enum companion without `fromOrdinal`");
    };
    let (sym, any_ref) = private_method(l, class, "readResolve");
    let this = l.this(class, Span::NONE);
    let ordinal = l.call(this, l.defs.case_classes.enum_ordinal, &[]);
    let module = l.ident(module, Span::NONE);
    let rhs = l.call(module, from_ordinal, &[ordinal]);
    let tpt = l.type_tree(any_ref, focus);
    let def = l.method_def(sym, &[], tpt, rhs, Span::NONE);
    Ok(l.with_span(def, focus))
}
