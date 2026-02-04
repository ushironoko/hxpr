// C++ sample file for syntax highlighting test

#include <iostream>
#include <vector>
#include <memory>
#include <string>
#include <algorithm>
#include <concepts>

namespace geometry {

template<typename T>
concept Numeric = std::is_arithmetic_v<T>;

template<Numeric T>
class Point {
public:
    T x, y;

    Point(T x = 0, T y = 0) : x(x), y(y) {}

    Point operator+(const Point& other) const {
        return Point(x + other.x, y + other.y);
    }

    bool operator==(const Point& other) const = default;

    T distance_squared(const Point& other) const {
        T dx = x - other.x;
        T dy = y - other.y;
        return dx * dx + dy * dy;
    }
};

} // namespace geometry

class Animal {
public:
    virtual ~Animal() = default;
    virtual void speak() const = 0;
    virtual std::string name() const = 0;
};

class Dog final : public Animal {
private:
    std::string m_name;

public:
    explicit Dog(std::string name) : m_name(std::move(name)) {}

    void speak() const override {
        std::cout << m_name << " says: Woof!" << std::endl;
    }

    std::string name() const override {
        return m_name;
    }
};

class Cat final : public Animal {
private:
    std::string m_name;

public:
    explicit Cat(std::string name) : m_name(std::move(name)) {}

    void speak() const override {
        std::cout << m_name << " says: Meow!" << std::endl;
    }

    std::string name() const override {
        return m_name;
    }
};

// Template function with auto return type
template<typename Container>
auto sum(const Container& c) {
    typename Container::value_type total = 0;
    for (const auto& elem : c) {
        total += elem;
    }
    return total;
}

// Lambda and STL algorithms
void demonstrate_lambda() {
    std::vector<int> numbers = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};

    // Filter even numbers
    std::vector<int> evens;
    std::copy_if(numbers.begin(), numbers.end(),
                 std::back_inserter(evens),
                 [](int n) { return n % 2 == 0; });

    // Transform with lambda
    std::vector<int> squared;
    std::transform(numbers.begin(), numbers.end(),
                   std::back_inserter(squared),
                   [](int n) { return n * n; });

    std::cout << "Even numbers: ";
    for (int n : evens) {
        std::cout << n << " ";
    }
    std::cout << std::endl;
}

int main() {
    // Smart pointers
    std::unique_ptr<Animal> dog = std::make_unique<Dog>("Buddy");
    std::shared_ptr<Animal> cat = std::make_shared<Cat>("Whiskers");

    dog->speak();
    cat->speak();

    // Template usage
    geometry::Point<double> p1(0.0, 0.0);
    geometry::Point<double> p2(3.0, 4.0);

    std::cout << "Distance squared: " << p1.distance_squared(p2) << std::endl;

    // STL containers
    std::vector<int> numbers = {5, 2, 8, 1, 9};
    std::sort(numbers.begin(), numbers.end());

    std::cout << "Sorted: ";
    for (const auto& n : numbers) {
        std::cout << n << " ";
    }
    std::cout << std::endl;

    std::cout << "Sum: " << sum(numbers) << std::endl;

    // Lambda demonstration
    demonstrate_lambda();

    // Try-catch
    try {
        throw std::runtime_error("Example exception");
    } catch (const std::exception& e) {
        std::cerr << "Caught: " << e.what() << std::endl;
    }

    return 0;
}
