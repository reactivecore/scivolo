object Probe:
  enum E:
    case A(x: Int)
    case B(y: Int)
  type C[+T] = String ?=> T
  class Box[+A](val a: A)
  extension [A](c: Box[A])
    def valid: C[A] = c.a
  def validate[T](op: Int => C[T]): List[T] = Nil

  def g(b: Boolean): List[E] = validate: v =>
    if b then
      val c = Box(1)
      E.A(c.valid)
    else
      val d = Box(2)
      E.B(d.valid)
