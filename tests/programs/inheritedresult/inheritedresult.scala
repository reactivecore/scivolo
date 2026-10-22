//> using scala 3.9.0
abstract class Counter {
  def count(n: Int): Long
}

object Factors extends (Int => List[Int]) {
  override def apply(n: Int) = {
    val d = (2 to n / 2).find(n % _ == 0)
    d.map(x => x :: apply(n / x)).getOrElse(List(n))
  }
}

object Down extends Counter {
  def count(n: Int) = if (n == 0) 0L else 1 + count(n - 1)
}

object inheritedresult {
  def main(args: Array[String]): Unit = {
    println(Factors(360))
    println(Down.count(5))
  }
}
