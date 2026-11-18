case class City(zipCode: Int)

object Use:
  val city: lib.Forwarding.Mapped[NamedTuple.From[City], Option] = (zipCode = Some(1))
  def main(args: Array[String]): Unit = println(city.zipCode.get)
