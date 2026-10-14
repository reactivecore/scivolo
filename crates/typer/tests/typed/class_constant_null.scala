object ClassConstantNull {
  def a = classOf[Int] == null
  def b = classOf[String] != null
  def c = null == classOf[Int]
  def d = classOf[Int] == classOf[Int]
  def e = "s" == null
}
