object Frames {
  def number(c: Boolean): Any = {
    val n: Any = if c then 1 else 2.5
    n
  }

  def ref(c: Int, args: Array[String]): AnyRef = {
    val r: AnyRef = if c > 0 then "pos" else if c < 0 then args else null
    r
  }

  def entry(c: Boolean): AnyRef = {
    val e =
      if c then new java.util.AbstractMap.SimpleEntry[String, String]("a", "b")
      else new java.util.AbstractMap.SimpleImmutableEntry[String, String]("c", "d")
    e
  }

  def sums(n: Int): Double = {
    var total = 0L
    var d = 0.5
    var i = 0
    while i < n do {
      var j = 0L
      val half = d / 2
      while j < i do {
        total += j
        j += 1L
      }
      d = d + half
      i += 1
    }
    total + d
  }

  def unimplemented: Int = ???

  def guarded(c: Boolean): Int = if (if c then true else ???) then 1 else 2

  def main(args: Array[String]): Unit = {
    println(guarded(true))
    try unimplemented catch { case _: NotImplementedError => println("???") }
    println(number(true))
    println(number(false))
    println(ref(1, args))
    println(ref(-1, args) eq null)
    println(ref(0, args) eq null)
    println(entry(false))
    println(sums(5))
  }
}
