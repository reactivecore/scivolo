trait Config { def tagName: String = "$type" }

trait Types { types: Config =>
  trait Reader[T] { def read: T }
  trait Writer[T] { def write(t: T): String }
  trait TaggedReader[T] extends Reader[T] { def tag: String; def key = tagName + tag }
  trait TaggedWriter[T] extends Writer[T] { def tag: String; def key = tagName + tag }
  private trait Hidden { def h = 1 }

  object ReadWriter {
    def join[T](r0: Reader[T], w0: Writer[T]): String = (r0, w0) match {
      case (r1: TaggedReader[T], w1: TaggedWriter[T]) => r1.key + w1.key
      case _ => "plain"
    }
  }

  def hidden(x: Any): Int = x match {
    case h: Hidden => h.h
    case _ => 0
  }
  def makeHidden: Any = new Hidden {}
}

class Lib(override val tagName: String) extends Types with Config

@main def traitpatterns(): Unit = {
  val Api = new Lib("$type")
  val Other = new Lib("#")
  val r = new Api.TaggedReader[Int] { def read = 1; def tag = "r" }
  val w = new Api.TaggedWriter[Int] { def write(t: Int) = t.toString; def tag = "w" }
  val ow = new Other.TaggedWriter[Int] { def write(t: Int) = t.toString; def tag = "o" }
  println(Api.ReadWriter.join(r, w))
  println(Api.ReadWriter.join(r, ow.asInstanceOf[Api.Writer[Int]]))
  println(Api.hidden(Api.makeHidden))
  println(Api.hidden(Other.makeHidden))
}
