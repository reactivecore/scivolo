//> using scala 3.9.0

trait Shape
class Square extends Shape
class Big extends Square

def depth(n: Int): Int = if n == 0 then 0 else 1 + depth(n - 1)

@main def runtime(): Unit =
  println(classOf[Object].isAssignableFrom(classOf[String]))
  println(classOf[String].isAssignableFrom(classOf[Object]))
  println(classOf[Shape].isAssignableFrom(classOf[Big]))
  println(classOf[Comparable[?]].isInstance("x"))
  println(classOf[Big].getSuperclass.getName)
  println(classOf[Class[?]].getSuperclass.getName)
  println(classOf[Square].getInterfaces.map(_.getName).mkString(","))
  val n = -1
  try println(new Array[Int](n).length)
  catch case e: NegativeArraySizeException => println("negative size caught")
  println(depth(10000))
  try println(depth(100000000))
  catch case _: StackOverflowError => println("stack overflow caught")
  println(Integer.parseInt("-42") + java.lang.Short.parseShort("-7"))
