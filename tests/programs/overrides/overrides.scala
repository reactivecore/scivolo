//> using scala 3.9.0
class Base:
  override def toString: String = "base"

class Sub extends Base:
  override def toString: String = "sub"

trait Maker:
  def make: Base = new Base()

class SubMaker extends Maker:
  override def make: Sub = new Sub()

class Parent:
  def make: Base = new Base()

class Child extends Parent:
  override def make: Sub = new Sub()

class ValChild extends Parent:
  override val make: Sub = new Sub()

@main def overrides(): Unit =
  val m: Maker = new SubMaker()
  println(m.make)
  val p: Parent = new Child()
  println(p.make)
  println(new Child().make)
  val v: Parent = new ValChild()
  println(v.make)
