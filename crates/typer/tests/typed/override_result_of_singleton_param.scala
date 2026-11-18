trait C:
  def g(x: String)(y: x.type): x.type

class D extends C:
  def g(x: String)(y: x.type): y.type = y
