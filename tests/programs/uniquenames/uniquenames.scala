class Digits(n: Int) {
  var digits: Array[Int] = Array.fill(n)(n)

  def shift(k: Int): Unit = digits(n - 1) >>>= k

  def triple(s: String): (Int, Int, Int) = (s.length, s.indexOf('x'), s.lastIndexOf('x'))

  def pair(s: String): (Int, Int) = (s.length, s.indexOf('y'))

  def parts(s: String): Int = {
    val (a, b, c) = triple(s)
    a + b + c
  }

  def mixed(s: String): Int = {
    digits(0) += 1
    val (a, b) = (s.length, digits(0))
    for ((x, y) <- List((a, b))) digits(1) += x * y
    a * b
  }

  def aliased(s: String): Option[(Int, Int, Boolean)] = {
    digits(2) -= 1
    for {
      t <- Option(s)
      (l, x) = pair(t)
    } yield (l, x, false)
  }
}

object uniquenames {
  def main(args: Array[String]): Unit = {
    val d = new Digits(3)
    d.shift(1)
    println(d.parts("axbx"))
    println(d.mixed("ab"))
    println(d.aliased("xyx"))
    println(d.digits.toList)
  }
}
