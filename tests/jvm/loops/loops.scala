import scala.annotation.tailrec

object Loops {
  def fact(n: Int): Long = if n <= 1 then 1L else n * fact(n - 1)

  def sum(n: Int): Int = {
    var i = 0
    var s = 0
    while i < n do {
      s += i
      i += 1
    }
    s
  }

  @tailrec def gcd(a: Int, b: Int): Int = if b == 0 then a else gcd(b, a % b)

  def find(xs: Array[Int], v: Int): Int = {
    var i = 0
    while i < xs.length do {
      if xs(i) == v then return i
      i += 1
    }
    -1
  }

  def sign(x: Double): Int = if x > 0 then 1 else if x < 0 then -1 else 0

  def main(args: Array[String]): Unit = {
    println(fact(20))
    println(sum(100))
    println(gcd(84, 36))
    val xs = new Array[Int](10)
    var i = 0
    while i < xs.length do {
      xs(i) = i * i
      i += 1
    }
    println(find(xs, 49))
    println(find(xs, 50))
    println(sign(-2.5))
    println(sign(0.0))
    var j = 0
    var k = 0
    while j < 5 do {
      var m = 0
      while m < j do {
        k += m
        m += 1
      }
      j += 1
    }
    println(k)
    if k > 100 then println("big") else println("small")
  }
}
