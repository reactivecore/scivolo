trait ArrVisitor[-T, +J] {
  def size: Int
}

trait Visitor[-T, +J] {
  def visitArray(length: Int, index: Int): ArrVisitor[T, J]
  def visitKey(index: Int): Visitor[T, J]
}

class Parser[J](v: Any) {
  def reject(i: Int): PartialFunction[Throwable, Nothing] = { case e: IllegalStateException => throw e }

  def array(i: Int): Int = {
    val ctx =
      try v.asInstanceOf[Visitor[_, J]].visitArray(-1, i)
      catch reject(i)
    ctx.size
  }

  def key(i: Int): Int = {
    val sub = v.asInstanceOf[Visitor[_, J]].visitKey(i)
    sub.visitArray(i, i).size
  }
}

@main def skolemparams(): Unit = {
  lazy val visitor: Visitor[Any, String] = new Visitor[Any, String] {
    def visitArray(l: Int, i: Int) = new ArrVisitor[Any, String] { def size = l + i }
    def visitKey(i: Int) = visitor
  }
  val p = new Parser[String](visitor)
  println(p.array(3))
  println(p.key(4))
}
