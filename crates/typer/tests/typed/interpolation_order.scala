object Main {
  def opt[A]: Option[A] = None
  def a = opt.getOrElse("x")
  def b = List.empty :+ 1
  def c = Option.empty.getOrElse(0)
  def d = Map.empty.updated(1, "a")
  val e: Option[Any] = Option.empty.orElse(Some(1))
  def f = Seq.empty ++ List(1)
  val g: Map[String, List[Int]] = Map.empty.withDefaultValue(Nil)
  val h: Option[Int] = List.empty.headOption
  val i: Iterable[Int] = Map.empty.keys
  def j = List(Some(1), opt)
  def k = List(opt, Some(1))
}
