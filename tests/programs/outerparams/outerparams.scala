trait Visitor[-T, +J] {
  def visitString(s: CharSequence, index: Int): J
  def map[Z](f: J => Z): Visitor[T, Z] = new Visitor.MapReader[T, J, Z](Visitor.this) {
    def mapNonNullsFunction(v: J): Z = f(v)
  }
}

object Visitor {
  abstract class MapReader[-T, V, Z](delegatedReader: Visitor[T, V]) extends Visitor[T, Z] {
    def mapNonNullsFunction(t: V): Z
    def mapFunction(v: V): Z =
      if (v == null) null.asInstanceOf[Z]
      else mapNonNullsFunction(v)
    override def visitString(s: CharSequence, index: Int) = mapFunction(delegatedReader.visitString(s, index))
  }
}

@main def outerparams(): Unit = {
  val v = new Visitor[Any, String] { def visitString(s: CharSequence, index: Int) = s.toString }
  println(v.map(_.length).visitString("abc", 0))
  println(v.map(_ + "!").map(_.length).visitString("abcd", 0))
}
