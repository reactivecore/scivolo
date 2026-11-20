class C(x: => Int, y: Int):
  def get: Int = x + y
  def twice: Int = y + x
