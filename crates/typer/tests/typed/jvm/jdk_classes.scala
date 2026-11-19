import java.util.GregorianCalendar

object JdkClasses:
  val calendar = new GregorianCalendar()
  val year = calendar.get(java.util.Calendar.YEAR)
  def show(x: Int): Unit = System.out.println(x)
  def methods = classOf[String].getMethods.length
