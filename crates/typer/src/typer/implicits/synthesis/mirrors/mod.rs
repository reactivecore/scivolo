// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! A `Mirror` argument (dotty's `synthesizedMirror`, `makeMirror`): the product mirror of a case
//! class, case object, enum value or tuple (`product.rs`, `tuple.rs`), the sum mirror of a sealed
//! class or enum (`sum.rs`), an anonymous class where no companion serves (`anonymous.rs`), cast
//! to the refined mirror type (`formal.rs`).

mod anonymous;
mod child_prefixes;
mod children;
mod formal;
mod product;
mod sources;
mod sum;
mod tuple;

use super::Synthesizer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::Typer;
use formal::{MirrorFormal, MirrorKind};
use sources::MirrorSource;
use syntax::ast::Span;
use tasty::{Type, TypeId};

/// The refusal when the library lacks a class a mirror needs.
const MISSING: &str = "Mirror without its library classes";

/// Makes the `Mirror` argument `formal` asks for (dotty's `synthesizedMirror` and what it calls);
/// its trees are placed at `at` and typed in `ctx`.
#[derive(Clone, Copy)]
struct MirrorSynthesis {
    formal: MirrorFormal,
    at: Span,
    ctx: Ctx,
}

impl Synthesizer {
    /// Synthesizes a `Mirror.Of[T]`, `Mirror.ProductOf[T]` or `Mirror.SumOf[T]` argument for a
    /// case class, case object, enum, sealed hierarchy or tuple `T`; `None` for another expected
    /// type or a `T` without a mirror.
    pub fn mirror(self, t: &mut Typer<'_>, pt: TypeId) -> Result<Option<TNodeId>, &'static str> {
        let Some(mut formal) = MirrorFormal::of(t.u, pt) else { return Ok(None) };
        formal.mirrored = t.fully_defined_as(formal.mirrored, true)?;
        let Some(source) = MirrorSource::of(t, formal.mirrored)? else { return Ok(None) };
        MirrorSynthesis { formal, at: self.at, ctx: self.ctx }.run(t, source)
    }
}

impl MirrorSynthesis {
    /// Synthesizes the product or sum mirror of `source` the expected type asks for; for a plain
    /// `Mirror.Of[T]` the product mirror if `T` has one, else its sum mirror.
    fn run(self, t: &mut Typer<'_>, source: MirrorSource) -> Result<Option<TNodeId>, &'static str> {
        match self.formal.kind {
            MirrorKind::Product => self.product(t, source),
            MirrorKind::Sum => self.sum(t, source),
            MirrorKind::Any => match self.product(t, source.clone()) {
                Ok(Some(m)) => Ok(Some(m)),
                Ok(None) => self.sum(t, source),
                Err(what) => match self.sum(t, source) {
                    Ok(Some(m)) => Ok(Some(m)),
                    _ => Err(what),
                },
            },
        }
    }

    /// Builds the instance creation `new C(args)` of library class `full`, e.g.
    /// `scala.runtime.TupleMirror`.
    fn new_with_args(self, t: &mut Typer<'_>, full: &str, args: &[TNodeId]) -> Result<TNodeId, &'static str> {
        let at = self.at;
        let class = t.resolve_full_name(full, true).ok_or(MISSING)?;
        let ctor = t.u.decl(class, "<init>").ok().and_then(|d| d.first().copied()).ok_or(MISSING)?;
        let raw = crate::defn::type_ref(t.u, class);
        let tpt = t.tpd.node(T::TypeTree { inferred: false }, at, Some(raw), None);
        let new = t.tpd.node(T::New { tpt }, at, Some(raw), None);
        let select = t.build().select(new, ctor, at);
        Ok(t.build().apply(select, args, raw, at))
    }

    /// Casts the mirror object `mirror` to its mirror type `to`, giving the synthesized argument.
    fn cast(self, t: &mut Typer<'_>, mirror: TNodeId, to: TypeId) -> Result<Option<TNodeId>, &'static str> {
        t.build().cast_to(mirror, to, self.at).map(Some)
    }
}

/// Builds the tuple type of `labels` as string literal types, e.g. `("x", "y")`.
fn label_pairs(t: &mut Typer<'_>, labels: &[String]) -> Result<TypeId, &'static str> {
    let labels: Vec<TypeId> = labels.iter().map(|l| t.u.intern_type(Type::Str(l.clone()))).collect();
    t.nested_pairs(&labels)
}
