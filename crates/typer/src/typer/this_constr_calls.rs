// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/cc/Setup.scala
//   compiler/src/dotty/tools/dotc/core/Annotations.scala
//! The self-constructor calls of secondary constructors (dotty's `checkThisConstrCall`): the
//! constructor they call, and the `@caps.internal.paramAlias` annotations of the parameters they
//! pass on under capture checking (`Setup.recordParamAliases`).

use super::Typer;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::defn::PARAM_ALIAS;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Whether the self-constructor call in constructor body `tree` of `sym`, if any, calls a
    /// constructor defined before `sym` (dotty's `checkThisConstrCall`).
    pub fn calls_preceding_constructor(&mut self, tree: TNodeId, sym: SymbolId) -> bool {
        if let T::Block { stats, expr } = *self.tpd.tree(tree) {
            let first = self.tpd.items(stats).first().copied();
            return first.is_none_or(|s| self.calls_preceding_constructor(s, sym))
                && self.calls_preceding_constructor(expr, sym);
        }
        let Some(target) = self.this_constr_call_target(tree) else { return true };
        if self.is_primary_constructor(target) {
            return true;
        }
        let start = |t: &Self, s| t.syms.def_trees.get(&s).map(|&d| t.ast.span(d).start());
        match (start(self, sym), start(self, target)) {
            (Some(own), Some(called)) => own > called || self.tpd.span(tree).is_synthetic(),
            _ => true,
        }
    }

    /// Annotates the parameters of secondary constructor `ctor` that the self-constructor call
    /// in its body `rhs` passes on: in `def this(xx: B^) = this(xx, 0)` of
    /// `class A(x: B^, y: Int)`, `xx` gets `@paramAlias("x")`; one passed to another secondary
    /// constructor's parameter gets that parameter's aliases. The arguments of the call's last
    /// clause are paired with the target's parameters of all clauses, from the first.
    pub fn record_param_aliases(&mut self, ctor: SymbolId, rhs: TNodeId) {
        if let T::Block { stats, expr } = *self.tpd.tree(rhs) {
            if let Some(&call) = self.tpd.items(stats).first() {
                self.record_param_aliases(ctor, call);
            }
            return self.record_param_aliases(ctor, expr);
        }
        let Some(target) = self.this_constr_call_target(rhs) else { return };
        let T::Apply { args, .. } = *self.tpd.tree(rhs) else { return };

        let primary = self.is_primary_constructor(target);
        let decls = self.u.symbol(target).decls.clone();
        let params: Vec<SymbolId> =
            decls.into_iter().filter(|&p| self.u.symbol(p).kind == Kind::Param).collect();
        let args = self.tpd.items(args).to_vec();
        for (param, arg) in params.into_iter().zip(args) {
            let Some(a) = self.tpd.sym(arg).filter(|&a| {
                matches!(self.tpd.tree(arg), T::Ident { .. })
                    && self.u.symbol(a).kind == Kind::Param
                    && self.u.symbol(a).owner == Some(ctor)
            }) else {
                continue;
            };
            let names = match primary {
                true => vec![self.u.symbol(param).name.clone()],
                false => self.param_aliases(param),
            };
            for name in names {
                self.add_param_alias(a, &name);
            }
        }
    }

    /// Makes `new cls(text)` of span `span`, its parts at the start of `span`, if `cls` has a
    /// constructor of one parameter (dotty's `Annotation(cls, arg, span)`).
    pub fn string_annotation(&mut self, cls: SymbolId, text: &str, span: Span) -> Option<TNodeId> {
        let ctors = self.u.decls_named(cls, nme::CONSTRUCTOR);
        let ctor = ctors.into_iter().find(|&c| self.u.erased_method(c).0.len() == 1)?;
        let at = if span.exists() { span.start_pos() } else { span };
        let tpe = type_ref(self.u, cls);
        let tpt = self.type_tree(tpe, at, false);
        let new = self.tpd.node(T::New { tpt }, at, Some(tpe), None);
        let select = self.build().select(new, ctor, at);
        let arg = self.build().literal(Constant::String(text.encode_utf16().collect()), at);
        Some(self.build().apply(select, &[arg], tpe, span))
    }

    /// The constructor that `tree` calls if it is an application of `this(...)`.
    fn this_constr_call_target(&self, tree: TNodeId) -> Option<SymbolId> {
        if !matches!(self.tpd.tree(tree), T::Apply { .. }) {
            return None;
        }
        let callee = self.tpd.meth_part(tree);
        let self_call = matches!(*self.tpd.tree(callee), T::Select { qualifier, .. }
            if matches!(self.tpd.tree(qualifier), T::This { .. }));
        self.tpd.sym(callee).filter(|&s| self_call && self.u.symbol(s).name == nme::CONSTRUCTOR)
    }

    fn is_primary_constructor(&self, ctor: SymbolId) -> bool {
        let class = self.u.symbol(ctor).owner;
        class.and_then(|c| self.syms.primary_ctors.get(&c)) == Some(&ctor)
    }

    /// The names of the `@paramAlias` annotations of `param`.
    fn param_aliases(&mut self, param: SymbolId) -> Vec<String> {
        let mut annots = self.syms.annotations.get(&param).cloned().unwrap_or_default();
        annots.retain(|&a| self.u.is_class_named(self.tpd.tpe(a), PARAM_ALIAS));
        annots
            .into_iter()
            .filter_map(|a| {
                let T::Apply { args, .. } = *self.tpd.tree(a) else { return None };
                match self.tpd.tree(*self.tpd.items(args).first()?) {
                    T::Literal { value: Constant::String(s) } => Some(String::from_utf16_lossy(s)),
                    _ => None,
                }
            })
            .collect()
    }

    /// Puts `@paramAlias(name)` first among the annotations of parameter `param`.
    fn add_param_alias(&mut self, param: SymbolId, name: &str) {
        let Some(cls) = self.resolve_full_name(PARAM_ALIAS, true) else { return };
        let span = self.syms.param_trees.get(&param).map_or(Span::NONE, |&p| self.ast.span(p));
        let Some(annot) = self.string_annotation(cls, name, span) else { return };
        self.syms.annotations.entry(param).or_default().insert(0, annot);
        self.u.annotate(param, cls);
    }
}
