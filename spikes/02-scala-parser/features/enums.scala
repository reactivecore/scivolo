enum Color:
  case Red, Green, Blue

enum Planet(val mass: Double, val radius: Double) extends java.lang.Enum[Planet]:
  case Mercury extends Planet(3.303e+23, 2.4397e6)
  case Earth extends Planet(5.976e+24, 6.37814e6)
  def surfaceGravity: Double = 6.67300e-11 * mass / (radius * radius)

enum Tree[+T] derives CanEqual:
  case Leaf(value: T)
  case Node(left: Tree[T], right: Tree[T])
  case Empty

enum Opt[+T] {
  case Some(x: T)
  case None
}

def depth[T](t: Tree[T]): Int = t match
  case Tree.Leaf(_) => 1
  case Tree.Node(l, r) => 1 + depth(l).max(depth(r))
  case Tree.Empty => 0
