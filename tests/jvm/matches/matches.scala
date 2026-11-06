object Matches {
  def dense(i: Int): String = i match {
    case 1 => "one"
    case 2 | 3 => "two or three"
    case 5 => "five"
    case _ => "many"
  }

  def sparse(i: Int): Int = i match {
    case 1 => 10
    case 100 => 20
    case 10000 => 30
    case _ => 0
  }

  def chars(c: Char): Int = c match {
    case 'a' => 1
    case 'b' | 'c' => 2
    case _ => 3
  }

  def strings(s: String): Int = s match {
    case "foo" => 1
    case "bar" | "baz" => 2
    case "Aa" => 3
    case "BB" => 4
    case _ => 0
  }

  def types(x: Any): String = x match {
    case i: Int if i > 10 => "big int"
    case i: Int => "int " + i
    case s: String => "string " + s
    case _ => "other"
  }

  def first(xs: Array[Int], n: Int): Int = {
    var i = 0
    while (i < xs.length) {
      if (xs(i) > n) return xs(i)
      i += 1
    }
    -1
  }

  def main(args: Array[String]): Unit = {
    println(dense(1) + " " + dense(3) + " " + dense(5) + " " + dense(7))
    println(sparse(100) + sparse(10000) + sparse(3))
    println(chars('a') + chars('c') + chars('z'))
    println(strings("foo") + strings("baz") + strings("Aa") + strings("BB") + strings("qux"))
    println(types(42) + ", " + types(7) + ", " + types("x") + ", " + types(1.5))
    println(first(Array(1, 5, 9), 4) + first(Array(1), 4))
    var n = 0
    for (i <- 0 until 10) n += sparse(i * 100)
    println(n)
  }
}
