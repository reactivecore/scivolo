import scala.deriving.Mirror
import scala.util.NotGiven

trait Wr[T]
trait Cur[T]
object Wr {
  inline given sup[T: Mirror.ProductOf, V >: T: Wr: Mirror.SumOf](using NotGiven[Cur[V]]): Wr[T] = new Wr[T] {}
}
case class P(i: Int)
object P { implicit val w: Wr[P] = new Wr[P] {} }

object MirrorVar {
  def main(args: Array[String]): Unit = println(summon[Wr[P]] != null)
}
