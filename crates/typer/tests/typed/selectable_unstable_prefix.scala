trait Foo extends Selectable:
  val f: Any
  type Fields = (myfield: f.type)
  def selectDynamic(name: String): Any

object Test:
  def elem: Foo { val f: Int } = ???
  val b: Int = elem.myfield
