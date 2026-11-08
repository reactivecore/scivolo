object Arith {
  def add(a: Int, b: Int): Int = a + b

  def mix(l: Long, d: Double, f: Float): Double = l * d + f

  def main(args: Array[String]): Unit = {
    val x = 7
    val y = 3
    println(add(x, y))
    println(x - y)
    println(x * y)
    println(x / y)
    println(x % y)
    println(-x)
    println(~x)
    println(x << 2)
    println(-x >>> 28)
    println(x & y | 8 ^ 1)
    val l = 10000000000L
    println(l * 3 + x)
    println(mix(l, 0.5, 2.5f))
    val f = 2.5f
    println(f / 2)
    println(x.toDouble / 2)
    println(l.toInt)
    println('a'.toInt)
    println((x + 'b').toChar)
    val bt: Byte = 100
    val s: Short = 1000
    println(bt + s)
    println(x > y)
    println(x == 7 && y != 3)
    println(!(x < y) || false)
    println(l >= 0L)
    println(f < 1.0f)
    var z = 0
    z += x
    z = z * 2
    println(z)
    val o: Any = "s"
    println(o.isInstanceOf[String])
    println(o.asInstanceOf[String])
    val n: Any = x
    println(n.asInstanceOf[Int] + 1)
    val arr = new Array[Int](3)
    arr(0) = 1
    arr(1) = x
    println(arr(0) + arr(1) + arr.length)
    val ds = Array(1.5, 2.5)
    println(ds(1))
    val strs = Array("a", "b")
    println(strs(1))
  }
}
