enum Color:
  case Red, Green
enum Tree[+T]:
  case Leaf
  case Node(l: Tree[T], r: Tree[T])
  case Other extends Tree[Nothing]
object O:
  def f = { enum E { case A }; E.A }
