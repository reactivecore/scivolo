//! The NIR of casts, equality, switches and other expressions, as Scala Native 0.5.12 generates
//! it for the same snippets.

mod snippet;

use snippet::{assert_defs, lowered};

#[test]
fn casts_and_type_tests() {
    let source = r#"object C {
  def asString(x: Any): String = x.asInstanceOf[String]
  def upcast(s: String): Any = s.asInstanceOf[Any]
  def isString(x: Any): Boolean = x.isInstanceOf[String]
  def isInt(x: Any): Boolean = x.isInstanceOf[Int]
  def toNothing(x: Any): Nothing = x.asInstanceOf[Nothing]
  def toNull(x: Any): Null = x.asInstanceOf[Null]
  def toInt(x: Any): Int = x.asInstanceOf[Int]
  def nullCast: String = null.asInstanceOf[String]
}"#;
    let Some(dump) = lowered(source, "C$") else { return };
    assert_defs(
        &dump,
        &[
            r"
def @C$::asString(java.lang.Object): java.lang.String : (@C$, @java.lang.Object) => @java.lang.String {
%3(%2 <this> : @C$, %1 <x> : @java.lang.Object):
  %4 = as[@java.lang.String] %1 <x> : @java.lang.Object
  ret %4 : @java.lang.String
}",
            r"
def @C$::upcast(java.lang.String): java.lang.Object : (@C$, @java.lang.String) => @java.lang.Object {
%3(%2 <this> : @C$, %1 <s> : @java.lang.String):
  ret %1 <s> : @java.lang.String
}",
            r"
def @C$::isString(java.lang.Object): bool : (@C$, @java.lang.Object) => bool {
%3(%2 <this> : @C$, %1 <x> : @java.lang.Object):
  %4 = is[@java.lang.String] %1 <x> : @java.lang.Object
  ret %4 : bool
}",
            r"
def @C$::isInt(java.lang.Object): bool : (@C$, @java.lang.Object) => bool {
%3(%2 <this> : @C$, %1 <x> : @java.lang.Object):
  %4 = is[@java.lang.Integer] %1 <x> : @java.lang.Object
  ret %4 : bool
}",
            r#"
def @C$::toNothing(java.lang.Object): nothing : (@C$, @java.lang.Object) => nothing {
%3(%2 <this> : @C$, %1 <x> : @java.lang.Object):
  %4 = classalloc @java.lang.ClassCastException
  %5 = call[(@java.lang.ClassCastException, @java.lang.String) => unit] @java.lang.ClassCastException::<init>(java.lang.String) : ptr(%4 : !?@java.lang.ClassCastException, "Cannot cast to scala.Nothing")
  throw %4 : !?@java.lang.ClassCastException
}"#,
            r"
def @C$::toNull(java.lang.Object): null : (@C$, @java.lang.Object) => null {
%3(%2 <this> : @C$, %1 <x> : @java.lang.Object):
  %4 = as[@scala.runtime.Null$] %1 <x> : @java.lang.Object
  ret %4 : @scala.runtime.Null$
}",
            r"
def @C$::toInt(java.lang.Object): int : (@C$, @java.lang.Object) => int {
%3(%2 <this> : @C$, %1 <x> : @java.lang.Object):
  %4 = unbox[@java.lang.Integer] %1 <x> : @java.lang.Object
  ret %4 : int
}",
            r"
def @C$::nullCast(): java.lang.String : (@C$) => @java.lang.String {
%2(%1 <this> : @C$):
  ret null
}",
        ],
    );
}

#[test]
fn reference_equality_universal_equality_and_hashes() {
    let source = r#"object E {
  def refEq(a: AnyRef, b: AnyRef): Boolean = a eq b
  def refNe(a: AnyRef, b: AnyRef): Boolean = a ne b
  def strEq(a: String, b: String): Boolean = a == b
  def strNe(a: String, b: String): Boolean = a != b
  def anyEq(a: Any, b: Any): Boolean = a == b
  def litEq(b: String): Boolean = "lit" == b
  def nullEq(a: String): Boolean = a == null
  def anyHash(a: Any): Int = a.##
  def longHash(l: Long): Int = l.##
}"#;
    let Some(dump) = lowered(source, "E$") else { return };
    assert_defs(
        &dump,
        &[
            r"
def @E$::refEq(java.lang.Object, java.lang.Object): bool : (@E$, @java.lang.Object, @java.lang.Object) => bool {
%4(%3 <this> : @E$, %1 <a> : @java.lang.Object, %2 <b> : @java.lang.Object):
  %5 = ieq[@java.lang.Object] %1 <a> : @java.lang.Object, %2 <b> : @java.lang.Object
  ret %5 : bool
}",
            r"
def @E$::refNe(java.lang.Object, java.lang.Object): bool : (@E$, @java.lang.Object, @java.lang.Object) => bool {
%4(%3 <this> : @E$, %1 <a> : @java.lang.Object, %2 <b> : @java.lang.Object):
  %5 = ine[@java.lang.Object] %1 <a> : @java.lang.Object, %2 <b> : @java.lang.Object
  ret %5 : bool
}",
            r"
def @E$::strEq(java.lang.String, java.lang.String): bool : (@E$, @java.lang.String, @java.lang.String) => bool {
%4(%3 <this> : @E$, %1 <a> : @java.lang.String, %2 <b> : @java.lang.String):
  %9 = ieq[@java.lang.Object] %1 <a> : @java.lang.String, null
  if %9 : bool then %5 else %6
%5:
  %10 = ieq[@java.lang.Object] %2 <b> : @java.lang.String, null
  jump %7(%10 : bool)
%6:
  %11 = method %1 <a> : @java.lang.String, equals(java.lang.Object): bool
  %12 = call[(@java.lang.Object, @java.lang.Object) => bool] %11 : ptr(%1 <a> : @java.lang.String, %2 <b> : @java.lang.String)
  jump %7(%12 : bool)
%7(%8 : bool):
  ret %8 : bool
}",
            r"
def @E$::strNe(java.lang.String, java.lang.String): bool : (@E$, @java.lang.String, @java.lang.String) => bool {
%4(%3 <this> : @E$, %1 <a> : @java.lang.String, %2 <b> : @java.lang.String):
  %9 = ieq[@java.lang.Object] %1 <a> : @java.lang.String, null
  if %9 : bool then %5 else %6
%5:
  %10 = ieq[@java.lang.Object] %2 <b> : @java.lang.String, null
  jump %7(%10 : bool)
%6:
  %11 = method %1 <a> : @java.lang.String, equals(java.lang.Object): bool
  %12 = call[(@java.lang.Object, @java.lang.Object) => bool] %11 : ptr(%1 <a> : @java.lang.String, %2 <b> : @java.lang.String)
  jump %7(%12 : bool)
%7(%8 : bool):
  %13 = xor[bool] true, %8 : bool
  ret %13 : bool
}",
            r"
def @E$::anyEq(java.lang.Object, java.lang.Object): bool : (@E$, @java.lang.Object, @java.lang.Object) => bool {
%4(%3 <this> : @E$, %1 <a> : @java.lang.Object, %2 <b> : @java.lang.Object):
  %5 = call[(@java.lang.Object, @java.lang.Object) => bool] @scala.runtime.BoxesRunTime::static equals(java.lang.Object, java.lang.Object): bool : ptr(%1 <a> : @java.lang.Object, %2 <b> : @java.lang.Object)
  ret %5 : bool
}",
            r#"
def @E$::litEq(java.lang.String): bool : (@E$, @java.lang.String) => bool {
%3(%2 <this> : @E$, %1 <b> : @java.lang.String):
  %4 = method "lit", equals(java.lang.Object): bool
  %5 = call[(@java.lang.Object, @java.lang.Object) => bool] %4 : ptr("lit", %1 <b> : @java.lang.String)
  ret %5 : bool
}"#,
            r"
def @E$::nullEq(java.lang.String): bool : (@E$, @java.lang.String) => bool {
%3(%2 <this> : @E$, %1 <a> : @java.lang.String):
  %4 = ieq[@java.lang.Object] %1 <a> : @java.lang.String, null
  ret %4 : bool
}",
            r"
def @E$::anyHash(java.lang.Object): int : (@E$, @java.lang.Object) => int {
%3(%2 <this> : @E$, %1 <a> : @java.lang.Object):
  %4 = call[(@java.lang.Object) => int] @scala.runtime.Statics::static anyHash(java.lang.Object): int : ptr(%1 <a> : @java.lang.Object)
  ret %4 : int
}",
            r"
def @E$::longHash(long): int : (@E$, long) => int {
%3(%2 <this> : @E$, %1 <l> : long):
  %4 = call[(long) => int] @scala.runtime.Statics::static longHash(long): int : ptr(%1 <l> : long)
  ret %4 : int
}",
        ],
    );
}

#[test]
fn literal_matches_switch_on_ints_and_chain_equality_tests_on_strings() {
    let source = r#"object S {
  def ints(x: Int): String = x match {
    case 1 => "a"
    case 2 | 3 => "b"
    case _ => "c"
  }
  def strings(s: String): Int = s match {
    case "a" => 1
    case "b" => 2
    case _ => 3
  }
}"#;
    let Some(dump) = lowered(source, "S$") else { return };
    assert_defs(
        &dump,
        &[
            r#"
def @S$::ints(int): java.lang.String : (@S$, int) => @java.lang.String {
%3(%2 <this> : @S$, %1 <x> : int):
  jump %4
%4:
  switch %1 <x> : int { case int 1 => %7 case int 2 => %8 case int 3 => %9 default => %10 }
%10:
  jump %5("c")
%7:
  jump %5("a")
%8:
  jump %5("b")
%9:
  jump %5("b")
%5(%6 : @java.lang.String):
  ret %6 : @java.lang.String
}"#,
            r#"
def @S$::strings(java.lang.String): int : (@S$, @java.lang.String) => int {
%3(%2 <this> : @S$, %1 <s> : @java.lang.String):
  jump %4
%4:
  1: %11 = method "a", equals(java.lang.Object): bool
  1: %12 = call[(@java.lang.Object, @java.lang.Object) => bool] %11 : ptr("a", %1 <s> : @java.lang.String)
  if %12 : bool then %7 else %8
%7:
  jump %5(int 1)
%8:
  jump %9
%9:
  1: %18 = method "b", equals(java.lang.Object): bool
  1: %19 = call[(@java.lang.Object, @java.lang.Object) => bool] %18 : ptr("b", %1 <s> : @java.lang.String)
  if %19 : bool then %14 else %15
%14:
  jump %5(int 2)
%15:
  jump %16
%16:
  jump %5(int 3)
%5(%6 : int):
  ret %6 : int
}"#,
        ],
    );
}

#[test]
fn concatenation_monitors_arrays_and_finally() {
    let source = r#"object M {
  def concat(s: String, i: Int): String = s + i
  def locked(o: AnyRef): Int = o.synchronized { 1 }
  def arr: Int = { val a = new Array[Int](3); a(1) = 2; a(1) }
  def tryFinally(f: () => Int): Int = try f() finally println("done")
}"#;
    let Some(dump) = lowered(source, "M$") else { return };
    assert_defs(
        &dump,
        &[
            r"
def @M$::concat(java.lang.String, int): java.lang.String : (@M$, @java.lang.String, int) => @java.lang.String {
%4(%3 <this> : @M$, %1 <s> : @java.lang.String, %2 <i> : int):
  %5 = classalloc @java.lang.StringBuilder
  %6 = call[(@java.lang.StringBuilder, int) => unit] @java.lang.StringBuilder::<init>(int) : ptr(%5 : !?@java.lang.StringBuilder, int 0)
  %7 = call[(@java.lang.StringBuilder, @java.lang.String) => @java.lang.StringBuilder] @java.lang.StringBuilder::append(java.lang.String): java.lang.StringBuilder : ptr(%5 : !?@java.lang.StringBuilder, %1 <s> : @java.lang.String)
  %8 = call[(@java.lang.StringBuilder, int) => @java.lang.StringBuilder] @java.lang.StringBuilder::append(int): java.lang.StringBuilder : ptr(%5 : !?@java.lang.StringBuilder, %2 <i> : int)
  %9 = call[(@java.lang.StringBuilder) => @java.lang.String] @java.lang.StringBuilder::toString(): java.lang.String : ptr(%5 : !?@java.lang.StringBuilder)
  ret %9 : @java.lang.String
}",
            r"
def @M$::locked(java.lang.Object): int : (@M$, @java.lang.Object) => int {
%3(%2 <this> : @M$, %1 <o> : @java.lang.Object):
  %7 = call[(@java.lang.Object) => unit] @scala.scalanative.runtime.package::static enterMonitor(java.lang.Object): unit : ptr(%1 <o> : @java.lang.Object)
  jump %4
%4:
  jump %11
%11:
  1: %12 = call[(@java.lang.Object) => unit] @scala.scalanative.runtime.package::static exitMonitor(java.lang.Object): unit : ptr(%1 <o> : @java.lang.Object)
  jump %6(int 1)
%6(%8 : int):
  ret %8 : int
}",
            r"
def @M$::arr(): int : (@M$) => int {
%2(%1 <this> : @M$):
  %3 <a> = arrayalloc[int] int 3
  %4 = arraystore[int] %3 <a> : ?array[int], int 1, int 2
  %5 = arrayload[int] %3 <a> : ?array[int], int 1
  ret %5 : int
}",
            r#"
def @M$::tryFinally(scala.Function0): int : (@M$, @scala.Function0) => int {
%3(%2 <this> : @M$, %1 <f> : @scala.Function0):
  jump %5
%5:
  1: %10 = method %1 <f> : @scala.Function0, apply$mcI$sp(): int unwind %9 : @java.lang.Throwable to %4(%9 : @java.lang.Throwable)
  1: %12 = call[(@scala.Function0) => int] %10 : ptr(%1 <f> : @scala.Function0) unwind %11 : @java.lang.Throwable to %4(%11 : @java.lang.Throwable)
  jump %14
%4(%7 : @java.lang.Throwable):
  jump %18
%14:
  3: %15 = module @scala.Predef$
  3: %16 = method %15 : !?@scala.Predef$, println(java.lang.Object): unit
  3: %17 = call[(@scala.Predef$, @java.lang.Object) => unit] %16 : ptr(%15 : !?@scala.Predef$, "done")
  jump %6(%12 : int)
%18:
  4: %19 = module @scala.Predef$
  4: %20 = method %19 : !?@scala.Predef$, println(java.lang.Object): unit
  4: %21 = call[(@scala.Predef$, @java.lang.Object) => unit] %20 : ptr(%19 : !?@scala.Predef$, "done")
  throw %7 : @java.lang.Throwable
%6(%8 : int):
  ret %8 : int
}"#,
        ],
    );
}

#[test]
fn arrays_of_parameters_bounded_by_applied_higher_kinded_parameters() {
    let source = r#"trait Marker
object C {
  def f[F[x] <: Seq[x], B <: F[Int]](x: Array[B]): Unit = ()
  def m[K[x] <: x](a: K[Int]): K[Int] = a
  def n[K[x] <: x, B <: K[Marker]](a: Array[B]): Unit = ()
  def n2[K[x] <: x, B <: K[Marker]](a: B & java.io.Serializable): Unit = ()
}"#;
    let Some(dump) = lowered(source, "C$") else { return };
    assert_defs(
        &dump,
        &[
            r"
def @C$::f(array[scala.collection.immutable.Seq]): unit : (@C$, array[@scala.collection.immutable.Seq]) => unit {
%3(%2 <this> : @C$, %1 <x> : array[@scala.collection.immutable.Seq]):
  ret unit
}",
            r"
def @C$::m(int): int : (@C$, int) => int {
%3(%2 <this> : @C$, %1 <a> : int):
  ret %1 <a> : int
}",
            r"
def @C$::n(array[Marker]): unit : (@C$, array[@Marker]) => unit {
%3(%2 <this> : @C$, %1 <a> : array[@Marker]):
  ret unit
}",
            r"
def @C$::n2(Marker): unit : (@C$, @Marker) => unit {
%3(%2 <this> : @C$, %1 <a> : @Marker):
  ret unit
}",
        ],
    );
}

#[test]
fn jdk_overrides_javalib_lacks_are_called_on_their_class() {
    let source = "object R { def r = java.util.concurrent.ThreadLocalRandom.current()\n  \
                  def f(n: Int) = r.nextInt(0, n) + r.nextLong(0L, n) + r.nextDouble(1.0) }";
    let Some(dump) = lowered(source, "R$") else { return };
    for sig in ["int, int) => int", "long, long) => long", "double) => double"] {
        assert!(dump.contains(&format!("call[(@java.util.concurrent.ThreadLocalRandom, {sig}]")), "{dump}");
    }
}

#[test]
fn an_object_named_as_a_java_class_of_the_library_gets_a_mirror_class() {
    let source = "package scala.runtime\nobject Statics { def f: Int = 1 }";
    let Some(dump) = lowered(source, "scala.runtime.Statics") else { return };
    assert!(dump.contains("def @scala.runtime.Statics::static f(): int"), "{dump}");
}
