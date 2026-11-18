import language.experimental.captureChecking

trait Sq[+A]:
  def name: String = "s"
trait Ix[+A] extends Sq[A]

trait Ops[+A]:
  self: Ops[A]^ =>
  def n: Int
  def a: String = self match
    case s: Ix[?] => s.name
    case _ => toString
  def b: Sq[A] = (this: @unchecked) match
    case s: Sq[A] => s
    case _ => new Sq[A] {}
  def d: Int = self match
    case o: Ops[?] => o.n
  def e: Int = this match
    case _: Ix[?] => 1
    case x => x.n
