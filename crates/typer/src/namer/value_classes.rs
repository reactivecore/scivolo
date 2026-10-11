//! The rules of value classes (dotty's `Checking.checkDerivedValueClass`), with the messages of
//! `reporting/messages.scala`.

use crate::tpd::{TNodeId, Tree as T};
use crate::typer::Typer;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{NodeId, Span, Tree};
use syntax::diag::{Message, MessageId};
use tasty::{Kind, SymbolId, Type};

const PRIMITIVES: [&str; 9] = [
    "scala.Boolean",
    "scala.Byte",
    "scala.Short",
    "scala.Char",
    "scala.Int",
    "scala.Long",
    "scala.Float",
    "scala.Double",
    "scala.Unit",
];

fn syntax_msg(code: u16, explained: bool, text: impl Into<String>) -> Message {
    Message { text: text.into(), id: Some(MessageId { code, kind: "Syntax", explained }) }
}

impl Typer<'_> {
    /// Whether `c` is a class deriving from `AnyVal`, not `AnyVal` or a primitive value class
    /// (dotty's `isDerivedValueClass`).
    pub fn is_derived_value_class(&mut self, c: SymbolId) -> bool {
        let Some(any_val) = self.u.builtins().any_val else { return false };
        self.u.symbol(c).kind == Kind::Class
            && c != any_val
            && self.u.derives_from(c, any_val)
            && !PRIMITIVES.contains(&self.u.full_name(c).as_str())
    }

    /// Makes class `cls` final if it is a value class, its parents set.
    pub fn finalize_value_class(&mut self, cls: SymbolId) {
        if self.is_derived_value_class(cls) {
            let flags = self.dotty_flags(cls).or(f::FINAL);
            self.syms.set_flags(self.u, cls, flags);
        }
    }

    /// Reports how class `cls`, defined by `id` with typed parents `parents` and statements
    /// `stats`, breaks the rules of value classes, if it derives from `AnyVal`.
    pub fn check_derived_value_class(
        &mut self,
        id: NodeId,
        cls: SymbolId,
        parents: &[TNodeId],
        stats: &[NodeId],
    ) {
        if !self.is_derived_value_class(cls) || self.is_enum_anonym_class(cls) {
            return;
        }
        let span = self.ast.span(id);
        let flags = self.dotty_flags(cls);
        let shown = self.shown_class(cls);
        if flags.is(f::TRAIT) || flags.is(f::MODULE) {
            self.vc_error(span, syntax_msg(68, true, format!("{shown} cannot extend AnyVal")));
        }
        if flags.is(f::ENUM) {
            self.vc_error(span, syntax_msg(206, false, format!("{shown} may not be a value class")));
        }
        if flags.is(f::ABSTRACT) {
            self.vc_error(span, syntax_msg(74, false, "Value classes may not be abstract"));
        }
        let owner = self.u.symbol(cls).owner;
        if !owner.is_some_and(|o| self.is_static_owner(o)) {
            let local = owner.is_some_and(|o| self.u.symbol(o).kind.is_term());
            let what = if local { "local class" } else { "member of another class" };
            self.vc_error(span, syntax_msg(75, false, format!("Value classes may not be a {what}")));
        }
        self.check_parent_is_not_any_val_alias(span, parents);
        let params = self.value_class_params(cls);
        let wrapped = params.first().and_then(|&p| {
            let info = self.u.info(p);
            let info = match *self.u.types().get(info) {
                Type::ByName(result) => result,
                _ => info,
            };
            self.u.class_symbol(info)
        });
        if wrapped.is_some_and(|c| self.is_derived_value_class(c)) {
            let msg = "A value class may not wrap another user-defined value class";
            self.vc_error(span, syntax_msg(76, false, msg));
        } else if let Some((&param, rest)) = params.split_first() {
            self.check_value_class_param(param, rest, stats, span);
            if self.u.underlying(cls) != Some(param) {
                self.unsupported_at(span, "value class the erasure does not unwrap");
            }
        } else {
            self.vc_error(span, syntax_msg(78, false, "Value class needs one val parameter"));
        }
        for &s in stats {
            self.check_value_class_member(s);
        }
    }

    /// The parameter accessors of class `cls` that are not getters.
    fn value_class_params(&mut self, cls: SymbolId) -> Vec<SymbolId> {
        let decls = self.u.symbol(cls).decls.clone();
        let params: Vec<SymbolId> = decls
            .into_iter()
            .filter(|&d| {
                let fl = self.dotty_flags(d);
                self.u.symbol(d).kind.is_term() && fl.is(f::PARAM_ACCESSOR) && !fl.is(f::ACCESSOR)
            })
            .collect();
        for &p in &params {
            self.complete(p);
        }
        params
    }

    /// Checks the underlying parameter `param` of a value class defined at `cls_span` and the
    /// parameters `rest` after it.
    fn check_value_class_param(
        &mut self,
        param: SymbolId,
        rest: &[SymbolId],
        stats: &[NodeId],
        cls_span: Span,
    ) {
        let span = self.def_span(param, stats, cls_span);
        let flags = self.dotty_flags(param);
        let info = self.u.info(param);
        let context_function = self
            .u
            .class_symbol(info)
            .is_some_and(|c| self.u.full_name(c).starts_with("scala.ContextFunction"));
        if context_function {
            self.vc_error(span, "value classes are not allowed for context function types");
        }
        if flags.is(f::MUTABLE) {
            self.vc_error(span, syntax_msg(77, true, "A value class parameter may not be a var"));
        }
        if matches!(self.u.types().get(info), Type::ByName(_)) {
            let name = self.u.symbol(param).name.clone();
            let msg = format!("Value class parameter `{name}` may not be call-by-name");
            self.vc_error(span, syntax_msg(126, false, msg));
        }
        if flags.is(f::ERASED) {
            self.vc_error(span, "value class first parameter cannot be `erased`");
        } else {
            for &p in rest {
                if !self.dotty_flags(p).is(f::ERASED) {
                    let span = self.def_span(p, stats, cls_span);
                    self.vc_error(span, "value class can only have one non `erased` parameter");
                }
            }
        }
    }

    /// Checks statement `stat` of a value class's body (dotty's `checkValueClassMember`).
    fn check_value_class_member(&mut self, stat: NodeId) {
        let sym = self.syms.by_tree.get(&stat).copied();
        let flags = sym.map_or(f::EMPTY, |s| self.dotty_flags(s));
        let (code, text) = match *self.ast.tree(stat) {
            Tree::TypeDef { rhs, .. } if matches!(self.ast.tree(rhs), Tree::Template { .. }) => {
                (70, "Value classes may not define an inner class")
            }
            Tree::ValDef { .. } if !flags.is(f::PARAM_ACCESSOR) => {
                (71, "Value classes may not define non-parameter field")
            }
            Tree::DefDef { name, .. } if self.text(name.name) == "<init>" => {
                (72, "Value classes may not define a secondary constructor")
            }
            Tree::ValDef { .. }
            | Tree::DefDef { .. }
            | Tree::TypeDef { .. }
            | Tree::Import { .. }
            | Tree::Export { .. }
            | Tree::Empty => return,
            _ => (73, "Value classes may not contain initialization statements"),
        };
        let span = self.ast.span(stat);
        self.vc_error(span, syntax_msg(code, false, text));
    }

    /// Reports a first parent of `parents` that names an alias of `AnyVal`.
    fn check_parent_is_not_any_val_alias(&mut self, span: Span, parents: &[TNodeId]) {
        let Some(&first) = parents.first() else { return };
        let mut tpt = self.tpd.meth_part(first);
        if let T::Select { qualifier, .. } = *self.tpd.tree(tpt)
            && let T::New { tpt: t } = *self.tpd.tree(qualifier)
        {
            tpt = t;
        }
        let Some(alias) = self.tpd.sym(tpt) else { return };
        if self.u.symbol(alias).kind != Kind::Type {
            return;
        }
        let tpe = self.tpd.tpe(tpt);
        let any_val = self.u.builtins().any_val;
        if any_val.is_some() && self.u.class_symbol(tpe) == any_val {
            let name = self.u.symbol(alias).name.clone();
            let msg = format!("A value class cannot extend a type alias (type {name}) of AnyVal");
            self.vc_error(span, syntax_msg(210, false, msg));
        }
    }

    /// Whether `cls` is the anonymous class of an enum case (dotty's `isEnumAnonymClass`).
    fn is_enum_anonym_class(&self, cls: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(cls).owner else { return false };
        let o = self.u.symbol(owner);
        self.u.symbol(cls).name == "$anon"
            && (o.name == "$new" || o.kind == Kind::Val && o.flags.has(tasty::tags::CASE))
    }

    /// Class `cls` as dotty shows a symbol: `class C`, `trait T`, `object O`.
    fn shown_class(&self, cls: SymbolId) -> String {
        let flags = self.dotty_flags(cls);
        let name = &self.u.symbol(cls).name;
        if flags.is(f::MODULE) {
            format!("object {}", name.strip_suffix('$').unwrap_or(name))
        } else if flags.is(f::TRAIT) {
            format!("trait {name}")
        } else {
            format!("class {name}")
        }
    }

    /// The span of the definition of `sym` among `stats`, else anywhere in the unit, else `or`.
    fn def_span(&self, sym: SymbolId, stats: &[NodeId], or: Span) -> Span {
        let tree = stats.iter().copied().find(|s| self.syms.by_tree.get(s) == Some(&sym));
        let tree = tree.or_else(|| self.syms.by_tree.iter().find(|&(_, &s)| s == sym).map(|(&t, _)| t));
        tree.map_or(or, |t| self.ast.span(t))
    }

    fn dotty_flags(&self, s: SymbolId) -> FlagSet {
        self.syms.flags.get(&s).copied().unwrap_or(f::EMPTY)
    }

    fn vc_error(&mut self, span: Span, msg: impl Into<Message>) {
        self.diags.error_at(span.start(), span.end(), span.point(), msg);
    }
}
