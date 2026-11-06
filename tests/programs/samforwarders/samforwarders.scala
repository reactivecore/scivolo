trait Eqv[T] extends Any with Serializable {
  def eqv(x: T, y: T): Boolean
}

trait Eqv2 extends Eqv[Int]

object Eqv {
  def by[T, S](f: T => S): Eqv[T] = (x, y) => f(x) == f(y)
  def two: Eqv2 = (x, y) => x == y
  private val Sentinel: () => AnyRef = () => "s"
  def get(m: Map[Int, AnyRef], k: Int): AnyRef = m.getOrElse(k, Sentinel())
}

trait Mp {
  def get(m: Map[Int, AnyRef], k: Int): AnyRef = m.getOrElse(k, Mp.SentinelFn())
}

object Mp extends Mp {
  private val SentinelFn: () => AnyRef = () => "t"
}

object samforwarders {
  def main(args: Array[String]): Unit = {
    println(Eqv.by[String, Int](_.length).eqv("ab", "cd"))
    println(Eqv.two.eqv(1, 2))
    println(Eqv.get(Map(1 -> "a"), 2))
    println(Mp.get(Map(1 -> "a"), 2))
  }
}
