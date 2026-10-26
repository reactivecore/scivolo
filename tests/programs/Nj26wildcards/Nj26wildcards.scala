//> using scala 3.9.0
import java.util.{ArrayList, Collection, Comparator}
import java.util.function.{Consumer, Supplier}

class Bag[E](val items: ArrayList[E], val name: String):
  def this() = this(new ArrayList[E](), "bag")
  def this(c: Collection[? <: E]) =
    this()
    c.forEach(e => items.add(e))
  def this(e: E, n: Int) =
    this()
    for _ <- 1 to n do items.add(e)
  def addAll(c: Collection[? <: E]): Boolean =
    var changed = false
    c.forEach(e => changed = items.add(e) || changed)
    changed

class Ordered[E](cmp: Comparator[? >: E]):
  def comparator(): Comparator[? >: E] = cmp
  def less(a: E, b: E): Boolean = comparator().compare(a, b) < 0

class Box[T](val value: T):
  def map[U](f: T => U): Box[U] = new Box(f(value))
  def same: Box[T] = this

extension [A](xs: java.lang.Iterable[A])
  def countAll: Int =
    var n = 0
    xs.forEach(_ => n += 1)
    n

def each[T](xs: Collection[? <: T], f: Consumer[? >: T]): Unit =
  val it = xs.iterator()
  while it.hasNext() do f.accept(it.next())

def first[T](s: Supplier[? <: T]): T = s.get()

def containsAll(c: Collection[?], all: Collection[?]): Boolean =
  var it = c.iterator()
  var found = true
  while it.hasNext() do found &&= all.contains(it.next())
  found

@main def Nj26wildcards(): Unit =
  val l = new ArrayList[String]()
  l.add("a"); l.add("b")
  val bag = new Bag[CharSequence](l)
  println(bag.items)
  println(new Bag[String]("x", 3).items)
  println(bag.addAll(l))
  each[String](l, s => println(s))
  println(first[String](() => "got"))
  println(new Ordered[String](Comparator.naturalOrder[String]()).less("a", "b"))
  println(l.countAll)
  println(containsAll(l, bag.items))
  val boxes: List[Box[?]] = List(new Box(1), new Box("s"))
  println(boxes.map(_.same).map(_.value))
