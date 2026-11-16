//> using scala 3.9.0
object Unit0 { override def toString = "Unit0" }

class Wrap(val u: Unit0.type) extends AnyVal

def wrapOf[T](x: T): Wrap = x.asInstanceOf[Wrap]

@main def nullobjectunbox(): Unit =
  val w: Wrap = null.asInstanceOf[Wrap]
  println(w.u == null)
  println(wrapOf(null).u == null)
  println(new Wrap(Unit0).u)
