final class Arr extends java.io.Serializable with java.lang.Cloneable
final class Crr extends java.lang.Cloneable
final class Drr extends java.lang.Cloneable
final class Srr extends java.io.Serializable
final class Trr extends java.io.Serializable
final class Box(val n: Int) extends Comparable[Box]:
  def compareTo(that: Box): Int = n - that.n
final class Cell(val n: Int) extends Comparable[Box]:
  def compareTo(that: Box): Int = n - that.n

def array(b: Boolean, o: Array[Object]) = if b then o else new Arr
def cloneables(b: Boolean) = if b then new Crr else new Drr
def serializables(b: Boolean) = if b then new Srr else new Trr
def comparables(b: Boolean) = if b then Box(1) else Cell(2)

@main def universaljoins(): Unit =
  println(array(true, Array[Object]("a")).getClass.getName)
  println(array(false, Array[Object]()).getClass.getName)
  println(cloneables(true).getClass.getName)
  println(serializables(false).getClass.getName)
  println(comparables(true).compareTo(Box(0)))
