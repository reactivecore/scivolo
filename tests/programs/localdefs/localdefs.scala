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

class C(p: String):
  def twice(f: String => String, x: String): String = f(f(x))
  def field(): String = twice(s => s + p, "c")

@main def localdefs(): Unit =
  println(D.plain(3))
  println(D.recursive(5))
  println(D.capturing(10))
  println(D.nested("."))
  println(D.inLambda("!"))
  println(D.explicitThis())
  println(new C("+").field())
