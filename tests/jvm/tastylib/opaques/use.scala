import lib.*

class ViaOpaque extends TF:
  def f(x: O.F[Int]) = 1

class ViaExport extends TF:
  def f(x: Fw.F[Int]) = 2

class ImplVar extends TV:
  var w = 5

object Use:
  def direct: O.F[Int] = O.mkF
  def exported: Fw.F[String] = ???
  def nested: List[Fw.F[Int]] = Nil
  def function: VF.Op[String] = VF.op
  def lambda: PF.FB[String] = ???
  def bounded: O.B[Int] = O.mkB
  val field: Fw.F[Int] = Fw.mkF

  def kinded[C[_]](c: C[Int]): C[Int] = c

  def main(args: Array[String]): Unit =
    val a: O.F[Int] = Fw.mkF
    val b: V.Op[String] = VF.op
    val same = summon[Fw.F[Int] =:= O.F[Int]]
    val seq: Seq[Int] = O.mkB
    println(kinded(Fw.mkG))
    println(kinded[O.F](same(a)))
    println(seq.head)
    println(ViaOpaque().f(a) + ViaExport().f(a))
    println(b.toString.nonEmpty)
    println(O.headOf(O.mkF) + 1)
    println(O.firstOf(O.mkF) + 2)
    println(O.mkB.map(_ + 1))
    println(O.fs(0).getClass.getName)
    println(O.fs.getClass.getName)
    println(Prim.metres(Prim.m))
    val unit: Any = Prim.u
    val meters: Any = Prim.m
    println(unit)
    println(meters)
    println(Prim.i == (41: Any))
    println(Prim.u == Prim.u)
    println(ImplVar().w)
    val cls = getClass
    val names = Set("direct", "exported", "nested", "function", "lambda", "bounded")
    for m <- cls.getDeclaredMethods.filter(m => names(m.getName)).sortBy(_.getName) do
      println(s"${m.getName}: ${m.getGenericReturnType}")
    println(s"field: ${cls.getDeclaredField("field").getGenericType}")
