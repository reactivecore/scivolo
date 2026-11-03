trait Visitor[-T, +J] { def visit(t: T): J }
object Visitor {
  class Delegate[T, J](d: Visitor[T, J]) extends Visitor[T, J] { def visit(t: T) = d.visit(t) }
}

trait Types {
  trait Reader[T] extends Visitor[Any, T]
  trait Writer[T] { def write(t: T): String }
  trait ReadWriter[T] extends Reader[T] with Writer[T]
  trait TaggedReader[T] extends Reader[T] { def tag: String = "t" }
  trait TaggedReadWriter[T] extends ReadWriter[T] with TaggedReader[T]

  object ReadWriter {
    def join[T](r0: Reader[T], w0: Writer[T]): ReadWriter[T] = (r0, w0) match {
      case (r1: TaggedReader[T], _) =>
        new TaggedReadWriter[T] {
          def visit(t: Any) = r1.visit(t)
          def write(t: T) = w0.write(t)
        }
      case _ =>
        new Visitor.Delegate[Any, T](r0) with ReadWriter[T] {
          def write(t: T) = w0.write(t)
        }
    }
  }
}

class Lib extends Types

@main def erasedjoins(): Unit = {
  val l = new Lib
  val r = new l.Reader[Int] { def visit(t: Any) = 1 }
  val w = new l.Writer[Int] { def write(t: Int) = t.toString }
  println(l.ReadWriter.join(r, w).write(2))
}
