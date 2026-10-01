//> using scala 3.9.0
class Base:
  override def toString: String = "base"

class Sub extends Base:
  override def toString: String = "sub"

class SubSub extends Sub:
  override def toString: String = "subsub"

trait Maker:
  def make: Base = new Base()

class SubMaker extends Maker:
  override def make: Sub = new Sub()

class Parent:
  def make: Base = new Base()

class Child extends Parent:
  override def make: Sub = new Sub()

class GrandChild extends Child:
  override def make: SubSub = new SubSub()

class ValChild extends Parent:
  override val make: Sub = new Sub()

trait Abstract:
  def make: Base

class Concrete extends Abstract:
  def make: Sub = new Sub()

abstract class AbstractParent:
  def make: Base

abstract class Narrowed extends AbstractParent:
  override def make: Sub

class Implemented extends Narrowed:
  def make: Sub = new Sub()

@main def overrides(): Unit =
  val m: Maker = new SubMaker()
  println(m.make)
  val p: Parent = new Child()
  println(p.make)
  println(new Child().make)
  val v: Parent = new ValChild()
  println(v.make)
  val g: Parent = new GrandChild()
  println(g.make)
  val a: Abstract = new Concrete()
  println(a.make)
  val n: AbstractParent = new Implemented()
  println(n.make)
