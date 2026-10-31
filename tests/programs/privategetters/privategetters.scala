//> using scala 3.9.0
class Getter(p: String) extends Comparable[Getter]:
  private val path: String = p.trim
  private var count: Long = 1L
  def combine(other: Getter): Unit = count += other.count
  override def equals(o: Any): Boolean = o match
    case that: Getter => this.path == that.path
    case _            => false
  override def hashCode: Int = path.##
  def compareTo(o: Getter): Int = path.compareTo(o.path)

object Getter:
  def total(a: Getter, b: Getter): Long = a.count + b.count

class Boxed(val b: java.lang.Boolean, val l: Long, val d: Double, val f: Float):
  def isFalse: Boolean = b == false
  def hashes: List[Int] = List(l.##, d.##, f.##)

@main def privategetters(): Unit =
  val (a, b) = (Getter(" x "), Getter("x"))
  a.combine(b)
  b.combine(a)
  println(s"${a == b} ${a.compareTo(b)} ${Getter.total(a, b)} ${a.hashCode == "x".hashCode}")
  println(Boxed(false, 1L, 2.0, 3.0f).isFalse)
  println(Boxed(true, 1L, 2.0, 3.0f).isFalse)
  println(Boxed(null, 5000000000L, 1.5, 2.5f).hashes)
