//! The classes that start `@main` methods (dotty's `MainProxies`): for `@main def f()`,
//! `final class f { @static def main(args: Array[String]): Unit = try f() catch
//! case error: ParseError => showError(error) }`.

use crate::ctx::Ctx;
use crate::defn::{applied, owner_prefix, type_ref};
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::Typer;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The typed proxy classes of the `@main` methods among `stats` and their objects' members.
    pub fn main_proxies(&mut self, stats: &[TNodeId], ctx: Ctx) -> Vec<TNodeId> {
        let mut mains = Vec::new();
        self.main_methods(stats, &mut mains);
        let classes: Vec<NodeId> =
            mains.into_iter().filter_map(|(m, span)| self.main_proxy(m, span)).collect();
        if classes.is_empty() || self.diags.has_errors() {
            return Vec::new();
        }
        self.index(&classes, ctx);
        self.typed_stats(&classes, ctx)
    }

    fn main_methods(&mut self, stats: &[TNodeId], out: &mut Vec<(SymbolId, Span)>) {
        for &s in stats {
            let sym = self.tpd.sym(s);
            match *self.tpd.tree(s) {
                T::DefDef { .. } => {
                    let annots = sym.and_then(|s| self.syms.annotations.get(&s));
                    if let Some(&(_, span)) =
                        annots.into_iter().flatten().find(|(c, _)| *c == self.defn.main_annot)
                    {
                        out.push((sym.expect("a method"), span));
                    }
                }
                T::TypeDef { rhs, .. }
                    if sym.is_some_and(|s| self.u.symbol(s).flags.has(tasty::tags::OBJECT)) =>
                {
                    if let T::Template { body, .. } = *self.tpd.tree(rhs) {
                        let body = self.tpd.items(body).to_vec();
                        self.main_methods(&body, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// An untyped tree standing for a typed one of type `tpe`: a reference to `sym`, or a type tree.
    fn splice(&mut self, tree: T, tpe: TypeId, sym: Option<SymbolId>) -> NodeId {
        let typed = self.tpd.node(tree, Span::NONE, Some(tpe), sym);
        let id = self.ast.node(Tree::TypeTree);
        self.splices.insert(id, typed);
        id
    }

    fn splice_ref(&mut self, sym: SymbolId) -> NodeId {
        let owner = self.u.symbol(sym).owner.expect("a member");
        let prefix = owner_prefix(self.u, owner);
        let tpe = self.u.intern_type(Type::Sym(Some(prefix), sym));
        let name = TName { name: self.name(&self.u.symbol(sym).name.clone()), is_type: false };
        self.splice(T::Ident { name }, tpe, Some(sym))
    }

    fn splice_type(&mut self, tpe: TypeId) -> NodeId {
        self.splice(T::TypeTree { inferred: false }, tpe, None)
    }

    fn ident(&mut self, text: &str, is_type: bool) -> NodeId {
        let name = TName { name: self.name(text), is_type };
        self.ast.node(Tree::Ident { name })
    }

    fn command_line_parser_member(&mut self, name: &str, kind: Kind) -> Option<SymbolId> {
        let class = self.u.module_class(self.defn.command_line_parser)?;
        self.u.member(class, name).into_iter().find(|&s| self.u.symbol(s).kind == kind)
    }

    fn main_proxy(&mut self, fun: SymbolId, annot_span: Span) -> Option<NodeId> {
        let owner = self.u.symbol(fun).owner.expect("a member");
        if !self.is_static_owner(owner) {
            self.unsupported_at(annot_span, "@main method not statically accessible");
            return None;
        }
        if self.syms.annotations.get(&fun).is_some_and(|a| a.len() > 1) {
            self.unsupported_at(annot_span, "@main method with other annotations");
            return None;
        }
        let mut call = self.splice_ref(fun);
        let info = self.u.info(fun);
        match self.u.types().get(info).clone() {
            Type::Method { params, result, .. } if params.is_empty() => {
                if matches!(self.u.types().get(result), Type::Method { .. }) {
                    self.unsupported_at(annot_span, "@main method with several parameter lists");
                    return None;
                }
                let args = self.ast.list(&[]);
                call = self.ast.node(Tree::Apply { fun: call, args });
            }
            Type::Method { params, result, .. }
                if !matches!(self.u.types().get(result), Type::Method { .. }) =>
            {
                let mut args = Vec::new();
                for (i, &(_, p)) in params.iter().enumerate() {
                    let repeated = self.repeated_elem(p);
                    let parser = if repeated.is_some() { "parseRemainingArguments" } else { "parseArgument" };
                    let Some(parser) = self.command_line_parser_member(parser, Kind::Def) else {
                        self.unsupported_at(annot_span, "CommandLineParser missing from the library");
                        return None;
                    };
                    let parse = self.parse_call(parser, repeated.unwrap_or(p), i);
                    args.push(match repeated {
                        Some(_) => {
                            let star = self.ident("_*", true);
                            self.ast.node(Tree::Typed { expr: parse, tpt: star })
                        }
                        None => parse,
                    });
                }
                let args = self.ast.list(&args);
                call = self.ast.node(Tree::Apply { fun: call, args });
            }
            Type::Method { .. } | Type::Poly { .. } => {
                self.unsupported_at(
                    annot_span,
                    "@main method with several parameter lists or type parameters",
                );
                return None;
            }
            _ => {}
        }
        let parse_error = self.command_line_parser_member("ParseError", Kind::Class);
        let show_error = self.command_line_parser_member("showError", Kind::Def);
        let (Some(parse_error), Some(show_error)) = (parse_error, show_error) else {
            self.unsupported_at(annot_span, "CommandLineParser missing from the library");
            return None;
        };
        let parse_error = type_ref(self.u, parse_error);
        let err_var = self.ident("error", false);
        let tpt = self.splice_type(parse_error);
        let pat = self.ast.node(Tree::Typed { expr: err_var, tpt });
        let show = self.splice_ref(show_error);
        let err_ref = self.ident("error", false);
        let args = self.ast.list(&[err_ref]);
        let handler_body = self.ast.node(Tree::Apply { fun: show, args });
        let handler = self.ast.node(Tree::CaseDef { pat, guard: None, body: handler_body });
        let cases = self.ast.list(&[handler]);
        let body = self.ast.node(Tree::Try { expr: call, cases, finalizer: None });
        let string = type_ref(self.u, self.defn.string);
        let array_of_string = applied(self.u, self.defn.array, vec![string]);
        let args_tpt = self.splice_type(array_of_string);
        let args_name = TName { name: self.name("args"), is_type: false };
        let arg_mods = self.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
        let main_arg =
            self.ast.node(Tree::ValDef { name: args_name, tpt: args_tpt, rhs: None, mods: arg_mods });
        let unit = type_ref(self.u, self.defn.unit);
        let unit_tpt = self.splice_type(unit);
        let main_meth =
            self.def_def("main", &[main_arg], unit_tpt, Some(body), f::JAVA_STATIC.or(f::SYNTHETIC));
        let empty_tpt = self.ast.node(Tree::TypeTree);
        let constr = self.def_def("<init>", &[], empty_tpt, None, f::EMPTY);
        let parents = self.ast.list(&[]);
        let body = self.ast.list(&[main_meth]);
        let template =
            Tree::Template { constr, parents_or_derived: parents, self_: None, body, derived: None };
        let template = self.ast.node(template);
        let cls_name = TName { name: self.name(&self.u.symbol(fun).name.clone()), is_type: true };
        let mods = self.ast.mods(Modifiers::with_flags(f::FINAL.or(f::INVISIBLE)));
        let cls = self.ast.node(Tree::TypeDef { name: cls_name, rhs: template, mods });
        self.ast.with_span(cls, annot_span.to_synthetic());
        Some(cls)
    }

    /// `parser[tpe](args, i)`, a call of `CommandLineParser.parseArgument` or
    /// `parseRemainingArguments`.
    fn parse_call(&mut self, parser: SymbolId, tpe: TypeId, i: usize) -> NodeId {
        let fun = self.splice_ref(parser);
        let targ = self.splice_type(tpe);
        let targs = self.ast.list(&[targ]);
        let fun = self.ast.node(Tree::TypeApply { fun, args: targs });
        let args = self.ident("args", false);
        let index = self.ast.node(Tree::Literal { value: syntax::ast::constant::Constant::Int(i as i32) });
        let args = self.ast.list(&[args, index]);
        self.ast.node(Tree::Apply { fun, args })
    }

    fn def_def(
        &mut self,
        name: &str,
        params: &[NodeId],
        tpt: NodeId,
        rhs: Option<NodeId>,
        flags: FlagSet,
    ) -> NodeId {
        let name = TName { name: self.name(name), is_type: false };
        let clause = self.ast.list(params);
        let paramss = self.ast.clauses(&[clause]);
        let mods = self.ast.mods(Modifiers::with_flags(flags));
        self.ast.node(Tree::DefDef { name, paramss, tpt, rhs, mods })
    }
}
