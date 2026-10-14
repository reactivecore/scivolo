object Main:
  def foo(a: Int | Double) = a match
    case a: (Float | Boolean) => 1
    case _ => 0
  def bar[T](x: T): Int = x match
    case i: Int => i + 1
    case s: String => s.length
    case _ => 0
  def baz(h: Any) = h match
    case 5 :: _ => "hello"
    case List(from) => from
  def qux(x: AnyRef) = x match
    case List(_) => 0
    case _ => -2
