//> using scala 3.9.0
object D:
  def twice(f: String => String, x: String): String = f(f(x))
  def plain(n: Int): Int =
    def square(x: Int): Int = x * x
    square(n) + square(n + 1)
  def recursive(n: Int): Int =
    def fact(k: Int): Int = if k <= 1 then 1 else k * fact(k - 1)
    fact(n)
  def capturing(k: Int): Int =
    def add(x: Int): Int = x + k
    add(1) + add(2)
  def nested(k: String): String =
    def outer(a: String): String =
      def inner(b: String): String = b + k
      inner(a) + inner(a)
    outer("n")
  def inLambda(k: String): String =
    def bang(s: String): String = s + k
    twice(x => bang(x), "l")
  def g(s: String): String = s + "g"
  def explicitThis(): String = twice(x => this.g(x), "t")
  def mutual(n: Int, k: Int): Boolean =
    def even(i: Int): Boolean = if i == 0 then true else odd(i - 1)
    def odd(i: Int): Boolean = if i == 0 then false else even(i - k)
    even(n)
  def shadow(x: Int): Int =
    def h(): Int =
      def x(): Int = 1
      x()
    def g(): Int = x
    g() + h()
  def ap(f: Int => Int, x: Int): Int = f(x)
  def defInLambda(k: Int): Int = ap(x => { def h(y: Int): Int = y + k; ap(z => h(z), x) }, 1)
  def generic(k: Int): String =
    def pair[A](a: A): List[A] = List(a, a)
    def either[A, B](a: A, b: B, first: Boolean): Any = if first then a else b
    def tagged[T](t: T): String = s"$t$k"
    pair("p").mkString + either(1, "b", false) + tagged(2.5) + pair(k).sum

class C(p: String):
  def twice(f: String => String, x: String): String = f(f(x))
  def field(): String = twice(s => s + p, "c")
  def ap(f: Int => Int, x: Int): Int = f(x)
  def pure(k: Int): Int =
    def sq(y: Int): Int = y * k
    ap(x => sq(x), 3)

trait T:
  def n(k: Int): Int =
    def g(x: Int): Int = x + k
    g(2)

class TI extends T

@main def localdefs(): Unit =
  def inc(n: Int) = n + 1
  val g = (n: Int) => inc(n)
  println(g(1))
  println(D.plain(3))
  println(D.recursive(5))
  println(D.capturing(10))
  println(D.nested("."))
  println(D.inLambda("!"))
  println(D.explicitThis())
  println(D.mutual(6, 1))
  println(D.mutual(5, 1))
  println(D.shadow(5))
  println(D.defInLambda(10))
  println(D.generic(3))
  println(new C("+").field())
  println(new C("+").pure(4))
  println(new TI().n(3))
