trait Dec[A]
object Dec {
  implicit val di: Dec[Int] = new Dec[Int] {}
  implicit val ds: Dec[String] = new Dec[String] {}
  def forProduct2[A0, A1, A](n0: String, n1: String)(f: (A0, A1) => A)(implicit d0: Dec[A0], d1: Dec[A1]): Dec[A] = ???
}
case class P(id: Int, name: String)
object MaximizedParamVars {
  val d = Dec.forProduct2("id", "name")(P.apply)
}
class C { def foo[T <: String: reflect.ClassTag](f: T => Int) = 1; def bar(f: String => Int) = foo(f) }
