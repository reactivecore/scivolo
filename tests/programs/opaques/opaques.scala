//> using scala 3.9.0
object Meters {
  opaque type Meter = Double
  def apply(d: Double): Meter = d
  def sum(a: Meter, b: Meter): Meter = a + b
  def lifted(d: Double): Meter = Meters(d)
  extension (m: Meter) def value: Double = m
}

object Ids {
  opaque type Id <: AnyRef = java.lang.String
  def of(s: String): Id = s
  def all(ids: List[Id]): List[String] = ids
}

object opaques {
  def main(args: Array[String]): Unit = {
    val m = Meters.sum(Meters(1.5), Meters.lifted(2.0))
    println(m.value)
    val ids = List(Ids.of("a"), Ids.of("b"))
    println(Ids.all(ids).mkString(","))
    val id: AnyRef = ids.head
    println(id)
  }
}
