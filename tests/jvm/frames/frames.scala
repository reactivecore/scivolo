object Frames {
  def number(c: Boolean): Any = {
    val n: Any = if c then 1 else 2.5
    n
  }

  def ref(c: Int, args: Array[String]): AnyRef = {
    val r: AnyRef = if c > 0 then "pos" else if c < 0 then args else null
    r
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

  def main(args: Array[String]): Unit = {
    println(number(true))
    println(number(false))
    println(ref(1, args))
    println(ref(-1, args) eq null)
    println(ref(0, args) eq null)
    println(sums(5))
  }
}
