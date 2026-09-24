//> using scala 3.9.0

case class Point(x: Int, y: Int)

object Features:
  val greeting = "hi"

  def classify(n: Int): String = n match
    case 0 => "zero"
    case 1 => "one"
    case _ => "many"

  def norm1(p: Point): Int = math.abs(p.x) + math.abs(p.y)

  def mean(a: Double, b: Double): Double = (a + b) / 2.0

  def main(args: Array[String]): Unit =
    val p = Point(3, -4)
    println(s"$greeting ${p.x},${p.y} norm1=${norm1(p)}")
    var i = 0
    while i < 3 do
      println(classify(i))
      i += 1
    val big = 1L << 40
    println("big=" + big + " mean=" + mean(1.5, 2.0) + " ok=" + (i == 3 && !(big < 0)))
