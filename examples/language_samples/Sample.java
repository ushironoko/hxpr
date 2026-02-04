// Java sample file for syntax highlighting test

package com.example.samples;

import java.util.*;
import java.util.stream.*;
import java.util.function.*;

public class Sample {
    private static final String GREETING = "Hello";
    private final String name;
    private int count = 0;

    public Sample(String name) {
        this.name = name;
    }

    public void greet() {
        System.out.println(GREETING + ", " + name + "!");
    }

    public int increment() {
        return ++count;
    }

    public static void main(String[] args) {
        // Instance creation
        Sample sample = new Sample("World");
        sample.greet();

        // Generics and collections
        List<Integer> numbers = Arrays.asList(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);

        // Stream API
        List<Integer> evenSquares = numbers.stream()
            .filter(n -> n % 2 == 0)
            .map(n -> n * n)
            .collect(Collectors.toList());

        System.out.println("Even squares: " + evenSquares);

        // Lambda expressions
        Comparator<String> byLength = (s1, s2) -> Integer.compare(s1.length(), s2.length());

        List<String> words = Arrays.asList("apple", "pie", "banana", "kiwi");
        words.sort(byLength);
        System.out.println("Sorted by length: " + words);

        // Method reference
        words.forEach(System.out::println);

        // Switch expression (Java 14+)
        String day = "MONDAY";
        String type = switch (day) {
            case "SATURDAY", "SUNDAY" -> "Weekend";
            case "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY" -> "Weekday";
            default -> "Unknown";
        };
        System.out.println(day + " is a " + type);

        // Try-catch with resources
        try (var scanner = new Scanner(System.in)) {
            // Do something
        } catch (Exception e) {
            e.printStackTrace();
        }

        // Anonymous inner class
        Runnable task = new Runnable() {
            @Override
            public void run() {
                System.out.println("Running task...");
            }
        };
        task.run();
    }
}

// Interface with default method
interface Drawable {
    void draw();

    default void clear() {
        System.out.println("Clearing...");
    }
}

// Abstract class
abstract class Shape implements Drawable {
    protected String color;

    public Shape(String color) {
        this.color = color;
    }

    public abstract double area();

    @Override
    public void draw() {
        System.out.println("Drawing " + color + " shape");
    }
}

// Record (Java 16+)
record Point(int x, int y) {
    public double distanceFromOrigin() {
        return Math.sqrt(x * x + y * y);
    }
}

// Sealed class (Java 17+)
sealed class Animal permits Dog, Cat {
    protected final String name;

    public Animal(String name) {
        this.name = name;
    }

    public void speak() {
        System.out.println(name + " makes a sound");
    }
}

final class Dog extends Animal {
    public Dog(String name) {
        super(name);
    }

    @Override
    public void speak() {
        System.out.println(name + " barks!");
    }
}

final class Cat extends Animal {
    public Cat(String name) {
        super(name);
    }

    @Override
    public void speak() {
        System.out.println(name + " meows!");
    }
}

// Enum with methods
enum Status {
    PENDING("P"),
    ACTIVE("A"),
    COMPLETED("C");

    private final String code;

    Status(String code) {
        this.code = code;
    }

    public String getCode() {
        return code;
    }
}

// Generic class
class Box<T> {
    private T content;

    public void set(T content) {
        this.content = content;
    }

    public T get() {
        return content;
    }

    public boolean isEmpty() {
        return content == null;
    }
}
