trait Writable { def writeTo(out: StringBuilder): Unit }
trait Source { def handle(): Writable }

object Sources {
  def generator[T](a: List[T])(implicit f: T => Writable): Source = {
    val f0 = f
    new Source {
      def handle() = new Writable {
        def writeTo(out: StringBuilder) = {
          for (x <- a) f0(x).writeTo(out)
        }
      }
    }
  }

  def traversable[T](a: Seq[T], sep: String)(implicit f: T => Writable): Source = {
    val f0 = f
    new Source {
      def handle() = new Writable {
        def writeTo(out: StringBuilder) = {
          a.foreach { x => f0(x).writeTo(out); out.append(sep) }
          a.map(x => f0(x)).foreach(_.writeTo(out))
        }
      }
    }
  }

  def nested(k: Int) = {
    class C {
      def g = {
        class D { def h = List(1, 2).flatMap(y => List(y).map(_ + k)) }
        new D().h
      }
    }
    new C().g
  }
}

@main def outerlambdas(): Unit = {
  implicit val w: Int => Writable = i => new Writable { def writeTo(out: StringBuilder) = out.append(i) }
  val sb = new StringBuilder
  Sources.generator(List(1, 2, 3)).handle().writeTo(sb)
  Sources.traversable(Vector(4, 5), ",").handle().writeTo(sb)
  println(sb)
  println(Sources.nested(10))
}
