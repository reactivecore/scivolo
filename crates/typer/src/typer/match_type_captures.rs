// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The bounds of the type variables a match type case binds (dotty's `mapPatternBounds`).

use super::Typer;
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Gives each of `captures` that is an argument of a class type in `pat` the bounds of the
    /// class's type parameter.
    pub fn bound_captures(&mut self, pat: TypeId, captures: &[SymbolId]) {
        let mut at = Vec::new();
        self.u.map_type(pat, &mut |u, part| {
            if let Some((tycon, args)) = u.applied(part)
                && let Type::Sym(_, c) = *u.types().get(tycon)
                && matches!(u.symbol(c).kind, Kind::Class | Kind::Type)
            {
                for (i, &a) in args.iter().enumerate() {
                    if let Type::Sym(_, s) = *u.types().get(a)
                        && captures.contains(&s)
                    {
                        at.push((s, c, i));
                    }
                }
            }
            None
        });
        let unbounded = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        for (s, c, i) in at {
            let params = self.u.type_params(c);
            let info = self.u.info(s);
            if info != unbounded {
                continue;
            }
            let bounds = match params.is_empty() {
                true => self.lambda_param_bounds(c, i).unwrap_or(info),
                false => self.with_param_bounds(info, params.get(i).copied(), &params),
            };
            self.u.set_info(s, bounds);
        }
    }

    /// The bounds of the `i`th parameter of the type lambda abstract type `c` stands for, if they
    /// refer to no parameter.
    fn lambda_param_bounds(&mut self, c: SymbolId, i: usize) -> Option<TypeId> {
        let info = self.u.info(c);
        let lambda = match *self.u.types().get(info) {
            Type::Bounds(_, hi) => hi,
            _ => info,
        };
        let bounds = self.u.lambda(lambda)?.params.get(i)?.1;
        let mut dependent = false;
        self.u.map_type(bounds, &mut |u, t| {
            dependent |= matches!(u.types().get(t), Type::Param(..));
            None
        });
        (!dependent && matches!(self.u.types().get(bounds), Type::Bounds(..))).then_some(bounds)
    }
}
