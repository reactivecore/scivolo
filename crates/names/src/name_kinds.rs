// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameKinds.scala
//   compiler/src/dotty/tools/dotc/core/NameOps.scala
//   compiler/src/dotty/tools/dotc/core/StdNames.scala
//! The names the compiler generates from other names (dotty's `NameKinds`), spelled as dotty
//! and Scala Native spell them.

use crate::nme;

/// The separator of an expanded name `p$C$$x` (dotty's `ExpandedName`).
pub const EXPAND_SEPARATOR: &str = "$$";
/// The separator of a trait setter's name `T$_setter_$x_=`.
pub const TRAIT_SETTER_SEPARATOR: &str = "$_setter_$";
/// The suffix of a setter's name.
pub const SETTER_SUFFIX: &str = "_=";
/// The prefix of a super accessor's name.
pub const SUPER_PREFIX: &str = "super$";
/// The separator of a super accessor's name with a mixin, `m$super$Q`.
const SUPER_MIX_SEPARATOR: &str = "$super$";
/// The separator of the name of a method computing a super constructor's argument.
pub const SUPER_ARG_SEPARATOR: &str = "$superArg$";
/// The suffix of a forwarder to a parameter accessor of a superclass.
pub const PARAM_ACCESSOR_SUFFIX: &str = "$accessor";
/// The suffix of an adapted closure's method.
pub const ADAPTED_SUFFIX: &str = "$adapted";
/// The suffix of a parameter's copy in a tail-recursive loop.
const TAIL_LOCAL_SUFFIX: &str = "$tailLocal";
/// The separator of a lazy value's bitmap name.
pub const BITMAP_PREFIX: &str = "bitmap$";
/// The base of a temporary's name (dotty's `TempResultName`).
pub const TEMP_RESULT: &str = "ev$";
/// The filler of a lifted name for each term the definition was in.
pub const TERM_FILLER: &str = "_$";
/// The separator of a default getter's name.
pub const DEFAULT_GETTER: &str = "$default$";
/// A constructor's name `<init>` as part of another name.
pub const DEFAULT_GETTER_INIT: &str = "$lessinit$greater";
/// The prefix of the type tags of a specialized name, after the name.
const SPECIALIZED_PREFIX: &str = "$mc";
/// The suffix of a specialized name.
const SPECIALIZED_SUFFIX: &str = "$sp";
/// The prefix of an inline accessor's name.
pub const INLINE_PREFIX: &str = "inline$";

/// Builds the expanded name `prefix$$name` (dotty's `ExpandedName`).
pub fn expanded(prefix: &str, name: &str) -> String {
    format!("{prefix}{EXPAND_SEPARATOR}{name}")
}

/// Builds the flat name `prefix$name` of a member lifted out of `prefix` (dotty's `FlatName`).
pub fn flat(prefix: &str, name: &str) -> String {
    format!("{prefix}${name}")
}

/// Builds `base$`, the prefix a unique name's number follows (dotty's `UniqueName`).
pub fn unique_prefix(base: &str) -> String {
    format!("{base}$")
}

/// Builds the base `prefix$anonfun` of the name of a closure's method, `prefix` the
/// `unique_prefix` of the member it is in, or empty.
pub fn anon_fun(prefix: &str) -> String {
    format!("{prefix}{}", nme::ANON_FUN)
}

/// Builds the name of the accessor of the enclosing instance, `prefix$$$outer`.
pub fn outer_accessor(prefix: &str) -> String {
    expanded(prefix, nme::OUTER)
}

/// Determines whether `name` names an accessor of an enclosing instance or its field.
pub fn is_outer(name: &str) -> bool {
    name == nme::OUTER || name.ends_with(&expanded("", nme::OUTER))
}

/// Builds the name `x_=` of the setter of variable `x` (dotty's `setterName`).
pub fn setter(x: &str) -> String {
    format!("{x}{SETTER_SUFFIX}")
}

/// Builds the name of trait setter `T$_setter_$x_=` of value `x`, `prefix` the trait's
/// (dotty's `TraitSetterName`).
pub fn trait_setter(prefix: &str, x: &str) -> String {
    format!("{prefix}{TRAIT_SETTER_SEPARATOR}{}", setter(x))
}

/// Builds the name of the super accessor of member `name`: `super$name`, or `name$super$mix`
/// for a super call naming mixin `mix` (dotty's `superAccessorName`).
pub fn super_accessor(name: &str, mix: &str) -> String {
    match mix {
        "" => format!("{SUPER_PREFIX}{name}"),
        mix => format!("{name}{SUPER_MIX_SEPARATOR}{mix}"),
    }
}

/// Splits unexpanded super accessor name `name` into its member's name and its mixin: `m` and
/// an empty mixin for `super$m`, `m` and `Q` for `m$super$Q` (dotty's `decomposeSuperName`).
pub fn decompose_super(name: &str) -> Option<(&str, &str)> {
    match name.strip_prefix(SUPER_PREFIX) {
        Some(member) => Some((member, "")),
        None => name.rsplit_once(SUPER_MIX_SEPARATOR),
    }
}

/// Builds the name of the protected accessor of `name` (dotty's `ProtectedAccessorName`).
pub fn protected_accessor(name: &str) -> String {
    format!("protected${name}")
}

/// Builds the name of the inline accessor of `name` (dotty's `InlineAccessorName`).
pub fn inline_accessor(name: &str) -> String {
    format!("{INLINE_PREFIX}{name}")
}

/// Builds the name of the getter of the `n`th default argument, from 1, of method `name`
/// (dotty's `DefaultGetterName`).
pub fn default_getter(name: &str, n: usize) -> String {
    format!("{name}{DEFAULT_GETTER}{n}")
}

/// Builds the name of the extension method of value class method `name` (dotty's `ExtMethName`).
pub fn ext_meth(name: &str) -> String {
    format!("{name}$extension")
}

/// Builds the name of a forwarder to parameter accessor `name` (dotty's `ParamAccessorName`).
pub fn param_accessor(name: &str) -> String {
    format!("{name}{PARAM_ACCESSOR_SUFFIX}")
}

/// Builds the name of the field of value `name` when another member takes its name (dotty's
/// `ExplicitFieldName`).
pub fn explicit_field(name: &str) -> String {
    format!("{name}$field")
}

/// Builds the name of method `name` taking its context parameters directly (dotty's
/// `DirectMethName`).
pub fn direct(name: &str) -> String {
    format!("{name}$direct")
}

/// Builds the name of closure method `name` adapted to boxed values (dotty's
/// `AdaptedClosureName`).
pub fn adapted_closure(name: &str) -> String {
    format!("{name}{ADAPTED_SUFFIX}")
}

/// Builds the base of the name of a lazy value's holder or field (dotty's `LazyLocalName`).
pub fn lazy_local(name: &str) -> String {
    format!("{name}$lzy")
}

/// Builds the base of the name of a lazy value's initializer (dotty's `LazyLocalInitName`).
pub fn lazy_local_init(name: &str) -> String {
    format!("{name}$lzyINIT")
}

/// Builds the name of the `VarHandle` of lazy field `name` (dotty's `LazyVarHandleName`).
pub fn lazy_var_handle(name: &str) -> String {
    format!("{name}$lzyHandle")
}

/// Builds the base of the name of lazy value `name`'s bitmap (dotty's `LazyBitMapName`).
pub fn lazy_bitmap(name: &str) -> String {
    format!("{name}{BITMAP_PREFIX}")
}

/// Builds the base of the name of parameter `name`'s copy in a tail-recursive loop (dotty's
/// `TailLocalName`).
pub fn tail_local(name: &str) -> String {
    format!("{name}{TAIL_LOCAL_SUFFIX}")
}

/// Builds the base of the name of variable `name`'s temporary in a tail call (dotty's
/// `TailTempName`).
pub fn tail_temp(name: &str) -> String {
    format!("{name}$tmp")
}

/// Builds the name of the `n`th method, from 1, computing a super constructor argument of
/// class `class` (dotty's `SuperArgName`).
pub fn super_arg(class: &str, n: usize) -> String {
    format!("{class}{SUPER_ARG_SEPARATOR}{n}")
}

/// Builds the base `super_arg` names number, for class `class`.
pub fn super_arg_base(class: &str) -> String {
    format!("{class}{}", SUPER_ARG_SEPARATOR.trim_end_matches('$'))
}

/// Builds the specialized name `name$mc<tags>$sp`, each type by its tag (dotty's
/// `specializedName`, `specializedFunction`).
pub fn specialized(name: &str, tags: &str) -> String {
    format!("{name}{SPECIALIZED_PREFIX}{tags}{SPECIALIZED_SUFFIX}")
}

/// Determines whether `name` is a specialized variant of method `base`.
pub fn is_specialized(name: &str, base: &str) -> bool {
    name.strip_prefix(base).is_some_and(|r| r.starts_with(SPECIALIZED_PREFIX))
}

/// Builds the name of the `i`th synthetic parameter, `x$i` (dotty's `syntheticParamName`).
pub fn synthetic_param(i: usize) -> String {
    format!("x${i}")
}

/// Builds the name of the static forwarder `m$` of trait method `name` (dotty's
/// `traitSuperAccessorName`).
pub fn trait_super_accessor(name: &str) -> String {
    format!("{name}$")
}

/// Builds the name of the `n`th lambda class of top-level class `top`, as Scala Native names it.
pub fn lambda_class(top: &str, n: u32) -> String {
    format!("{top}$$Lambda${n}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_spelled_as_dotty_spells_them() {
        let dotty = [
            (expanded("p$C", "x"), "p$C$$x"),
            (outer_accessor("C"), "C$$$outer"),
            (setter("x"), "x_="),
            (trait_setter("p$T", "x"), "p$T$_setter_$x_="),
            (super_accessor("m", ""), "super$m"),
            (super_accessor("m", "Q"), "m$super$Q"),
            (default_getter("f", 2), "f$default$2"),
            (default_getter(DEFAULT_GETTER_INIT, 1), "$lessinit$greater$default$1"),
            (specialized("apply", "II"), "apply$mcII$sp"),
            (super_arg("C", 1), "C$superArg$1"),
            (super_arg_base("C"), "C$superArg"),
            (ext_meth("m"), "m$extension"),
            (lazy_local("x"), "x$lzy"),
            (synthetic_param(1), "x$1"),
            (lambda_class("p.C", 2), "p.C$$Lambda$2"),
        ];
        for (ours, expected) in dotty {
            assert_eq!(ours, expected);
        }
    }

    #[test]
    fn generated_names_are_recognized_again() {
        for (name, mix) in [("m", ""), ("m", "Q"), ("+", "")] {
            assert_eq!(decompose_super(&super_accessor(name, mix)), Some((name, mix)));
        }
        assert_eq!(decompose_super("m"), None);
        assert!(is_specialized(&specialized("apply", "II"), "apply"));
        assert!(!is_specialized("applyII", "apply"));
        assert!(!is_specialized(&specialized("apply", "I"), "map"));
        assert!(is_outer(nme::OUTER) && is_outer(&outer_accessor("C")));
        assert!(!is_outer("outer") && !is_outer("C$outer"));
    }
}
