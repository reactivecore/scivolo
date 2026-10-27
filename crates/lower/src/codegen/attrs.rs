// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/Attrs.scala

use nir::Attr;

pub fn rank(a: &Attr) -> u8 {
    match a {
        Attr::MayInline | Attr::InlineHint | Attr::NoInline | Attr::AlwaysInline => 0,
        Attr::MaySpecialize | Attr::NoSpecialize => 1,
        Attr::UnOpt | Attr::NoOpt | Attr::DidOpt | Attr::BailOpt(_) => 2,
        Attr::Align { .. } => 3,
        Attr::Extern { .. } => 4,
        Attr::Dyn => 5,
        Attr::Stub => 6,
        Attr::LinkCppRuntime => 7,
        Attr::Abstract => 8,
        Attr::Volatile => 9,
        Attr::Final => 10,
        Attr::SafePublish => 11,
        Attr::LinktimeResolved => 12,
        Attr::UsesIntrinsic => 13,
        Attr::Link(_) => 14,
        Attr::Define(_) => 15,
    }
}
