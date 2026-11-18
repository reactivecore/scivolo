class AnyMembers:
  def string(x: String) = x.getClass
  def ref(x: AnyRef) = x.getClass
  def int(x: Int) = x.getClass
  def own = getClass
  def name(x: List[Int]) = x.getClass.getName
  def show(x: Any) = x.toString
  def hash(x: Any) = x.hashCode
  def same(x: Any) = x == 1
  def hashHash(x: Any) = x.##
  def test(x: Any) = x.isInstanceOf[String]
  def refEq(x: AnyRef) = x eq x
  def concat(x: String) = x + 1
