//> using scala 3.9.0

class Box(val elems: Any) extends AnyVal { self =>
  def show: String = s"Box($elems)"
  def toIt: Iterable[String] = new Iterable[String] {
    def iterator = Iterator(self.show)
  }
  def pick(b: Boolean): Box = elems match
    case null => Box.empty
    case _ => if b then this else Box.empty
  def isEmpty: Boolean = elems == null
}

object Box:
  def empty: Box = new Box(null)

def id[A](a: A): A = a

def ident[T](t: T): T = t

@main def valuebranches(): Unit =
  val b = new Box(1)
  println(b.toIt.toList)
  println(id(b).isInstanceOf[Box])
  println(b.pick(true).show)
  println(b.pick(false).isEmpty)
  var i = 0
  while i < 3 do ident { i += 1 }
  println(i)
