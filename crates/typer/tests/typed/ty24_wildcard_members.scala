trait Consumer[T]:
  def accept(x: T): Unit

trait Producer[T]:
  def produce(x: Any): T

trait ProdCons[T]:
  def apply(x: T): T

object Ty24WildcardMembers:
  def consume(a: Consumer[? >: String]): Unit = a.accept("foo")
  def produce(b: Producer[? <: String]): String = b.produce(1)
  val c: ProdCons[? <: String] = x => x
  val d: Consumer[? >: String] = x => ()
