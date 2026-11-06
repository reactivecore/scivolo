class Point(val x: Int, val y: Int) {
  override def toString: String = "(" + x + ", " + y + ")"
}

object Concat {
  def describe(name: String, n: Int, d: Double, c: Char, z: Boolean, l: Long): String =
    s"$name has $n items at $d, flag $c/$z, total $l"

  def main(args: Array[String]): Unit = {
    val p = new Point(1, 2)
    println("p = " + p)
    println(describe("box", 3, 1.5, 'x', true, 42L))
    val o: Any = p
    println(s"any: $o, null: ${null}")
    println("a" + 1 + 2)
    println(s"${p.x + p.y}")
    val s: String = null
    println(s + "!")
    println(o == p)
    println(o != "x")
    println(o.## == p.hashCode)
    val i: Any = 3
    println(i == 3)
    println(i.##)
    if (args.length > 5) throw new IllegalStateException("many")
    println("done")
  }
}
