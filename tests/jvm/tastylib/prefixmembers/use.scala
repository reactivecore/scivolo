import trees.*

object Use:
  def hole(h: Hole[AnyRef]): Hole[String] = h.withType("t")
  def main(args: Array[String]): Unit = println(hole(Hole(1)))
