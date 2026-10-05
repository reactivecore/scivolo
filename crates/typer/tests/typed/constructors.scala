class Counter(start: Int):
  var n = start

class Cell[T](val value: T)

class Pair[A, B](val a: A, val b: B)

object Main:
  def run(): Unit =
    val c = Counter(3)
    val d = new Cell(5)
    val e = Cell("x")
    val p = new Pair(1, "y")
    val t = Thread("name")
    val sb = StringBuilder()
    println(c.n + d.value + e.value + p.b + t.getName + sb.toString)
