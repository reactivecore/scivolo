//> using scala 3.9.0
// Casts of null to arrays, equals and hashCode of classes, equality of constants.

object Arrays {
  def cast(): Array[Int] = null.asInstanceOf[Array[Int]]
  def local(): Int = {
    val a: Array[Int] = null
    if (a == null) -1 else a.length
  }
  def take(a: Array[String]): Int = if (a == null) -1 else a.length
  def pass(): Int = take(null)
  lazy val empty: Array[Int] = null
}

object Classes {
  def same(c: Class[?]): Boolean = c.equals(classOf[String])
  def hash(c: Class[?]): Int = c.hashCode - c.hashCode
  def literal(): Boolean = classOf[Int].equals(classOf[Int])
}

object Consts {
  def nullString(): Boolean = "s" == null
  def strings(): Boolean = "a" == "b"
  def different(): Boolean = "a" != "b"
}

object Test {
  def run(): Unit = {
    println(Arrays.cast() == null)
    println(Arrays.local())
    println(Arrays.pass())
    println(Arrays.empty == null)
    println(Classes.same(classOf[String]))
    println(Classes.hash(classOf[Int]))
    println(Classes.literal())
    println(Consts.nullString())
    println(Consts.strings())
    println(Consts.different())
  }
}

@main def nullcasts(): Unit = Test.run()
