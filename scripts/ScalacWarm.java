import com.sun.management.OperatingSystemMXBean;
import java.lang.management.ManagementFactory;
import java.util.Arrays;

/**
 * Compiles with dotty's `Main` N times in one JVM, as a compile server does; prints the wall and
 * process CPU nanoseconds of each run. Usage: ScalacWarm N SCALAC_ARGS...
 */
public class ScalacWarm {
  public static void main(String[] args) {
    int runs = Integer.parseInt(args[0]);
    String[] scalac = Arrays.copyOfRange(args, 1, args.length);
    var os = (OperatingSystemMXBean) ManagementFactory.getOperatingSystemMXBean();
    for (int i = 0; i < runs; i++) {
      long wall = System.nanoTime(), cpu = os.getProcessCpuTime();
      var reporter = dotty.tools.dotc.Main$.MODULE$.process(scalac);
      System.out.println((System.nanoTime() - wall) + " " + (os.getProcessCpuTime() - cpu));
      if (reporter.hasErrors()) System.exit(1);
    }
  }
}
