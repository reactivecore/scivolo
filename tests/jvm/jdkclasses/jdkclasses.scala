import java.time.{DayOfWeek, Duration, LocalDate, Year}
import java.time.temporal.{ChronoField, ChronoUnit, TemporalAccessor, TemporalAdjuster, TemporalQueries, TemporalQuery}
import java.util.concurrent.ConcurrentSkipListMap

class DayQuery extends TemporalQuery[Integer] {
  def queryFrom(t: TemporalAccessor): Integer = t.get(ChronoField.DAY_OF_MONTH)
}

object Jdkclasses {
  def main(args: Array[String]): Unit = {
    val d = LocalDate.of(2024, 2, 28)
    println(d.plusDays(2))
    println(d.getDayOfWeek() == DayOfWeek.WEDNESDAY)
    println(Duration.ofMinutes(90).toHours())
    println(ChronoUnit.DAYS.between(d, LocalDate.of(2024, 3, 10)))
    println(d.datesUntil(LocalDate.of(2024, 3, 3)).count())
    val day: LocalDate = d.query(TemporalQueries.localDate())
    println(day.getYear())
    println(Duration.between(d.atStartOfDay(), d.plusDays(1).atTime(1, 30)).toMinutes())
    println(d.query(new DayQuery))
    val first: TemporalAdjuster = t => t.`with`(ChronoField.DAY_OF_MONTH, 1)
    println(d.`with`(first))
    println(DayOfWeek.values().length)
    println(Year.MIN_VALUE)
    val m = new ConcurrentSkipListMap[String, Integer]()
    m.put("b", 2)
    m.put("a", 1)
    println(m.firstEntry().getKey())
    println(m.headMap("b").size())
    println(java.text.MessageFormat.format("{0}+{1}", "x", "y"))
    val rows: java.util.Vector[java.util.Vector[?]] = new javax.swing.table.DefaultTableModel(2, 3).getDataVector()
    println(rows.get(1).size())
  }
}
