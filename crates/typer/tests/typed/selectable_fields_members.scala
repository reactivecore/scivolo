class SelectableNT[N <: Tuple] extends Selectable:
  def selectDynamic(name: String) = ???
  type Names = Tuple.Map[N, [X] =>> X]
  type Fields = NamedTuple.NamedTuple[Names, (String, Int)]

class SingletonNT[A <: NamedTuple.AnyNamedTuple](val nt: A) extends Selectable:
  type Fields = A
  def selectDynamic(x: String) = ???

trait Join[A <: Tuple, B <: Tuple]:
  type Names = Tuple.Concat[A, B]
  type NT = NamedTuple.NamedTuple[Names, (String, Int)]
  val nt: NT = ???

object Test:
  val x = new SelectableNT[("name", "age")]
  val n = x.name

  val a = (name = "foo", age = 1)
  val b: a.type = a
  val sb = SingletonNT(b)
  val m = sb.name

  val join: Join[Tuple1["first"], Tuple1["second"]] = ???
  val s = join.nt.second
