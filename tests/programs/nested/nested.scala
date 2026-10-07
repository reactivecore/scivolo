//> using scala 3.9.0
object Outer:
  var count = 0
  def id(n: Int): Int = n

  class Inner(val x: Int):
    def bump(): Int =
      count = count + x
      count
    def set(): Unit = count = x
    def twice(): Int = id(count) * 2

  object Nested:
    val name = "nested"
    def twice(n: Int) = n * 2
    object Deeper:
      val level = 3
      class Leaf(val tag: String):
        def show = s"leaf $tag at $level"

  case class Pt(a: Int, b: Int)
  def origin = Pt(0, 0)

@main def nested(): Unit =
  val i = new Outer.Inner(3)
  println(i.bump())
  println(i.bump())
  i.set()
  println(i.twice())
  Outer.count = 10
  println(Outer.count)
  println(Outer.Nested.name)
  println(Outer.Nested.twice(4))
  println(Outer.Nested.Deeper.level)
  println(new Outer.Nested.Deeper.Leaf("x").show)
  println(Outer.Pt(1, 2))
  println(Outer.origin == Outer.Pt(0, 0))
