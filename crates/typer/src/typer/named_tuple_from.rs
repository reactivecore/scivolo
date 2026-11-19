//! The reduction of `NamedTuple.From[T]` inside a type.

use super::Typer;
use tasty::TypeId;

impl Typer<'_> {
    /// `t` with each `NamedTuple.From[T]` that folds replaced by its named tuple: of a case class
    /// `T` its fields, of a tuple its elements named `_1`, `_2`, … (dotty's `From` reduction).
    pub fn with_case_fields(&mut self, mut t: TypeId) -> TypeId {
        let Some(from) = self.resolve_full_name("scala.NamedTuple$.From", true) else { return t };
        let mut applied = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Some((tycon, &[_])) = u.applied(part) {
                applied.push((part, tycon));
            }
            None
        });
        for (part, tycon) in applied {
            if self.referenced(tycon) != Some(from) {
                continue;
            }
            let Some(fields) = self.u.try_normalize(part) else { continue };
            t = self.u.map_type(t, &mut |_, p| (p == part).then_some(fields));
        }
        t
    }
}
