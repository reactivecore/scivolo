//> using scala 3.9.0
class C1:
  private class C2
  def isC2(x: Any): Boolean = x match
    case c: C2 => true
    case _ => false
  def make: Any = new C2

class P:
  class D
  def test(x: Any): String = x match
    case name: D => "D"
    case null => "null"
    case _ => "other"
  def make: D = new D

class C:
  val x = 1
  object Mid:
    val y = x + x
    class Inner:
      def xy = x + y
    object InnerObj:
      def xy = x + y

class S:
  class K:
    class In3:
      def f = 1
  val k = new K
  def r(h: S) = new h.k.In3().f

class W3:
  object O1:
    object O2:
      class In:
        def g = 2
      def mk(w: W3) = new w.O1.O2.In

@main def innerclasses2(): Unit =
  println(S().r(S()))
  val a = C1()
  println(a.isC2(a.make))
  println(C1().isC2(a.make))
  val p = P()
  println(p.test(p.make))
  println(p.test(P().make))
  println(p.test(null))
  val c = new C()
  println(c.Mid.y)
  println(c.Mid.InnerObj.xy)
  println(new c.Mid.Inner().xy)
