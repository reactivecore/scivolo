import language.experimental.captureChecking

class Node[T](var next: Node[T] | Null):
  this: Node[T]^{} =>
  def walk(): Int =
    var curr = this
    0
  def me() = this
  def m(): Int = this match
    case n: Node[?] => 1

class Plain[T]:
  def me = this
  def walk(): Int =
    var these = this
    0
  def m(): Int = this match
    case p: Plain[?] => 1
    case null => 2

trait Ops[+A]:
  self: Ops[A]^ =>
  def w(): Int =
    var c = this
    0
  def m(): Int = this match
    case o: Ops[?] => 1

trait Ser:
  this: Iterable[?]^ =>
  def f(): Int =
    val n: Int = this match
      case it: Product => it.productArity
      case it => it.size
    n

object Obj:
  def me = this
