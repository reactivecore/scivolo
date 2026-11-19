class ArrImpl extends ArrAbstract {
  def v(s: String*) = ()
}

object Test {
  def main(args: Array[String]): Unit =
    val a: ArrAbstract = ArrImpl()
    a.v(Array("a"))
}
