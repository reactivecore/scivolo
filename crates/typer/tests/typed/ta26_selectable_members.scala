class Box(val value: Int) extends AnyVal

class Record(fields: Map[String, Any]) extends Selectable:
  def selectDynamic(name: String): Any = fields(name)
  def applyDynamic(name: String)(arg: Int): Any = arg

object Ta26SelectableMembers:
  val counter = new reflect.Selectable:
    val count: Int = 0
    def box: Box = Box(count)
    def plus(n: Int): Int = count + n
  val wrapped = Option(new reflect.Selectable { def name = "a" })
  val record = Record(Map("x" -> 1)).asInstanceOf[Record { val x: Int; def twice(n: Int): Int }]
  def use: Int = counter.box.value + counter.plus(1) + wrapped.get.name.length + record.x + record.twice(2)
