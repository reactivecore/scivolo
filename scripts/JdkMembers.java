import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.file.Files;
import java.nio.file.Path;

/** Reads lines `class<TAB>method` and prints those whose class this JDK has, public, with a public
 *  method of that name, declared or inherited. */
public class JdkMembers {
  public static void main(String[] args) throws Exception {
    for (String line : Files.readAllLines(Path.of(args[0]))) {
      String[] f = line.split("\t");
      Class<?> c;
      try {
        c = Class.forName(f[0], false, ClassLoader.getSystemClassLoader());
      } catch (ClassNotFoundException | LinkageError e) {
        continue;
      }
      if (!Modifier.isPublic(c.getModifiers())) continue;
      for (Method m : c.getMethods()) {
        if (m.getName().equals(f[1])) {
          System.out.println(line);
          break;
        }
      }
    }
  }
}
