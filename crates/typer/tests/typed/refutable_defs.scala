class B[T]
object B:
  def unapply[T](x: Any): Option[B[T]] = None

object Main:
  val x0 = 0
  def run(x: Object): Int =
    val Array(who, what*) = "a b".split(" ")
    val List(_*) = List(1, 2)
    val Seq(1, 2) = LazyList(1, 2)
    val s @ (_s: String) = x
    val (c, List(`x0`, _, a), b) = (0, List(0, 1, 2), 3)
    val Some(y) = Option(3)
    val n :: rest = List(4, 5)
    val (p, q) = (1, "q")
    val Some(z) = Some(6)
    val l @ List(m) = (List(7): @unchecked)
    who.length + what.size + s.length + c + a + b + y + n + rest.size + p + q.length + z + l.size + m

  def nul(): Unit =
    val B(_) = null
