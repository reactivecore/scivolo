//> using scala 3.9.0

class Outer {
  var v = 0
  class Inner {
    def set(x: Int) = v = x
    def setL(x: Int) = { val f = () => v = x; f() }
  }
}

trait Tr {
  var tv = 0
  def setTL(x: Int) = { val f = () => tv += x; f() }
}
class TrC extends Tr

object Enc {
  var ev = 0
  object Nested {
    def set(x: Int) = ev = x
    def setL(x: Int) = { val f = () => ev = x; f() }
  }
}

class Loc {
  var lv = 0
  def m(x: Int) = {
    class L { def set() = lv = x }
    new L().set()
    lv
  }
}

object outervarassign {
  def main(args: Array[String]): Unit = {
    val o = new Outer
    val i = new o.Inner
    i.set(3); println(o.v)
    i.setL(4); println(o.v)
    val t = new TrC
    t.setTL(5); println(t.tv)
    Enc.Nested.set(7); println(Enc.ev)
    Enc.Nested.setL(8); println(Enc.ev)
    println(new Loc().m(11))
  }
}
