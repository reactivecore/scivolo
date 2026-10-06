object Main:
  def area(r: Double): Double = r * r
  def add(a: Int, b: Int): Int = a + b
  def run(): Int =
    val xs = List(1.0, 2.0).map(area)
    val f = area
    val g: (Int, Int) => Int = add
    List(1, 2).foreach(println)
    val s = "abc"
    val found = List("a").exists(s.startsWith)
    val sums = List((1, 2), (3, 4)).map((a, b) => a + b)
    val o: Option[Double => Double] = Some(area)
    println(xs)
    println(found)
    g(1, 2) + sums.sum + f(1.0).toInt
