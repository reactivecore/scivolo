object Widejumps {
  var n = 0

  inline def x2(inline body: Unit): Unit = { body; body }
  inline def x16(inline body: Unit): Unit = x2(x2(x2(x2(body))))
  inline def x4096(inline body: Unit): Unit = x16(x16(x16(body)))

  def run(b: Boolean, times: Int): Int = {
    var i = 0
    while (i < times) {
      if (b) {
        x4096 { n += 1 }
      }
      i += 1
    }
    n
  }

  def main(args: Array[String]): Unit = {
    println(run(true, 2))
    println(run(false, 3))
    println(run(true, 1))
  }
}
