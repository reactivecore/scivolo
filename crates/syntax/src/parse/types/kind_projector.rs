// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! `-Xkind-projector`: `*` (and `_` with `underscores`) as type lambda placeholders, and `λ`.

use crate::ast::flags;
use crate::ast::{NodeId, Span, TName, Tree};
use crate::config::KindProjector;
use crate::parse::Parser;
use crate::token::Token;

impl Parser<'_> {
    pub fn kind_projector(&self) -> KindProjector {
        self.s.cfg.kind_projector
    }

    /// `_`, `-_` or `+_` as a placeholder under `-Xkind-projector:underscores`.
    pub fn kind_projector_underscore(&mut self) -> Option<NodeId> {
        if self.kind_projector() != KindProjector::Underscores {
            return None;
        }
        let start = self.offset();
        let text = if self.tok() == Token::Uscore && !self.in_match_pattern {
            self.next();
            "_".to_string()
        } else if (self.is_ident_named("-") || self.is_ident_named("+"))
            && self.s.lookahead().token == Token::Uscore
        {
            let sign = self.s.name_text().to_string();
            self.next();
            self.next();
            format!("{sign}_")
        } else {
            return None;
        };
        let name = self.type_name(&text);
        let id = self.ast.node(Tree::Ident { name });
        self.ast.with_span(id, Span::new(start, self.last_offset(), start));
        Some(id)
    }

    /// The type parameter `name` of a kind-projector lambda, without a variance marker.
    fn kind_projector_type_def(&mut self, name: &str) -> NodeId {
        let name = name.strip_prefix(['+', '-']).unwrap_or(name);
        let name = self.type_name(name);
        let rhs = self.ast.node(Tree::TypeBoundsTree { lo: None, hi: None, alias: None });
        let mods = self.mods_with(flags::PARAM);
        self.ast.node(Tree::TypeDef { name, rhs, mods })
    }

    /// `ts` with placeholders replaced by fresh names, and the type parameters for them.
    pub fn replace_kind_projector_placeholders(&mut self, ts: &[NodeId]) -> (Vec<NodeId>, Vec<NodeId>) {
        let underscores = self.kind_projector() == KindProjector::Underscores;
        let mut tparams = Vec::new();
        let new = ts
            .iter()
            .map(|&t| {
                let placeholder = match *self.ast.tree(t) {
                    Tree::Ident { name } => match self.s.names.text(name.name) {
                        "*" | "-*" | "+*" => true,
                        "_" | "-_" | "+_" => underscores,
                        _ => false,
                    },
                    _ => false,
                };
                if !placeholder {
                    return t;
                }
                let fresh = self.fresh_name("_$");
                let text = self.s.names.text(fresh).to_string();
                tparams.push(self.kind_projector_type_def(&text));
                self.ast.node(Tree::Ident { name: TName { name: fresh, is_type: true } })
            })
            .collect();
        (new, tparams)
    }

    /// `applied[args]`; under kind-projector `λ[X => ...]`, or placeholders made parameters.
    pub fn applied_type(&mut self, applied: NodeId, args: Vec<NodeId>, start: u32) -> NodeId {
        if self.kind_projector() == KindProjector::Disabled {
            let args = self.ast.list(&args);
            return self.ast.node(Tree::AppliedTypeTree { tpt: applied, args });
        }
        if let Tree::Ident { name } = *self.ast.tree(applied)
            && self.s.names.text(name.name) == "λ"
        {
            if let [arg] = args[..]
                && let Tree::Function { args: params, body, .. } = *self.ast.tree(arg)
            {
                let params = self.ast.items(params).to_vec();
                let idents: Vec<(NodeId, TName)> = params
                    .iter()
                    .filter_map(|&p| match *self.ast.tree(p) {
                        Tree::Ident { name } => Some((p, name)),
                        _ => None,
                    })
                    .collect();
                let tparams: Vec<NodeId> = idents
                    .into_iter()
                    .map(|(p, name)| {
                        let text = self.s.names.text(name.name).to_string();
                        let td = self.kind_projector_type_def(&text);
                        let span = self.ast.span(p);
                        self.ast.with_span(td, span);
                        td
                    })
                    .collect();
                if tparams.len() == params.len() {
                    let tparams = self.ast.list(&tparams);
                    return self.ast.node(Tree::LambdaTypeTree { tparams, body });
                }
            }
            let msg = "λ requires a single argument of the form X => ... or (X, Y) => ...";
            self.syntax_error_span(msg, Span::synthetic(start, self.last_offset()));
            let args = self.ast.list(&args);
            return self.ast.node(Tree::AppliedTypeTree { tpt: applied, args });
        }
        let (args, tparams) = self.replace_kind_projector_placeholders(&args);
        let args = self.ast.list(&args);
        let t = self.ast.node(Tree::AppliedTypeTree { tpt: applied, args });
        self.lambda_abstract(&tparams, t)
    }

    /// `t`, or under kind-projector a tuple type with placeholders as a type lambda.
    pub fn kind_projector_tuple(&mut self, t: NodeId) -> NodeId {
        if self.kind_projector() == KindProjector::Disabled {
            return t;
        }
        let Tree::Tuple { trees } = *self.ast.tree(t) else { return t };
        let ts = self.ast.items(trees).to_vec();
        let (ts, tparams) = self.replace_kind_projector_placeholders(&ts);
        if tparams.is_empty() {
            return t;
        }
        let trees = self.ast.list(&ts);
        let tuple = self.ast.node(Tree::Tuple { trees });
        let tparams = self.ast.list(&tparams);
        self.ast.node(Tree::LambdaTypeTree { tparams, body: tuple })
    }

    /// A function type; under kind-projector with placeholders among `params` and `result` as a
    /// type lambda.
    pub fn function_type(&mut self, params: &[NodeId], result: NodeId) -> NodeId {
        if self.kind_projector() == KindProjector::Disabled {
            let args = self.ast.list(params);
            return self.ast.node(Tree::Function { args, body: result, mods: None, wildcard: false });
        }
        let all: Vec<NodeId> = params.iter().copied().chain([result]).collect();
        let (mut all, tparams) = self.replace_kind_projector_placeholders(&all);
        let result = all.pop().expect("a result type");
        let args = self.ast.list(&all);
        let f = self.ast.node(Tree::Function { args, body: result, mods: None, wildcard: false });
        self.lambda_abstract(&tparams, f)
    }
}
