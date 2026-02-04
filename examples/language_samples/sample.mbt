// MoonBit sample file for syntax highlighting test

// Struct definition
struct Point {
  x : Int
  y : Int
}

// Enum definition
enum Color {
  Red
  Green
  Blue
  RGB(Int, Int, Int)
}

// Trait definition
trait Drawable {
  draw(Self) -> String
}

// Implement trait for struct
fn Point::draw(self : Point) -> String {
  "Point(\{self.x}, \{self.y})"
}

// Function with pattern matching
fn color_to_string(c : Color) -> String {
  match c {
    Red => "red"
    Green => "green"
    Blue => "blue"
    RGB(r, g, b) => "rgb(\{r}, \{g}, \{b})"
  }
}

// Generic function
fn swap[T](pair : (T, T)) -> (T, T) {
  let (a, b) = pair
  (b, a)
}

// Recursive function
fn factorial(n : Int) -> Int {
  if n <= 1 {
    1
  } else {
    n * factorial(n - 1)
  }
}

// Fibonacci with tail recursion
fn fibonacci(n : Int) -> Int {
  fn go(n : Int, a : Int, b : Int) -> Int {
    if n == 0 {
      a
    } else {
      go(n - 1, b, a + b)
    }
  }
  go(n, 0, 1)
}

// Higher-order function
fn map[A, B](arr : Array[A], f : (A) -> B) -> Array[B] {
  let result : Array[B] = []
  for i = 0; i < arr.length(); i = i + 1 {
    result.push(f(arr[i]))
  }
  result
}

// Filter function
fn filter[A](arr : Array[A], pred : (A) -> Bool) -> Array[A] {
  let result : Array[A] = []
  for i = 0; i < arr.length(); i = i + 1 {
    if pred(arr[i]) {
      result.push(arr[i])
    }
  }
  result
}

// Main function
fn main {
  // Create point
  let p = { x: 10, y: 20 }
  println(p.draw())

  // Color usage
  let c = RGB(255, 128, 0)
  println(color_to_string(c))

  // Tuple swap
  let pair = (1, 2)
  let swapped = swap(pair)
  println("Swapped: (\{swapped.0}, \{swapped.1})")

  // Math functions
  println("5! = \{factorial(5)}")
  println("fib(10) = \{fibonacci(10)}")

  // Array operations
  let numbers = [1, 2, 3, 4, 5]
  let doubled = map(numbers, fn(x) { x * 2 })
  let evens = filter(numbers, fn(x) { x % 2 == 0 })

  // Loop
  for i = 0; i < doubled.length(); i = i + 1 {
    println("doubled[\{i}] = \{doubled[i]}")
  }

  // While loop
  let mut count = 0
  while count < 3 {
    println("Count: \{count}")
    count = count + 1
  }

  // Boolean operations
  let a = true
  let b = false
  println("a && b = \{a && b}")
  println("a || b = \{a || b}")
}

// Test
test "factorial" {
  @test.eq!(factorial(0), 1)
  @test.eq!(factorial(1), 1)
  @test.eq!(factorial(5), 120)
}

test "fibonacci" {
  @test.eq!(fibonacci(0), 0)
  @test.eq!(fibonacci(1), 1)
  @test.eq!(fibonacci(10), 55)
}
