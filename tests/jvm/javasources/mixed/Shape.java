package mixed;

import java.util.List;

/** A shape; Scala's `Point` and `Square` use it and it uses them. */
public interface Shape extends Comparable<Shape> {
    int SIDES_UNKNOWN = -1;
    String KIND = "shape";

    double area();

    String name();

    default String describe(Point origin) {
        return name() + " at " + origin.show() + ", area " + Math.round(area() * 100) / 100.0;
    }

    static Shape largest(List<? extends Shape> shapes) {
        Shape best = null;
        for (Shape s : shapes) {
            if (best == null || s.compareTo(best) > 0) best = s;
        }
        return best;
    }

    @Override
    default int compareTo(Shape other) {
        return Double.compare(area(), other.area());
    }

    static Square unit() {
        return new Square(1.0);
    }
}
