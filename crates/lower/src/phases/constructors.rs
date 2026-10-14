//! The superclass constructor call of dotty's `Mixin` and the constructor body of
//! `Constructors`: the class parameters stored in their fields, that call, then the field
//! initializers and statements of the class body in order.

use super::first_transform::literal_of;
use super::getters::{is_const_final, is_param_accessor, is_setter};
use super::initializers::constructor_params;
use super::memoize::bottom;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    if l.u.is_trait(class) {
        return super::trait_init::transform(l, td);
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let mut class_stats = Vec::new();
    let span = l.tpd.span(td);
    let mut inits = Vec::new();
    let mut accessors = Vec::new();
    let stats = l.tpd.items(body).to_vec();
    for &stat in &stats {
        if matches!(l.tpd.tree(stat), Tree::DefDef { .. } | Tree::ValDef { rhs: None, .. }) {
            accessors.extend(l.tpd.sym(stat).filter(|&s| is_param_accessor(l, s) && !is_setter(l, s)));
        }
    }
    for stat in stats {
        let sym = l.tpd.sym(stat);
        super::uninitialized_defs::drop_wildcard(l, stat, class);
        let const_getter = sym.filter(|&s| is_const_final(l, s));
        match *l.tpd.tree(stat) {
            Tree::Import { .. } => {}
            Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }
                if const_getter.is_some()
                    && !matches!(l.tpd.tree(rhs), Tree::Literal { .. } | Tree::ClassLiteral { .. }) =>
            {
                let getter = const_getter.expect("a getter's symbol");
                let literal = literal_of(l, getter, l.tpd.span(stat)).unwrap_or(rhs);
                if literal != rhs {
                    inits.push(rhs);
                }
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(literal) };
                class_stats.push(stat);
            }
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::ValDef { rhs: None, .. } => {
                class_stats.push(stat)
            }
            Tree::ValDef { name, tpt, rhs: Some(rhs) } => {
                let field = l.tpd.sym(stat).expect("a field's symbol");
                let getter = l.fields.get(&field).copied().filter(|&g| g != field);
                let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
                if getter.is_some_and(|g| bottom(l, g).is_some()) {
                    l.u.set_owner(field, ctor);
                    let s = l.tpd.span(rhs);
                    l.tpd.get_mut(stat).span = if s.exists() { s.with_point(s.start()) } else { s };
                }
                if l.u.symbol(field).owner == Some(ctor) {
                    inits.push(stat);
                    continue;
                }
                class_stats.push(stat);
                let memoized = getter.is_some();
                let at = match (memoized, l.tpd.span(rhs)) {
                    (true, s) if s.exists() => s.with_point(s.start()),
                    (true, s) => s,
                    (false, s) if !l.tpd.span(stat).exists() && s.exists() => s.with_point(s.start()),
                    (false, _) => l.tpd.span(stat),
                };
                let lhs = l.field_ref(field, at);
                inits.push(assign(l, lhs, rhs, at));
                l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: None };
            }
            _ => inits.push(stat),
        }
    }
    let Tree::DefDef { name, paramss, tpt, rhs: None } = *l.tpd.tree(constr) else {
        return unsupported(l.tpd.span(constr), "a constructor with a body");
    };
    let params = constructor_params(l, constr);
    let template_span = l.tpd.span(template);
    let mut stats = Vec::new();
    for accessor in accessors {
        let name = &l.u.symbol(accessor).name;
        let param = params.iter().copied().find(|&p| &l.u.symbol(p).name == name);
        let param = param.expect("a class parameter's constructor parameter");
        let field = l.fields.iter().find(|&(_, &g)| g == accessor).map(|(&f, _)| f);
        let field = field.expect("a class parameter's field");
        let lhs = l.field_ref(field, template_span);
        let value = l.ident(param, template_span);
        stats.push(assign(l, lhs, value, template_span));
    }
    let outer = l.outers.get(&class).copied();
    let null_test_at = stats.len();
    if let Some((Some(field), param)) = outer {
        let lhs = l.field_ref(field, template_span);
        let value = l.ident(param, template_span);
        stats.push(assign(l, lhs, value, template_span));
    }
    let Some(&first) = l.tpd.items(parents).first() else {
        return unsupported(span, "a class without a superclass");
    };
    stats.push(super_call(l, class, first)?);
    stats.extend(inits);
    let dropped_outer = matches!(outer, Some((None, _))).then_some(template_span);
    let spans = stats.iter().map(|&s| l.tpd.span(s)).chain(dropped_outer);
    let block_span = spans.filter(|s| s.exists()).reduce(|a, b| a.union(b));
    let block_span = block_span.map_or(span, |s| s.with_point(s.start()));
    let at = block_span.start_pos();
    if let Some((_, param)) = outer {
        let test_at = if null_test_at > 0 { Span::at(template_span.end()) } else { at };
        let test = null_test(l, param, test_at);
        stats.insert(null_test_at, test);
    }
    let proxies: Vec<_> =
        l.proxies.iter().copied().filter(|&(f, _)| l.u.symbol(f).owner == Some(class)).collect();
    let mut inits = Vec::new();
    for (field, param) in proxies {
        let lhs = l.field_ref(field, at);
        let value = l.ident(param, at);
        inits.push(assign(l, lhs, value, at));
    }
    let stats: Vec<TNodeId> = inits.into_iter().chain(stats).collect();
    let unit = l.unit_literal(block_span);
    let rhs = l.block(&stats, unit, block_span);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
    let body = l.tpd.list(&class_stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `if param eq null then throw new NullPointerException()`, at `span`.
fn null_test(l: &mut Lower, param: SymbolId, span: Span) -> TNodeId {
    let npe = l.u.class_ref("java.lang.NullPointerException");
    let npe = l.u.class_symbol(npe).expect("`NullPointerException`");
    let ctor = l.u.decl(npe, "<init>").unwrap_or_default().into_iter().find(|&c| {
        let info = l.u.info(c);
        matches!(l.u.types().get(info), tasty::Type::Method { params, .. } if params.is_empty())
    });
    let ctor = ctor.expect("`NullPointerException()`");
    let read = l.ident(param, Span::NONE);
    let null = l.null_literal(Span::NONE);
    let cond = super::lazy_trees::eq(l, read, null);
    let new = l.new_(npe, Span::NONE);
    let error = l.call(new, ctor, &[]);
    l.tpd.get_mut(error).tpe = l.tpd.get(new).tpe;
    let throw = l.throw_(error);
    let unit = l.unit_literal(Span::NONE);
    let tpe = l.tpd.tpe(unit);
    let test =
        l.tpd.node(Tree::If { cond, thenp: throw, elsep: unit, inline: false }, Span::NONE, Some(tpe), None);
    l.with_span(test, span)
}

fn assign(l: &mut Lower, lhs: TNodeId, rhs: TNodeId, span: Span) -> TNodeId {
    let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
    l.tpd.node(Tree::Assign { lhs, rhs }, span, Some(unit), None)
}

/// `super.<init>(args)` for parent `new P[Ts](args1)...(argsN)`, the argument lists joined; in
/// the block of a parent that is one.
fn super_call(l: &mut Lower, class: SymbolId, parent: TNodeId) -> Result<TNodeId> {
    let span = l.tpd.span(parent);
    if let Tree::Block { stats, expr } = *l.tpd.tree(parent) {
        let call = super_call(l, class, expr)?;
        let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
        return Ok(l.tpd.node(Tree::Block { stats, expr: call }, span, Some(unit), None));
    }
    let Tree::Apply { mut fun, args } = *l.tpd.tree(parent) else {
        return unsupported(span, "a superclass without a constructor call");
    };
    let mut args = l.tpd.items(args).to_vec();
    loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: inner, args: first } => {
                args.splice(0..0, l.tpd.items(first).iter().copied());
                fun = inner;
            }
            Tree::TypeApply { fun: inner, .. } => fun = inner,
            _ => break,
        }
    }
    let Some(ctor) = l.tpd.sym(fun).filter(|&s| l.u.is_constructor(s)) else {
        return unsupported(span, "a superclass constructor call without a constructor");
    };
    let parent_type = l.tpd.tpe(parent);
    let sup = l.super_(class, parent_type, span);
    let fun = l.select(sup, ctor, span);
    let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
    Ok(l.apply(fun, &args, unit, span))
}
