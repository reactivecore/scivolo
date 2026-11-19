import scala.deriving.Mirror

inline def some(p: Product): Some[Int] = summon[Mirror.ProductOf[Some[Int]]].fromProduct(p)

inline def ordinal(o: Option[Int]): Int = summon[Mirror.SumOf[Option[Int]]].ordinal(o)

object Inlinemirrors:
  def main(args: Array[String]): Unit =
    println(some(Tuple1(1)))
    println(some(Tuple1(2)).get + 1)
    println(ordinal(None))
    println(ordinal(Some(3)))
