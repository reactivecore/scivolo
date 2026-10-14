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
}
