class Inv[T]

class ViewsByApplicableMember:
  implicit class Wrong(test: ViewsByApplicableMember):
    def right: Any = ???
  implicit class Right(test: ViewsByApplicableMember):
    def right(node: Any): Any = node

  def inv[T](x: T): Inv[T] = new Inv[T]

  def called = (this).right(inv(1))
