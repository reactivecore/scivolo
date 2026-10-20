import scala.reflect.Typeable

object Test {
  def f[T: Typeable](x: Any): Boolean = x match {
    case _: T => true
    case _ => false
  }
  def main(args: Array[String]): Unit = {
    println(f[Any]("abc"))
    println(!f[Any](1))
  }
}
