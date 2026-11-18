import lib.*

class ViaOpaque extends TF:
  def f(x: O.F[Int]) = 1

class ViaExport extends TF:
  def f(x: Fw.F[Int]) = 2

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
    val cls = getClass
    val names = Set("direct", "exported", "nested", "function", "lambda", "bounded")
    for m <- cls.getDeclaredMethods.filter(m => names(m.getName)).sortBy(_.getName) do
      println(s"${m.getName}: ${m.getGenericReturnType}")
    println(s"field: ${cls.getDeclaredField("field").getGenericType}")
