class Cell {
  var n: Int = _
  var s: String = _
  var d: Double = _
  var b: Boolean = _
}

trait Counter {
  var hits: Int = _
}

object wildcardvars extends Counter {
  var count: Long = _
  def main(args: Array[String]): Unit = {
    val c = new Cell
    println(s"${c.n} ${c.s} ${c.d} ${c.b} $count $hits")
    c.n = 3
    count += 1
    hits += 2
    println(s"${c.n} $count $hits")
  }
}
