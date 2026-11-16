//! The implicit views of `f` interpolation arguments to the types format conversions take, which
//! the typer finds and the lowering applies.

use super::{TNodeId, Tpd};

/// The argument types the format conversions accept.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Any,
    Boolean,
    Null,
    Char,
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    BigInt,
    BigDecimal,
    Calendar,
    Date,
    Formattable,
}

impl Target {
    /// The targets other than `Any`.
    pub const CLASSES: [Target; 14] = [
        Target::Boolean,
        Target::Null,
        Target::Char,
        Target::Byte,
        Target::Short,
        Target::Int,
        Target::Long,
        Target::Float,
        Target::Double,
        Target::BigInt,
        Target::BigDecimal,
        Target::Calendar,
        Target::Date,
        Target::Formattable,
    ];

    /// The qualified name of the class of type `self`; `None` for `Any`.
    pub fn class_name(self) -> Option<&'static str> {
        Some(match self {
            Target::Any => return None,
            Target::Boolean => "scala.Boolean",
            Target::Null => "scala.Null",
            Target::Char => "scala.Char",
            Target::Byte => "scala.Byte",
            Target::Short => "scala.Short",
            Target::Int => "scala.Int",
            Target::Long => "scala.Long",
            Target::Float => "scala.Float",
            Target::Double => "scala.Double",
            Target::BigInt => "scala.math.BigInt",
            Target::BigDecimal => "scala.math.BigDecimal",
            Target::Calendar => "java.util.Calendar",
            Target::Date => "java.util.Date",
            Target::Formattable => "java.util.Formattable",
        })
    }
}

/// What the view search of an argument to a target found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FormatView {
    /// The argument converted by the view.
    Converted(TNodeId),
    /// A search the typer does not support or whose view the lowering cannot apply.
    Unknown,
}

impl Tpd {
    /// Records the views of `f` interpolation argument `arg` to the targets it does not conform to;
    /// a target without one has no view.
    pub fn set_format_views(&mut self, arg: TNodeId, views: Vec<(Target, FormatView)>) {
        self.format_views.insert(arg, views);
    }

    /// Gives the view of argument `arg` to `target`: `None` if there is none, `Unknown` if the
    /// typer did not record the argument's views.
    pub fn format_view(&self, arg: TNodeId, target: Target) -> Option<FormatView> {
        let Some(views) = self.format_views.get(&arg) else { return Some(FormatView::Unknown) };
        views.iter().find(|(t, _)| *t == target).map(|&(_, v)| v)
    }
}
