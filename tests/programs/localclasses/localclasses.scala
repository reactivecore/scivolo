//> using scala 3.9.0
object Shapes:
  def area(w: Int): Int =
    class Rect(h: Int):
      def size = w * h
    Rect(2).size + new Rect(3).size

  def greeter(name: String): Runnable = new Runnable:
    def run(): Unit = println(s"hello $name")

  def plain(): AnyRef = new Object:
    override def toString = "plain"

  def counter(start: Int, step: Int): AnyRef = new Object:
    var current = start
    def next(): Int =
      val value = current
      current += step
      value
    override def toString = s"${next()},${next()},${next()}"

class Box(val n: Int):
  def describe(label: String): String =
    class Tag:
      override def toString = s"$label#"
    Tag().toString + n

trait Check[-X]:
  def check(x: X): Boolean

class Outer:
  class Inner

def applied(o: Outer): Boolean =
  ((p: Outer) => {
    class Always extends Check[p.Inner]:
      def check(x: p.Inner) = true
    new Always: Check[Null]
  }).apply(o).check(null)

@main def localclasses(): Unit =
  println(applied(Outer()))
  println(((k: Int) => { class Plus { def get = k + 1 }; Plus().get })(2))
  println(Shapes.area(5))
  Shapes.greeter("world").run()
  println(Shapes.plain())
  println(Shapes.counter(1, 2))
  println(Box(7).describe("box"))
