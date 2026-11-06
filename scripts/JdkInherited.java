import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Set;
import java.util.TreeSet;

/** Reads lines `class<TAB>method` and prints `class<TAB>superclass<TAB>method` for each method the
 *  JDK's class does not declare and its closest superclass declaring it does, by name, both public
 *  or protected instance methods. */
public class JdkInherited {
  static Set<String> declared(Class<?> c) {
    Set<String> out = new TreeSet<>();
    for (Method m : c.getDeclaredMethods()) {
      int mod = m.getModifiers();
      if ((Modifier.isPublic(mod) || Modifier.isProtected(mod)) && !Modifier.isStatic(mod)) out.add(m.getName());
    }
    return out;
  }

  public static void main(String[] args) throws Exception {
    for (String line : Files.readAllLines(Path.of(args[0]))) {
      String[] f = line.split("\t");
      Class<?> c;
      try {
        c = Class.forName(f[0], false, ClassLoader.getSystemClassLoader());
      } catch (ClassNotFoundException | LinkageError e) {
        continue;
      }
      if (!Modifier.isPublic(c.getModifiers()) || c.isInterface() || declared(c).contains(f[1])) continue;
      for (Class<?> s = c.getSuperclass(); s != null; s = s.getSuperclass()) {
        if (declared(s).contains(f[1])) {
          if (s != Object.class) System.out.println(f[0] + "\t" + s.getName() + "\t" + f[1]);
          break;
        }
      }
    }
  }
}
