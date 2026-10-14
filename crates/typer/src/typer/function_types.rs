//! Function types `A => B` (dotty's `typedFunctionType`) and the parts of function types.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The class `scala.FunctionN` of `n` parameters.
    pub fn function_class(&mut self, n: usize) -> Option<TypeId> {
        let c = self.resolve_full_name(&format!("scala.Function{n}"), true)?;
        Some(crate::defn::type_ref(self.u, c))
    }

    /// The parameter and result types of `t` if it is a `FunctionN` of `n` parameters.
    pub fn function_parts(&mut self, t: TypeId, n: usize) -> Option<(Vec<TypeId>, TypeId)> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        let class = self.u.class_symbol(tycon)?;
        (self.u.full_name(class) == format!("scala.Function{n}") && args.len() == n + 1)
            .then(|| (args[..n].to_vec(), args[n]))
    }

    /// Type `t` as dotty's messages show it: function types as `(A, B) => R`.
    pub fn show_type(&mut self, t: TypeId) -> String {
        let t = self.zonk(t);
        if let Type::Applied(tycon, args) = self.u.types().get(t).clone()
            && let Some(class) = self.u.class_symbol(tycon)
            && self.u.full_name(class) == format!("scala.Function{}", args.len() - 1)
        {
            let (params, result) = args.split_at(args.len() - 1);
            let params: Vec<String> = params.iter().map(|&p| self.show_type(p)).collect();
            let result = self.show_type(result[0]);
            return match params.as_slice() {
                [p] if !p.contains("=>") => format!("{p} => {result}"),
                _ => format!("({}) => {result}", params.join(", ")),
            };
        }
        self.u.show(t)
    }

    /// Function type `(args) => body` as `FunctionN[args, body]`.
    pub fn typed_function_type(&mut self, id: NodeId, args: List, body: NodeId, ctx: Ctx) -> TNodeId {
        let arg_ids = self.ast.items(args).to_vec();
        let Some(class) = self.function_class(arg_ids.len()) else {
            return self.unsupported(id, "function type of that many parameters");
        };
        let mut typed: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
        typed.push(self.typed_type(body, ctx));
        if typed.iter().any(|&t| matches!(self.tpd.tree(t), T::ByNameTypeTree { .. })) {
            return self.unsupported(id, "by-name function parameter");
        }
        let span = self.ast.span(id);
        let first = arg_ids.first().copied().unwrap_or(body);
        let tycon = self.type_tree(class, self.ast.span(first).start_pos(), false);
        let arg_types: Vec<TypeId> = typed.iter().map(|&t| self.tpd.tpe(t)).collect();
        let tpe = self.u.intern_type(Type::Applied(class, arg_types));
        let args = self.tpd.list(&typed);
        self.tpd.node(T::AppliedTypeTree { tpt: tycon, args }, span, Some(tpe), None)
    }
}
