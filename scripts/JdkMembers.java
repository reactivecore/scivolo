import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.stream.Collectors;

/** Reads lines `class<TAB>method<TAB>parameters` (NIR's spelling) and prints each whose class this
 *  JDK has, public, with `sig` if the class has a public method of that name and parameters,
 *  declared or inherited, `name` if one of that name only, else `none`; then `interface` or
 *  `class`. */
public class JdkMembers {
  static String nir(Class<?> t) {
    if (t.isArray()) return "array[" + nir(t.getComponentType()) + "]";
    return t == boolean.class ? "bool" : t.getName();
  }

  public static void main(String[] args) throws Exception {
    for (String line : Files.readAllLines(Path.of(args[0]))) {
      String[] f = line.split("\t", -1);
      Class<?> c;
      try {
        c = Class.forName(f[0], false, ClassLoader.getSystemClassLoader());
      } catch (ClassNotFoundException | LinkageError e) {
        continue;
      }
      if (!Modifier.isPublic(c.getModifiers())) continue;
      String found = "none";
      for (Method m : c.getMethods()) {
        if (!m.getName().equals(f[1])) continue;
        String ps = Arrays.stream(m.getParameterTypes()).map(JdkMembers::nir).collect(Collectors.joining(", "));
        if (ps.equals(f[2])) {
          found = "sig";
          break;
        }
        found = "name";
      }
      System.out.println(line + "\t" + found + "\t" + (c.isInterface() ? "interface" : "class"));
    }
  }
}
