trait Sh[-A] { def show(a: A): String }
class Box[T]
class Mid[U] extends Box[U]
class Marker
object Marker { given Sh[Any] = _ => "Marker" }
class Bnd[T <: Marker] extends Box[T]

class Outer(val name: String) {
  class Inner
  object Inner { given Sh[Any] = _ => "Inner of " + name }
  class C extends Mid[Inner]
  def viaParam[T <: A.C](t: T): String = summon[Sh[T]].show(t)
  def mine(c: C): String = summon[Sh[C]].show(c)
  def ofA(c: A.C): String = summon[Sh[A.C]].show(c)
}
object A extends Outer("A")
class Plain extends Bnd[Nothing]

@main def parentargscope(): Unit =
  val x = Outer("x")
  println(x.viaParam(A.C()))
  println(x.mine(x.C()))
  println(summon[Sh[A.C]].show(A.C()))
  println(summon[Sh[Plain]].show(Plain()))
  println(x.ofA(A.C()))
