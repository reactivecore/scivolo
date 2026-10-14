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

object Nulls {
  def n0: Null = null
  def toObject(): Object = (null: String).asInstanceOf[Object]
  def nullToArray(): Array[Int] = n0.asInstanceOf[Array[Int]]
  def nullToString(): String = n0.asInstanceOf[String]
  def anyToArray(): Array[String] = (null: Any).asInstanceOf[Array[String]]
  def local(): Array[Int] = { val n = null; n }
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
  def classIsNull(): Boolean = classOf[Int] == null
  def classIsNotNull(): Boolean = classOf[String] != null
  def nullIsClass(): Boolean = null == classOf[Int]
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
    println(Consts.classIsNull())
    println(Consts.classIsNotNull())
    println(Consts.nullIsClass())
    println(Nulls.toObject() == null)
    println(Nulls.nullToArray() == null)
    println(Nulls.nullToString() == null)
    println(Nulls.anyToArray() == null)
    println(Nulls.local() == null)
  }
}

@main def nullcasts(): Unit = Test.run()
