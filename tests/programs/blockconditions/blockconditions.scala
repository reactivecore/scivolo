//> using scala 3.9.0
class Node(val left: Node, val right: Node, val key: Int) {
  def find(k: Int): Node = if (key == k) this else null
}

trait View { def name: String }
trait OwnerView extends View { def setOwner(o: String): Unit }
trait BasicView extends View { def times(): Int }
trait PosixView extends BasicView with OwnerView { def setGroup(g: String): Unit }
trait DosView extends BasicView { def setHidden(): Unit }

class PV extends PosixView {
  def name = "pv"
  def setOwner(o: String) = println(s"owner $o")
  def times() = 1
  def setGroup(g: String) = println(s"group $g")
}

object blockconditions {
  def search(p: Node, k: Int): Node = {
    var q: Node = null
    var ch: Node = null
    if (({ ch = p.left; ch } != null && { q = ch.find(k); q } != null) ||
        ({ ch = p.right; ch } != null && { q = ch.find(k); q } != null))
      q
    else null
  }

  def unescape(line: String): String = {
    val buf = new java.lang.StringBuilder
    var i = 0
    var ch: Char = 0
    def next(): Char = { ch = line.charAt(i); i += 1; ch }
    while (i < line.length) {
      if (next() == '\\') {
        next()
        ch match {
          case 't' => buf.append('\t')
          case 'n' => buf.append('\n')
          case _   => buf.append(ch)
        }
      } else buf.append(ch)
    }
    buf.toString
  }

  def view[V <: View](cls: Class[V]): V = new PV().asInstanceOf[V]

  def copy(attrs: Any, posix: Boolean): Unit = {
    val cls = if (posix) classOf[PosixView] else classOf[DosView]
    (attrs, view(cls)) match {
      case (attrs: String, v: PosixView) =>
        v.setGroup(attrs)
        v.setOwner(attrs)
      case _ => ()
    }
  }

  def index(b: Byte, s: Short): Int = (b - scala.Byte.MinValue) + (s - scala.Short.MinValue)

  def main(args: Array[String]): Unit = {
    val n = new Node(new Node(null, null, 1), new Node(null, null, 2), 0)
    println(search(n, 2).key)
    println(search(n, 3))
    println(unescape("a\\tb\\nc\\x"))
    copy("me", true)
    println(index(3, 4))
  }
}
