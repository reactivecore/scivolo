//> using scala 3.9.0
object acc:
  trait Meter:
    private[acc] val step = 3
    private[Meter] var count = 0
    private[acc] def tick(): Int =
      count += step
      count

  class Gauge extends Meter

  class Point private[acc] (val x: Int):
    protected[acc] def twice: Int = x * 2
    private def secret: Int = Point.offset + x

  object Point:
    private def offset = 100
    def make(x: Int): Point = new Point(x)
    def reveal(p: Point): Int = p.secret

  case class Box private (v: Int)

  object Box:
    def of(v: Int): Box = Box(v + 1)

  private[acc] class Hidden:
    def name: String = "hidden"

  def run(): Unit =
    val g = Gauge()
    g.tick()
    println(g.tick())
    val p = new Point(4)
    println(p.twice)
    println(Point.reveal(Point.make(5)))
    println(Box.of(1))
    println(Hidden().name)

@main def qualifiedaccess(): Unit =
  acc.run()
