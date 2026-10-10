//> using scala 3.9.0
object Test:
  def plain(): Int =
    object counter:
      println("counter")
      var n = 0
      def next(): Int =
        n += 1
        n
    counter.next() + counter.next()

  def captured(k: Int): Int =
    def scale(x: Int): Int = x * k
    object box:
      val v: Int = k * k
      def twice: Int = 2 * k
    class Pair(a: Int):
      def sum: Int = a + k
    box.v + box.twice + new Pair(1).sum + scale(2)

  def compared(): Boolean =
    object A
    def same(x: Any): Boolean = x == A
    same(A)

  val field: Int =
    lazy val y = 4
    y + 1

class Spec:
  def should(body: => Unit): Unit = body
  should {
    object Days:
      val first = "mon"
    println(Days.first)
  }

@main def localobjects(): Unit =
  new Spec()
  println(Test.field)
  println(Test.plain())
  println(Test.captured(3))
  println(Test.compared())
