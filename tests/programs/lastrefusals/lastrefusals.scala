import scala.scalanative.runtime.Continuations

trait Attr[T] {
  def name(): String
  def value(): T
}

trait Principal { def name: String }
class UserPrincipal(val name: String) extends Principal { def uid: Int = 7 }

trait Attrs {
  def owner(): Principal
  def size(): Long
}

abstract class Task[V] extends Serializable { def label(): String = "task" }
class Both extends Task[Int] with Runnable { def run(): Unit = () }

trait Fut[V]

object Exec {
  def index(f: Fut[AnyRef]): Int = f match {
    case f: Exec#Indexed[_] => f.index
    case _                  => -1
  }
}

class Exec {
  private sealed trait Indexed[V <: AnyRef] extends Fut[V] { var index = 3 }
  private class Impl[V <: AnyRef] extends Indexed[V]
  def fut(): Fut[AnyRef] = new Impl[AnyRef]
}

object Counter {
  private var c = 0
  def count: Int = c
  def count_=(v: Int): Unit = c = v
}

class Locked(size: Int) {
  private val lock = new Object
  private val buf = Array.tabulate(size)(i => ('a' + i).toChar)
  private var pos = 0

  def read(): Int = lock.synchronized {
    if (pos < buf.length) {
      val r = buf(pos)
      pos += 1
      r
    } else -1
  }
}

def isNull(b: Continuations.BoundaryLabel[Unit] | Null): Boolean = b == null

def attrs(all: Array[Attr[_]]): String =
  all.map(a => (a.name(), a.value())).toMap.map { case (n, v) => s"$n=$v" }.mkString(",")

def wrap(r: Runnable): String = {
  val t: Task[_] = r match {
    case t: Task[_] => t
    case _          => new Task[Unit] { override def label() = "wrapped" }
  }
  t.label()
}

private def attributes = new Attrs {
  private var st_size: Long = 0
  st_size = 42
  override def size(): Long = st_size
  override def owner(): UserPrincipal = new UserPrincipal("u")
}

def localDef[B](b: B): B = b

@main def lastrefusals(): Unit = {
  println(isNull(null))
  println(attrs(Array(new Attr[Int] { def name() = "a"; def value() = 1 })))
  println(wrap(new Both) + " " + wrap(() => ()))
  println(attributes.owner().uid + attributes.size())
  println(Exec.index(new Exec().fut()) + Exec.index(new Fut[AnyRef] {}))
  import Counter.count
  count = 5
  println(count)
  val l = new Locked(2)
  println(List(l.read(), l.read(), l.read()))
  println(localDef({ def f = 43; f }))
}
