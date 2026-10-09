//> using scala 3.9.0
class Outer(val base: Int):
  def twice(n: Int): Int = n * 2
  object Inner:
    val v: Int = twice(base)
    def show(k: Int): Int = twice(base + k)
  object Plain:
    def seven(): Int = 7
  def run(): Int = Inner.show(1) + Inner.v + Plain.seven()

trait Named:
  def name: String
  object Greeting:
    val text: String = "hello " + name
    def shout(): String = text + "!"

class Person(val name: String) extends Named

object Top:
  class C(val b: Int):
    object In:
      lazy val lv: Int = b + 1
      var w: Int = b
      def f(): Int = { w += lv; w }

@main def nestedobjects(): Unit =
  val o = Outer(3)
  println(o.run())
  println(o.Inner.v)
  println(o.Inner eq o.Inner)
  println(Outer(4).Inner.v)
  val c = Top.C(3)
  println(c.In.f())
  println(c.In.f())
  val p = Person("ann")
  println(p.Greeting.shout())
  println(p.Greeting eq p.Greeting)
