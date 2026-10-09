//> using scala 3.9.0
class Early extends Late
trait Late:
  val l = 42

trait T:
  val x: Int = 5
class D(override val x: Int) extends T

trait A:
  val a = 1
  println(s"A $a")
trait B extends A:
  override val a = 2
class C extends B

trait U:
  val u: Unit = println("u")
  val n: Null = null
  final val k = 10
class X extends U

object Outer:
  trait N:
    val nv = "n"
  class NC extends N

@main def traitorder(): Unit =
  println(Early().l)
  println(D(7).x)
  println(C().a)
  val x = X()
  println(x.k)
  println(x.n)
  println(Outer.NC().nv)
