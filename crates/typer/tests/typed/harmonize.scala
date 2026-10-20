object Harmonize {
  inline val b = 33
  def f(): Int = b + 1
  def c: Boolean = true

  val l1 = List(b, 33, 5.5)
  val l2 = List(f(), 33, 5.5)
  val l3 = List(5, 11L)
  val l4 = List(1.0f, 1234567890)
  val l5 = List(b, 33, 'a')
  val l6 = List(5.toByte, 300)
  val l7: List[AnyVal] = List(1.0f, 0)
  val l8 = List[Any](1.0f, 0)
  val a1: collection.mutable.ArrayBuffer[AnyVal] = collection.mutable.ArrayBuffer(1.0f, 0)
  val o1 = Option(if (c) 1 else 2.0)

  def keep[A >: Any](args: A*): String = args.mkString
  def collect[A >: Any](args: A*): List[A] = args.toList
  val k1 = keep(3.0, 42)
  val k2 = collect(3.0, 42)

  val i1 = if (c) b else if (c) 33 else 5.5
  val i2 = if (c) f() else 5.5
  val i3 = if (c) 1.0f else 2
  val i4 = if (c) 5.0 else '5'
  val i5: Any = if (c) 1L else 2

  val m1 = f() match {
    case 1 => 1
    case 2 => 2L
    case _ => 3
  }
  val t1 = try 1 catch { case _: Exception => 2.0 }
}
