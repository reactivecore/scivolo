trait Greeter:
  def greet(): String

trait Sized[T]:
  def size(x: T): Int

object Nums:
  val base = 100

object Lib:
  inline def mk(n: Int): Int =
    class Foo(val k: Int):
      def twice = k * 2
    new Foo(n).twice + 1
  inline def anon(s: String): Greeter = new Greeter:
    import Nums.*
    def greet() = "hi " + s + base
  inline def twice(x: => Int): Int = x + x
  inline def sized[T]: Sized[T] = new Sized[T]:
    def size(x: T): Int = inline x match
      case s: String => s.length
      case _ => -1
  inline def alias(n: Int): Int =
    type I = Int
    val i: I = n
    i + 1

object Other:
  def go(): Int = Lib.mk(5)

@main def TBinlinelocal(): Unit =
  class Local(val v: Int)
  println(Lib.mk(3))
  println(Lib.anon("there").greet())
  println(Other.go())
  println(new Local(4).v)
  println(Lib.twice(new Greeter { def greet() = "x" }.greet().length))
  println(Lib.sized[String].size("four"))
  println(Lib.alias(6))
