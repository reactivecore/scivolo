//> using scala 3.9.0
// A class nested in an anonymous class passing a captured value to its superclass.

class TDBase(val a: AnyRef)

trait TDProbe:
  def aField: AnyRef

object TDsupercapture:
  def probe(): Boolean =
    val a2 = new Object
    val obj: TDProbe = new TDProbe:
      class S extends TDBase(a2)
      val s = new S {}
      def aField = s.a
    obj.aField eq a2

  def main(args: Array[String]): Unit =
    println(probe())
