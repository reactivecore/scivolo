import java.lang.reflect.InvocationTargetException;

/** Calls `main` of class `Test` through reflection, whatever its result type, as dotty's test runner does. */
public class DottyRun {
  public static void main(String[] args) throws Throwable {
    try {
      Class.forName("Test").getMethod("main", String[].class).invoke(null, (Object) new String[0]);
    } catch (InvocationTargetException e) {
      throw e.getCause();
    }
  }
}
