case class Circle(r: Double)

object Main:
  def lit(x: Int): String = x match
    case 0 => "zero"
    case 1 | 2 => "small"
    case n if n < 0 => "neg"
    case _ => "big"

  def typ(x: Any): Int = x match
    case i: Int => i
    case s: String => s.length
    case s: java.lang.Double => 1
    case _ => 0

  def cls(c: Circle): Double = c match
    case Circle(r) => r

  def opt(o: Option[Int]): Int = o match
    case Some(v) => v
    case None => 0

  def nothing(x: Int) = x match
    case 1 => ???
    case _ => ???

  val b = 1 match
    case a => a
  val (p, q) = (1, "x")

  val cases = List(("a", 1, 2))
  def run(): Unit =
    for (name, a, b) <- cases do println(name)
    cases.foreach { case (n, x, y) => println(n) }
    val xs = cases.flatMap(t => List(t))
    val arr = new Array[Int](3)
    arr(1) = 5
