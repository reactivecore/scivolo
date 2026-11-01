trait A { type T; def arr: Array[T] }
class B extends A { type T = Int; def arr: Array[T] = Array(1, 2) }
class C extends B
class D extends C

trait A2 { type U; type T; def get: Array[T] }
class B2 extends A2 { type U = Long; type T = U; def get: Array[T] = Array(7L) }
class C2 extends B2

trait H { type F[X]; def put(x: F[Int]): String; def mk: Array[F[Int]] }
class HI extends H { type F[X] = Int; def put(x: Int): String = "HI" + x; def mk: Array[Int] = Array(1) }
abstract class M extends H { type F[X] <: Seq[X] }
class N extends M { type F[X] = List[X]; def put(x: List[Int]): String = "N" + x.sum; def mk: Array[List[Int]] = Array(List(2)) }
class O extends N { override def put(x: F[Int]): String = "O" + x.length }

trait V { type F[+X] <: AnyRef; def get: F[String] }
class VI extends V { type F[+X] = List[X]; def get: F[String] = List("v") }
class VJ extends VI

trait G { type F[-X, +Y] >: Null <: AnyRef; def ap(f: F[Int, String]): Array[F[Int, String]] }
class GI extends G { type F[-X, +Y] = X => Y; def ap(f: Int => String): Array[Int => String] = Array(f) }

class Box[P](p: P) extends H {
  type F[X] = (P, X)
  def put(x: (P, Int)): String = "Box" + x
  def mk: Array[F[Int]] = Array((p, 1))
}
class Deep extends Box[String]("d") { def first(xs: Array[F[Int]]): String = xs(0)._1 }

object hk {
  def same(a: A)(xs: Array[a.T]): Array[a.T] = xs
  def sameA2(a: A2)(xs: Array[a.T]): Array[a.T] = xs
  def sameH(h: H)(xs: Array[h.F[Int]]): Array[h.F[Int]] = xs
  def sameV(v: V)(xs: Array[v.F[String]]): Array[v.F[String]] = xs
}

@main def hkoverrides(): Unit =
  val d = new D
  val ys = hk.same(d)(d.arr)
  println(ys(0) + ys(1))
  val c2 = new C2
  val zs = hk.sameA2(c2)(c2.get)
  println(zs(0) + 1L)
  val hi = new HI
  val hs = hk.sameH(hi)(hi.mk)
  println(hs(0) + 1)
  val h0: H = hi
  println(h0.put(5.asInstanceOf[h0.F[Int]]))
  val o = new O
  println(o.put(List(1, 2)))
  println(o.mk(0))
  val vj = new VJ
  val vs = hk.sameV(vj)(Array(vj.get))
  println(vs(0).head + (vj: V).get)
  val g: G = new GI
  println(g.ap(null).length)
  val deep = new Deep
  println(deep.first(deep.mk) + deep.put(("e", 2)))
