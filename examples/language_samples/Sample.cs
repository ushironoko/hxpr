// C# sample file for syntax highlighting test

using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

namespace SampleApp
{
    // Interface definition
    public interface IGreeter
    {
        string Greet(string name);
        void SayGoodbye();
    }

    // Abstract class
    public abstract class Animal
    {
        public string Name { get; protected set; }

        protected Animal(string name)
        {
            Name = name;
        }

        public abstract void Speak();

        public virtual void Sleep()
        {
            Console.WriteLine($"{Name} is sleeping...");
        }
    }

    // Sealed class
    public sealed class Dog : Animal, IGreeter
    {
        public string Breed { get; init; }

        public Dog(string name, string breed) : base(name)
        {
            Breed = breed;
        }

        public override void Speak()
        {
            Console.WriteLine($"{Name} the {Breed} says: Woof!");
        }

        public string Greet(string name) => $"Hello {name}, I'm {Name}!";

        public void SayGoodbye() => Console.WriteLine("Goodbye, woof!");
    }

    // Record type (C# 9+)
    public record Point(int X, int Y)
    {
        public double DistanceFromOrigin => Math.Sqrt(X * X + Y * Y);
    }

    // Struct with readonly
    public readonly struct Color
    {
        public byte R { get; }
        public byte G { get; }
        public byte B { get; }

        public Color(byte r, byte g, byte b)
        {
            R = r;
            G = g;
            B = b;
        }

        public override string ToString() => $"#{R:X2}{G:X2}{B:X2}";
    }

    // Enum with flags
    [Flags]
    public enum Permissions
    {
        None = 0,
        Read = 1,
        Write = 2,
        Execute = 4,
        All = Read | Write | Execute
    }

    // Generic class with constraints
    public class Repository<T> where T : class, new()
    {
        private readonly List<T> _items = new();

        public void Add(T item) => _items.Add(item);

        public T? Find(Func<T, bool> predicate) => _items.FirstOrDefault(predicate);

        public IEnumerable<T> GetAll() => _items.AsReadOnly();
    }

    // Extension methods
    public static class StringExtensions
    {
        public static bool IsNullOrEmpty(this string? str)
        {
            return string.IsNullOrEmpty(str);
        }

        public static string Reverse(this string str)
        {
            var chars = str.ToCharArray();
            Array.Reverse(chars);
            return new string(chars);
        }
    }

    // Main program
    public class Program
    {
        public static async Task Main(string[] args)
        {
            // Null-coalescing and null-conditional
            string? nullableString = null;
            string result = nullableString ?? "default";
            int? length = nullableString?.Length;

            // Pattern matching
            object obj = "Hello, World!";
            if (obj is string s && s.Length > 5)
            {
                Console.WriteLine($"Long string: {s}");
            }

            // Switch expression
            var dayOfWeek = DateTime.Now.DayOfWeek;
            var dayType = dayOfWeek switch
            {
                DayOfWeek.Saturday or DayOfWeek.Sunday => "Weekend",
                _ => "Weekday"
            };

            // LINQ
            var numbers = Enumerable.Range(1, 10);
            var evenSquares = numbers
                .Where(n => n % 2 == 0)
                .Select(n => n * n)
                .ToList();

            Console.WriteLine($"Even squares: {string.Join(", ", evenSquares)}");

            // Async/await
            await DoAsyncWork();

            // Using declaration
            using var reader = new System.IO.StreamReader("file.txt");

            // Object initializer
            var dog = new Dog("Buddy", "Golden Retriever")
            {
                // Any settable property can be set here
            };
            dog.Speak();

            // Anonymous type
            var person = new { Name = "Alice", Age = 30 };
            Console.WriteLine($"{person.Name} is {person.Age} years old");

            // Tuple
            var (name, age) = GetPersonInfo();
            Console.WriteLine($"Name: {name}, Age: {age}");

            // Index and Range
            var array = new[] { 1, 2, 3, 4, 5 };
            var lastTwo = array[^2..];
            Console.WriteLine($"Last two: {string.Join(", ", lastTwo)}");

            // Try-catch-finally
            try
            {
                throw new InvalidOperationException("Example exception");
            }
            catch (InvalidOperationException ex) when (ex.Message.Contains("Example"))
            {
                Console.WriteLine($"Caught: {ex.Message}");
            }
            finally
            {
                Console.WriteLine("Finally block executed");
            }
        }

        private static async Task DoAsyncWork()
        {
            await Task.Delay(100);
            Console.WriteLine("Async work completed");
        }

        private static (string Name, int Age) GetPersonInfo()
        {
            return ("Bob", 25);
        }
    }
}
