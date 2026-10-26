// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! Capture conversion on paths (dotty's `compareCaptured`): a wildcard argument of a stable
//! path's type compared as the type parameter selected on the path.

use super::Typer;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Whether stable path `a` conforms to class type `b`, or to each side of intersection `b`,
    /// with the wildcard arguments of its base type in `b`'s class replaced by the type
    /// parameters selected on `a`.
    pub fn conforms_captured_path(&mut self, a: TypeId, b: TypeId) -> bool {
        if !matches!(self.u.types().get(a), Type::Sym(..) | Type::TermRef(..))
            || !super::stability::is_stable_path(self.u, a)
        {
            return false;
        }
        let b = self.zonk(b);
        if let Type::And(x, y) = *self.u.types().get(b) {
            return self.conforms(a, x) && self.conforms(a, y);
        }
        let Type::Applied(tycon, _) = *self.u.types().get(b) else { return false };
        let Some(class) = self.u.class_symbol(tycon) else { return false };
        let widened = self.u.widen(a);
        let Some(base) = self.u.base_type(widened, class) else { return false };
        let Type::Applied(base_tycon, args) = self.u.types().get(base).clone() else { return false };
        let params = self.u.type_params(class);
        let wildcard = |t: &Self, x: TypeId| matches!(t.u.types().get(x), Type::Bounds(..));
        if params.len() != args.len() || !args.iter().any(|&x| wildcard(self, x)) {
            return false;
        }
        let captured: Vec<TypeId> = args
            .into_iter()
            .zip(params)
            .map(|(x, p)| match wildcard(self, x) {
                true => self.u.intern_type(Type::Sym(Some(a), p)),
                false => x,
            })
            .collect();
        let captured = self.u.intern_type(Type::Applied(base_tycon, captured));
        self.conforms(captured, b)
    }
}
