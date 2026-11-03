package probe

import scala.quoted.*

object Probe:
  def vals(x: Expr[Int])(using Quotes): Expr[Int] = '{ val a = $x; val b = a * 2; a + b }
  def defs(x: Expr[Int])(using Quotes): Expr[Int] = '{ def f(y: Int): Int = y + $x; f(1) + f(2) }
  def lambda(x: Expr[Int])(using Quotes): Expr[Int] = '{ List(1, 2, 3).map(i => i + $x).sum }
  def function(x: Expr[Int])(using Quotes): Expr[Int] = '{ val f: Int => Int = i => i + $x; f(2) }
  def fold(xs: Expr[List[String]])(using Quotes): Expr[String] = '{ $xs.foldLeft("")((a, b) => a + b) }
  def folded(xs: Expr[List[String]])(using Quotes): Expr[String] =
    '{ $xs.foldLeft[String]("")((a: String, b: String) => a.concat(b)) }
  def matching(x: Expr[Int])(using Quotes): Expr[String] =
    '{ $x match { case 0 => "zero"; case n if n > 9 => "big" + n; case _ => "small" } }
  def higher(x: Expr[Int])(using Quotes): Expr[Int] =
    '{ def g[F[_]](fa: F[Int]): F[Int] = fa; g[List](List($x)).head }
  def alias(x: Expr[Int])(using Quotes): Expr[Int] = '{ type L[X] = List[X]; val l: L[Int] = List($x); l.head }
  def vars(x: Expr[Int])(using Quotes): Expr[Int] = '{ var s = 0; var i = 0; while i < $x do { s = s + i; i = i + 1 }; s }
  def options(x: Expr[Option[Int]])(using Quotes): Expr[Int] =
    '{ $x match { case Some(v) => v; case None => 0 } }
  def alternatives(x: Expr[Int])(using Quotes): Expr[Boolean] =
    '{ $x match { case 1 | 2 => true; case _ => false } }
  def tries(x: Expr[Int])(using Quotes): Expr[Int] =
    '{ try 10 / $x catch { case e: ArithmeticException => 0 } finally println("done") }
  def dollar(x: Expr[Int])(using Quotes): Expr[Int] = '{ val a$1 = $x; val b = a$1 + 1; b }
  def clauses(x: Expr[Int])(using Quotes): Expr[Int] =
    '{ def f[A](a: A)[B](b: B)()(c: Int)(using d: String): Int = c; f[Int](1)[String]("")()(2)(using "") + $x }
  def objects(x: Expr[Int])(using Quotes): Expr[Int] = '{ object Foo { given Int = 43; val y = $x }; Foo.y }
  def classes(x: Expr[Int])(using Quotes): Expr[Int] = '{ class C(val a: Int) { def b = a + $x }; new C(1).b }
  def anonymous(x: Expr[Int])(using Quotes): Expr[String] =
    '{ new Object { override def toString = "a" + $x }.toString }
  def bound(x: Expr[Int])(using Quotes): Expr[Int] =
    '{ class E[T: Ordering](t: T) { def m = implicitly[Ordering[T]].compare(t, t) }; new E(1).m + $x }
  def arrow(x: Expr[Int])(using Quotes): Expr[(Int, Int)] = '{ new Predef.ArrowAssoc($x).->(2) }
  def selves(x: Expr[Int])(using Quotes): Expr[Int] =
    '{ class D(var n: Int) { self => def me: D = self; def it = D.this }; new D($x).me.it.n }
