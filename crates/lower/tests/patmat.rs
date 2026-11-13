//! The NIR of matches per pattern form, as Scala Native 0.5.12 generates it for the same
//! snippets.

mod snippet;

use snippet::{assert_defs, lowered};

#[test]
fn type_patterns_and_their_alternatives() {
    let source = r#"object T {
  def typeTest(x: Any): Int = x match {
    case s: String => s.length
    case i: Int => i
    case _ => 0
  }
  def alternatives(x: Any): Int = x match {
    case _: String | _: Int => 1
    case _ => 0
  }
}"#;
    let Some(dump) = lowered(source, "T$") else { return };
    assert_defs(
        &dump,
        &[
            r"
def @T$::typeTest(java.lang.Object): int : (@T$, @java.lang.Object) => int {
%3(%2 <this> : @T$, %1 <x> : @java.lang.Object):
  jump %4
%4:
  1: %11 = is[@java.lang.String] %1 <x> : @java.lang.Object
  if %11 : bool then %7 else %8
%7:
  2: %12 <s> = as[@java.lang.String] %1 <x> : @java.lang.Object
  3: %13 = method %12 <s> : @java.lang.String, length(): int
  3: %14 = call[(@java.lang.String) => int] %13 : ptr(%12 <s> : @java.lang.String)
  jump %5(%14 : int)
%8:
  jump %9
%9:
  1: %20 = is[@java.lang.Integer] %1 <x> : @java.lang.Object
  if %20 : bool then %16 else %17
%16:
  4: %21 <i> = unbox[@java.lang.Integer] %1 <x> : @java.lang.Object
  jump %5(%21 <i> : int)
%17:
  jump %18
%18:
  jump %5(int 0)
%5(%6 : int):
  ret %6 : int
}",
            r"
def @T$::alternatives(java.lang.Object): int : (@T$, @java.lang.Object) => int {
%3(%2 <this> : @T$, %1 <x> : @java.lang.Object):
  jump %4
%4:
  jump %7
%7:
  jump %10
%10:
  3: %17 = is[@java.lang.String] %1 <x> : @java.lang.Object
  if %17 : bool then %13 else %14
%13:
  jump %11
%14:
  jump %15
%15:
  3: %23 = is[@java.lang.Integer] %1 <x> : @java.lang.Object
  if %23 : bool then %19 else %20
%19:
  jump %11
%20:
  jump %21
%21:
  jump %8
%11:
  jump %5(int 1)
%8:
  jump %5(int 0)
%5(%6 : int):
  ret %6 : int
}",
        ],
    );
}

#[test]
fn case_class_boolean_and_option_extractors_and_binders() {
    let source = r#"case class Pt(x: Int, y: Int)
object Even { def unapply(i: Int): Boolean = i % 2 == 0 }
object Half { def unapply(i: Int): Option[Int] = if i % 2 == 0 then Some(i / 2) else None }
object X {
  def caseClass(p: Any): Int = p match {
    case Pt(a, b) => a + b
    case _ => 0
  }
  def boolean(i: Int): String = i match {
    case Even() => "even"
    case _ => "odd"
  }
  def option(i: Int): Int = i match {
    case Half(h) => h
    case _ => -1
  }
  def binder(x: Any): Any = x match {
    case p @ Pt(_, _) => p
    case _ => null
  }
}"#;
    let Some(dump) = lowered(source, "X$") else { return };
    assert_defs(
        &dump,
        &[
            r"
def @X$::caseClass(java.lang.Object): int : (@X$, @java.lang.Object) => int {
%3(%2 <this> : @X$, %1 <p> : @java.lang.Object):
  jump %4
%4:
  1: %11 = is[@Pt] %1 <p> : @java.lang.Object
  if %11 : bool then %7 else %8
%7:
  2: %12 = module @Pt$
  2: %13 = as[@Pt] %1 <p> : @java.lang.Object
  2: %14 = method %12 : !?@Pt$, unapply(Pt): Pt
  2: %15 <x7> = call[(@Pt$, @Pt) => @Pt] %14 : ptr(%12 : !?@Pt$, %13 : @Pt)
  2: %16 = method %15 <x7> : @Pt, _1(): int
  2: %17 <x8> = call[(@Pt) => int] %16 : ptr(%15 <x7> : @Pt)
  2: %18 = method %15 <x7> : @Pt, _2(): int
  2: %19 <x9> = call[(@Pt) => int] %18 : ptr(%15 <x7> : @Pt)
  3: %20 = iadd[int] %17 <x8> : int, %19 <x9> : int
  jump %5(%20 : int)
%8:
  jump %9
%9:
  jump %5(int 0)
%5(%6 : int):
  ret %6 : int
}",
            r#"
def @X$::boolean(int): java.lang.String : (@X$, int) => @java.lang.String {
%3(%2 <this> : @X$, %1 <i> : int):
  jump %4
%4:
  1: %11 = module @Even$
  1: %12 = method %11 : !?@Even$, unapply(int): bool
  1: %13 = call[(@Even$, int) => bool] %12 : ptr(%11 : !?@Even$, %1 <i> : int)
  if %13 : bool then %7 else %8
%7:
  jump %5("even")
%8:
  jump %9
%9:
  jump %5("odd")
%5(%6 : @java.lang.String):
  ret %6 : @java.lang.String
}"#,
            r"
def @X$::option(int): int : (@X$, int) => int {
%3(%2 <this> : @X$, %1 <i> : int):
  jump %4
%4:
  2: %7 = module @Half$
  2: %8 = method %7 : !?@Half$, unapply(int): scala.Option
  2: %9 <x12> = call[(@Half$, int) => @scala.Option] %8 : ptr(%7 : !?@Half$, %1 <i> : int)
  2: %14 = method %9 <x12> : @scala.Option, isEmpty(): bool
  2: %15 = call[(@scala.Option) => bool] %14 : ptr(%9 <x12> : @scala.Option)
  2: %16 = xor[bool] true, %15 : bool
  if %16 : bool then %10 else %11
%10:
  3: %17 = method %9 <x12> : @scala.Option, get(): java.lang.Object
  3: %18 = call[(@scala.Option) => @java.lang.Object] %17 : ptr(%9 <x12> : @scala.Option)
  3: %19 <x13> = unbox[@java.lang.Integer] %18 : @java.lang.Object
  jump %5(%19 <x13> : int)
%11:
  jump %12
%12:
  jump %5(int -1)
%5(%6 : int):
  ret %6 : int
}",
            r"
def @X$::binder(java.lang.Object): java.lang.Object : (@X$, @java.lang.Object) => @java.lang.Object {
%3(%2 <this> : @X$, %1 <x> : @java.lang.Object):
  jump %4
%4:
  1: %11 = is[@Pt] %1 <x> : @java.lang.Object
  if %11 : bool then %7 else %8
%7:
  2: %12 = module @Pt$
  2: %13 = as[@Pt] %1 <x> : @java.lang.Object
  2: %14 = method %12 : !?@Pt$, unapply(Pt): Pt
  2: %15 <x17> = call[(@Pt$, @Pt) => @Pt] %14 : ptr(%12 : !?@Pt$, %13 : @Pt)
  2: %16 = method %15 <x17> : @Pt, _1(): int
  2: %17 <x18> = call[(@Pt) => int] %16 : ptr(%15 <x17> : @Pt)
  2: %18 = method %15 <x17> : @Pt, _2(): int
  2: %19 <x19> = call[(@Pt) => int] %18 : ptr(%15 <x17> : @Pt)
  2: %20 <p> = as[@Pt] %1 <x> : @java.lang.Object
  jump %5(%20 <p> : @Pt)
%8:
  jump %9
%9:
  jump %5(null)
%5(%6 : @Pt):
  ret %6 : @Pt
}",
        ],
    );
}

#[test]
fn guards_long_and_char_literals_and_match_errors() {
    let source = r#"object L {
  def guard(i: Int): Int = i match {
    case n if n > 10 => n
    case _ => 0
  }
  def long(l: Long): Int = l match {
    case 1L => 1
    case _ => 0
  }
  def char(c: Char): Int = c match {
    case 'a' => 1
    case 'b' => 2
    case _ => 0
  }
  def partial(i: Int): Int = i match {
    case 1 => 1
  }
}"#;
    let Some(dump) = lowered(source, "L$") else { return };
    assert_defs(
        &dump,
        &[
            r"
def @L$::guard(int): int : (@L$, int) => int {
%3(%2 <this> : @L$, %1 <i> : int):
  jump %4
%4:
  2: %11 = sgt[int] %1 <i> : int, int 10
  if %11 : bool then %7 else %8
%7:
  jump %5(%1 <i> : int)
%8:
  jump %9
%9:
  jump %5(int 0)
%5(%6 : int):
  ret %6 : int
}",
            r"
def @L$::long(long): int : (@L$, long) => int {
%3(%2 <this> : @L$, %1 <l> : long):
  jump %4
%4:
  1: %11 = ieq[long] long 1, %1 <l> : long
  if %11 : bool then %7 else %8
%7:
  jump %5(int 1)
%8:
  jump %9
%9:
  jump %5(int 0)
%5(%6 : int):
  ret %6 : int
}",
            r"
def @L$::char(char): int : (@L$, char) => int {
%3(%2 <this> : @L$, %1 <c> : char):
  jump %4
%4:
  1: %11 = zext[int] char 97
  1: %12 = zext[int] %1 <c> : char
  1: %13 = ieq[int] %11 : int, %12 : int
  if %13 : bool then %7 else %8
%7:
  jump %5(int 1)
%8:
  jump %9
%9:
  1: %19 = zext[int] char 98
  1: %20 = zext[int] %1 <c> : char
  1: %21 = ieq[int] %19 : int, %20 : int
  if %21 : bool then %15 else %16
%15:
  jump %5(int 2)
%16:
  jump %17
%17:
  jump %5(int 0)
%5(%6 : int):
  ret %6 : int
}",
            r"
def @L$::partial(int): int : (@L$, int) => int {
%3(%2 <this> : @L$, %1 <i> : int):
  jump %4
%4:
  1: %11 = ieq[int] int 1, %1 <i> : int
  if %11 : bool then %7 else %8
%7:
  jump %5(int 1)
%8:
  jump %9
%9:
  1: %13 = classalloc @scala.MatchError
  1: %14 = box[@java.lang.Integer] %1 <i> : int
  1: %15 = call[(@scala.MatchError, @java.lang.Object) => unit] @scala.MatchError::<init>(java.lang.Object) : ptr(%13 : !?@scala.MatchError, %14 : !?@java.lang.Integer)
  throw %13 : !?@scala.MatchError
%5(%6 : int):
  ret %6 : int
}",
        ],
    );
}

#[test]
fn tuple_sequence_and_cons_patterns() {
    let source = r#"object Q {
  def tuple(t: (Int, String)): String = t match {
    case (1, s) => s
    case _ => ""
  }
  def seq(xs: Seq[Int]): Int = xs match {
    case Seq(a, b) => a + b
    case Seq(a, _*) => a
    case _ => 0
  }
  def list(xs: List[Int]): Int = xs match {
    case h :: _ => h
    case Nil => 0
  }
}"#;
    let Some(dump) = lowered(source, "Q$") else { return };
    assert_defs(
        &dump,
        &[
            r#"
def @Q$::tuple(scala.Tuple2): java.lang.String : (@Q$, @scala.Tuple2) => @java.lang.String {
%3(%2 <this> : @Q$, %1 <t> : @scala.Tuple2):
  jump %4
%4:
  1: %15 = ine[@java.lang.Object] %1 <t> : @scala.Tuple2, null
  if %15 : bool then %11 else %12
%11:
  1: %16 = method %1 <t> : @scala.Tuple2, _1(): java.lang.Object
  1: %17 = call[(@scala.Tuple2) => @java.lang.Object] %16 : ptr(%1 <t> : @scala.Tuple2)
  1: %18 = unbox[@java.lang.Integer] %17 : @java.lang.Object
  1: %19 = ieq[int] int 1, %18 : int
  jump %13(%19 : bool)
%12:
  jump %13(false)
%13(%14 : bool):
  if %14 : bool then %7 else %8
%7:
  2: %20 = method %1 <t> : @scala.Tuple2, _2(): java.lang.Object
  2: %21 = call[(@scala.Tuple2) => @java.lang.Object] %20 : ptr(%1 <t> : @scala.Tuple2)
  2: %22 <s> = as[@java.lang.String] %21 : @java.lang.Object
  jump %5(%22 <s> : @java.lang.String)
%8:
  jump %9
%9:
  jump %5("")
%5(%6 : @java.lang.String):
  ret %6 : @java.lang.String
}"#,
            r"
def @Q$::seq(scala.collection.immutable.Seq): int : (@Q$, @scala.collection.immutable.Seq) => int {
%3(%2 <this> : @Q$, %1 <xs> : @scala.collection.immutable.Seq):
  jump %4
%4:
  1: %11 = ine[@java.lang.Object] %1 <xs> : @scala.collection.immutable.Seq, null
  if %11 : bool then %7 else %8
%7:
  2: %12 = module @scala.package$
  2: %13 = method %12 : !?@scala.package$, Seq(): scala.collection.immutable.Seq$
  2: %14 = call[(@scala.package$) => @scala.collection.immutable.Seq$] %13 : ptr(%12 : !?@scala.package$)
  2: %15 = method %14 : @scala.collection.immutable.Seq$, unapplySeq(scala.collection.SeqOps): scala.collection.SeqOps
  2: %16 <x8> = call[(@scala.collection.SeqFactory, @scala.collection.SeqOps) => @scala.collection.SeqOps] %15 : ptr(%14 : @scala.collection.immutable.Seq$, %1 <xs> : @scala.collection.immutable.Seq)
  2: %21 = module @scala.collection.SeqFactory$UnapplySeqWrapper$
  2: %22 = method %21 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, lengthCompare$extension(scala.collection.SeqOps, int): int
  2: %23 = call[(@scala.collection.SeqFactory$UnapplySeqWrapper$, @scala.collection.SeqOps, int) => int] %22 : ptr(%21 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, %16 <x8> : @scala.collection.SeqOps, int 2)
  2: %24 = ieq[int] %23 : int, int 0
  if %24 : bool then %17 else %18
%17:
  3: %25 = module @scala.collection.SeqFactory$UnapplySeqWrapper$
  3: %26 = method %25 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, apply$extension(scala.collection.SeqOps, int): java.lang.Object
  3: %27 = call[(@scala.collection.SeqFactory$UnapplySeqWrapper$, @scala.collection.SeqOps, int) => @java.lang.Object] %26 : ptr(%25 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, %16 <x8> : @scala.collection.SeqOps, int 0)
  3: %28 <x9> = unbox[@java.lang.Integer] %27 : @java.lang.Object
  3: %29 = module @scala.collection.SeqFactory$UnapplySeqWrapper$
  3: %30 = method %29 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, apply$extension(scala.collection.SeqOps, int): java.lang.Object
  3: %31 = call[(@scala.collection.SeqFactory$UnapplySeqWrapper$, @scala.collection.SeqOps, int) => @java.lang.Object] %30 : ptr(%29 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, %16 <x8> : @scala.collection.SeqOps, int 1)
  3: %32 <x10> = unbox[@java.lang.Integer] %31 : @java.lang.Object
  4: %33 = iadd[int] %28 <x9> : int, %32 <x10> : int
  jump %5(%33 : int)
%18:
  jump %19
%19:
  2: %39 = module @scala.collection.SeqFactory$UnapplySeqWrapper$
  2: %40 = method %39 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, lengthCompare$extension(scala.collection.SeqOps, int): int
  2: %41 = call[(@scala.collection.SeqFactory$UnapplySeqWrapper$, @scala.collection.SeqOps, int) => int] %40 : ptr(%39 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, %16 <x8> : @scala.collection.SeqOps, int 1)
  2: %42 = sge[int] %41 : int, int 0
  if %42 : bool then %35 else %36
%35:
  5: %43 = module @scala.collection.SeqFactory$UnapplySeqWrapper$
  5: %44 = method %43 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, apply$extension(scala.collection.SeqOps, int): java.lang.Object
  5: %45 = call[(@scala.collection.SeqFactory$UnapplySeqWrapper$, @scala.collection.SeqOps, int) => @java.lang.Object] %44 : ptr(%43 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, %16 <x8> : @scala.collection.SeqOps, int 0)
  5: %46 <x7> = unbox[@java.lang.Integer] %45 : @java.lang.Object
  5: %47 = module @scala.collection.SeqFactory$UnapplySeqWrapper$
  5: %48 = method %47 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, drop$extension(scala.collection.SeqOps, int): scala.collection.immutable.Seq
  5: %49 <x6> = call[(@scala.collection.SeqFactory$UnapplySeqWrapper$, @scala.collection.SeqOps, int) => @scala.collection.immutable.Seq] %48 : ptr(%47 : !?@scala.collection.SeqFactory$UnapplySeqWrapper$, %16 <x8> : @scala.collection.SeqOps, int 1)
  jump %5(%46 <x7> : int)
%36:
  jump %37
%37:
  jump %9
%8:
  jump %9
%9:
  jump %5(int 0)
%5(%6 : int):
  ret %6 : int
}",
            r"
def @Q$::list(scala.collection.immutable.List): int : (@Q$, @scala.collection.immutable.List) => int {
%3(%2 <this> : @Q$, %1 <xs> : @scala.collection.immutable.List):
  jump %4
%4:
  1: %11 = is[@scala.collection.immutable.$colon$colon] %1 <xs> : @scala.collection.immutable.List
  if %11 : bool then %7 else %8
%7:
  2: %12 = as[@scala.collection.immutable.$colon$colon] %1 <xs> : @scala.collection.immutable.List
  2: %13 = method %12 : @scala.collection.immutable.$colon$colon, next(): scala.collection.immutable.List
  2: %14 <x14> = call[(@scala.collection.immutable.$colon$colon) => @scala.collection.immutable.List] %13 : ptr(%12 : @scala.collection.immutable.$colon$colon)
  2: %15 = as[@scala.collection.immutable.$colon$colon] %1 <xs> : @scala.collection.immutable.List
  2: %16 = method %15 : @scala.collection.immutable.$colon$colon, head(): java.lang.Object
  2: %17 = call[(@scala.collection.immutable.$colon$colon) => @java.lang.Object] %16 : ptr(%15 : @scala.collection.immutable.$colon$colon)
  2: %18 <h> = unbox[@java.lang.Integer] %17 : @java.lang.Object
  jump %5(%18 <h> : int)
%8:
  jump %9
%9:
  1: %28 = module @scala.package$
  1: %29 = method %28 : !?@scala.package$, Nil(): scala.collection.immutable.Nil$
  1: %30 = call[(@scala.package$) => @scala.collection.immutable.Nil$] %29 : ptr(%28 : !?@scala.package$)
  1: %31 = ieq[@java.lang.Object] %30 : @scala.collection.immutable.Nil$, null
  if %31 : bool then %24 else %25
%24:
  1: %32 = ieq[@java.lang.Object] %1 <xs> : @scala.collection.immutable.List, null
  jump %26(%32 : bool)
%25:
  1: %33 = method %30 : @scala.collection.immutable.Nil$, equals(java.lang.Object): bool
  1: %34 = call[(@java.lang.Object, @java.lang.Object) => bool] %33 : ptr(%30 : @scala.collection.immutable.Nil$, %1 <xs> : @scala.collection.immutable.List)
  jump %26(%34 : bool)
%26(%27 : bool):
  if %27 : bool then %20 else %21
%20:
  jump %5(int 0)
%21:
  jump %22
%22:
  1: %36 = classalloc @scala.MatchError
  1: %37 = call[(@scala.MatchError, @java.lang.Object) => unit] @scala.MatchError::<init>(java.lang.Object) : ptr(%36 : !?@scala.MatchError, %1 <xs> : @scala.collection.immutable.List)
  throw %36 : !?@scala.MatchError
%5(%6 : int):
  ret %6 : int
}",
        ],
    );
}

#[test]
fn catch_cases_with_a_type_test_and_a_guard() {
    let source = r"object K {
  def caught(f: () => Int): Int = try f() catch {
    case _: IllegalStateException => 1
    case e: RuntimeException if e.getMessage == null => 2
  }
}";
    let Some(dump) = lowered(source, "K$") else { return };
    assert_defs(
        &dump,
        &[r"
def @K$::caught(scala.Function0): int : (@K$, @scala.Function0) => int {
%3(%2 <this> : @K$, %1 <f> : @scala.Function0):
  jump %5
%5:
  1: %10 = method %1 <f> : @scala.Function0, apply$mcI$sp(): int unwind %9 : @java.lang.Throwable to %4(%9 : @java.lang.Throwable)
  1: %12 = call[(@scala.Function0) => int] %10 : ptr(%1 <f> : @scala.Function0) unwind %11 : @java.lang.Throwable to %4(%11 : @java.lang.Throwable)
  jump %6(%12 : int)
%4(%7 : @java.lang.Throwable):
  2: %13 = is[@java.lang.IllegalStateException] %7 : @java.lang.Throwable
  if %13 : bool then %14 else %15
%14:
  jump %6(int 1)
%15:
  2: %19 = is[@java.lang.Throwable] %7 : @java.lang.Throwable
  if %19 : bool then %20 else %21
%20:
  4: %24 <ex1> = as[@java.lang.Throwable] %7 : @java.lang.Throwable
  jump %25
%25:
  5: %32 = is[@java.lang.RuntimeException] %24 <ex1> : @java.lang.Throwable
  if %32 : bool then %28 else %29
%28:
  6: %33 <e> = as[@java.lang.RuntimeException] %24 <ex1> : @java.lang.Throwable
  6: %38 = method %33 <e> : @java.lang.RuntimeException, getMessage(): java.lang.String
  6: %39 = call[(@java.lang.Throwable) => @java.lang.String] %38 : ptr(%33 <e> : @java.lang.RuntimeException)
  6: %40 = ieq[@java.lang.Object] %39 : @java.lang.String, null
  if %40 : bool then %34 else %35
%34:
  jump %26(int 2)
%35:
  jump %36
%36:
  jump %30
%29:
  jump %30
%30:
  throw %24 <ex1> : @java.lang.Throwable
%26(%27 : int):
  jump %6(%27 : int)
%21:
  throw %7 : @java.lang.Throwable
%6(%8 : int):
  ret %8 : int
}"],
    );
}
