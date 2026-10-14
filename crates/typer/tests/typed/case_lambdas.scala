case class Employee(name: String, age: Int)

object Folds {
  def oldest(staff: List[Employee]) =
    staff.foldLeft(Option.empty[Employee]) {
      case (None, e)                     => Some(e)
      case (Some(o), e) if e.age > o.age => Some(e)
      case (acc, _)                      => acc
    }
  def total(staff: List[Employee]) = staff.foldLeft(0) { case (acc, e) => acc + e.age }
  def first(staff: List[Employee]) = staff.foldLeft(0) { (acc, e) => acc match { case n => n } }
  def ages(staff: List[Employee]) = staff.map { case Employee(_, age) => age }
  def weighted(pairs: List[(Int, Int)]) = pairs.foldLeft(0) { case (acc, (x, w)) => acc + x * w }
}

trait F2 { def run(a: Int, b: Int): Int }

object Arities {
  val sam: F2 = { case (a, b) => a + b }
  val three: (Int, Int, Int) => Int = { case (a, b, c) => a + b + c }
  val zero: () => Int = { case _ => 1 }
}
