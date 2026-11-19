abstract class Endpoint[-R, F[_]] {
  type T0
  type T1
}
object Endpoint {
  type Full[_T0, _T1, -R, F[_]] = Endpoint[R, F] {
    type T0 = _T0
    type T1 = _T1
  }
  def apply[_T0, _T1, R, F[_]]: Endpoint.Full[_T0, _T1, R, F] = ???
}

object Test {
  def routes: List[Endpoint[Any, Option]] = {
    val a = Endpoint[Int, Unit, Any, Option]
    val b = Endpoint[Int, Int, Any, Option]
    val c = Endpoint[Unit, Int, Any, Option]
    List(a, b, c)
  }
}
