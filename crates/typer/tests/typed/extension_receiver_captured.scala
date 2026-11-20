trait Container:
  trait Path[T]:
    type Member

  extension (path: Path[?])
    infix def ext(color: path.Member): Unit = ???
    infix def ext(other: Int): Unit = ???

class Box[T]
extension (b: Box[?]) def name: String = ""

object ExtensionReceiverCaptured:
  val c: Container = ???
  import c.Path

  def transition(path: Path[?])(using DummyImplicit): Path[?] = ???

  def test(p: Path[?]): Unit =
    transition(p).ext(1)

  def boxed: Box[?] = ???
  val inScope = boxed.name
