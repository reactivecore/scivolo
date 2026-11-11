// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Constructors.scala
//   compiler/src/dotty/tools/dotc/transform/Mixin.scala
//! The superclass constructor call of dotty's `Mixin` and the constructor body of
//! `Constructors`: the class parameters stored in their fields, that call, the fields of eager
//! objects, then the other field initializers and statements of the class body in order.

use super::first_transform::literal_of;
use super::getters::{is_const_final, is_param_accessor, is_setter};
use super::initializers::constructor_params;
use super::memoize::bottom;
use super::param_forwarding::is_forwarder;
use crate::{Lower, Result, unsupported};
use names::{name_kinds, nme};
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
    let (mut inits, mut eager) = (Vec::new(), Vec::new());
    let mut accessors = Vec::new();
    let stats = l.tpd.items(body).to_vec();
    for &stat in &stats {
        if matches!(l.tpd.tree(stat), Tree::DefDef { .. } | Tree::ValDef { rhs: None, .. }) {
            let field_param = |s| is_param_accessor(l, s) && !is_setter(l, s) && !is_forwarder(l, s);
            accessors.extend(l.tpd.sym(stat).filter(|&s| field_param(s)));
        }
    }
    for stat in stats {
        let sym = l.tpd.sym(stat);
        super::uninitialized_defs::drop_wildcard(l, stat, class);
        let const_getter = sym.filter(|&s| is_const_final(l, s));
        match *l.tpd.tree(stat) {
            Tree::Import { .. } => {}
            Tree::DefDef { name, paramss, tpt, rhs }
                if const_getter.is_some()
                    && !rhs.is_some_and(|r| {
                        matches!(l.tpd.tree(r), Tree::Literal { .. } | Tree::ClassLiteral { .. })
                    }) =>
            {
                let getter = const_getter.expect("a getter's symbol");
                let literal = literal_of(l, getter, l.tpd.span(stat)).or(rhs);
                inits.extend(rhs.filter(|&r| Some(r) != literal));
                l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: literal };
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
                let init = assign(l, lhs, rhs, at);
                if super::lazy_vals::is_eager_field(l, field) { &mut eager } else { &mut inits }.push(init);
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
        let Some(field) = field else {
            let fieldless = bottom(l, accessor).is_some() || is_const_final(l, accessor);
            assert!(fieldless, "a class parameter's field");
            continue;
        };
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
    let full_name = l.u.full_name(class);
    if full_name == "scala.AnyVal" {
        stats.push(object_init_call(l, class, Span::at(span.point()))?);
    } else if !tasty::PRIMITIVE_VALUE_CLASSES.contains(&full_name.as_str()) {
        stats.push(super_call(l, class, first)?);
    }
    stats.extend(eager.into_iter().chain(inits));
    let dropped_outer = matches!(outer, Some((None, _))).then_some(template_span);
    let spans = stats.iter().map(|&s| l.tpd.span(s)).chain(dropped_outer);
    let block_span = spans.filter(|s| s.exists()).reduce(|a, b| a.union(b));
    let block_span = block_span.map_or(l.tpd.span(constr), |s| s.with_point(s.start()));
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
    avoid_field_clashes(l, &class_stats);
    let body = l.tpd.list(&class_stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// Renames each field of a getter named as another field of the class, `x`, to `x$field`
/// (dotty's `checkNoFieldClashes`).
fn avoid_field_clashes(l: &mut Lower, stats: &[TNodeId]) {
    let fields = stats.iter().filter(|&&s| matches!(l.tpd.tree(s), Tree::ValDef { .. }));
    let fields: Vec<SymbolId> = fields.filter_map(|&s| l.tpd.sym(s)).collect();
    let of_getter = |l: &Lower, f: SymbolId| l.fields.get(&f).is_some_and(|&g| g != f);
    let plain: Vec<String> =
        fields.iter().filter(|&&f| !of_getter(l, f)).map(|&f| l.u.symbol(f).name.clone()).collect();
    for f in fields {
        if of_getter(l, f) && plain.contains(&l.u.symbol(f).name) {
            let name = name_kinds::explicit_field(&l.u.symbol(f).name);
            l.u.rename(f, &name);
        }
    }
}

/// `if param eq null then throw new NullPointerException()`, at `span`.
fn null_test(l: &mut Lower, param: SymbolId, span: Span) -> TNodeId {
    let npe = l.u.class_ref("java.lang.NullPointerException");
    let npe = l.u.class_symbol(npe).expect("`NullPointerException`");
    let ctor = l.u.decls_named(npe, nme::CONSTRUCTOR).into_iter().find(|&c| {
        let info = l.u.info(c);
        l.u.method(info).is_some_and(|m| m.params.is_empty())
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

/// `super.<init>()` of `Object` for `class`, whose parent `Any` erases to it.
fn object_init_call(l: &mut Lower, class: SymbolId, span: Span) -> Result<TNodeId> {
    let object = l.u.defn().object;
    let ctor = super::lazy_trees::method(l, object, nme::CONSTRUCTOR, span)?;
    let object_type = crate::definitions::type_ref(l.u, object);
    let sup = l.super_(class, object_type, span);
    let fun = l.select(sup, ctor, span);
    let unit = l.u.intern_type(tasty::Type::Constant(tasty::tags::UNITconst, 0));
    Ok(l.apply(fun, &[], unit, span))
}
