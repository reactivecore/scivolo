//> using scala 3.9.0

class Buffer[A](val items: Array[A]):
  def size: Int = items.length
  def get(i: Int): A = items(i)
  def set(i: Int, a: A): Unit = items(i) = a

def swap[A](xs: Array[A], i: Int, j: Int): Unit =
  val t = xs(i)
  xs(i) = xs(j)
  xs(j) = t

def show[A](xs: Array[A]): Unit =
  var i = 0
  while i < xs.length do
    println(xs(i))
    i += 1

@main def genericarrays =
  val ints = Array(1, 2, 3)
  swap(ints, 0, 2)
  show(ints)
  val strs = Array("a", "b")
  swap(strs, 0, 1)
  show(strs)
  val buffer = Buffer(Array(1.5, 2.5))
  buffer.set(1, 4.0)
  println(buffer.get(1) + buffer.size)
