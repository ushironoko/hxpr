// Header file sample for syntax highlighting test
// Note: .h files are treated as C++ in octorus

#ifndef SAMPLE_H
#define SAMPLE_H

#include <string>
#include <vector>
#include <functional>

namespace utils {

// Forward declarations
class Logger;
struct Config;

// Type aliases
using StringList = std::vector<std::string>;
using Callback = std::function<void(int)>;

// Constants
constexpr int MAX_BUFFER_SIZE = 4096;
constexpr double PI = 3.14159265358979323846;

// Enum class
enum class LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Fatal
};

// Struct definition
struct Config {
    std::string name;
    int port;
    bool debug_mode;
    LogLevel log_level;

    Config() : name("default"), port(8080), debug_mode(false), log_level(LogLevel::Info) {}
};

// Abstract base class
class Logger {
public:
    virtual ~Logger() = default;

    virtual void log(LogLevel level, const std::string& message) = 0;
    virtual void flush() = 0;

    void info(const std::string& msg) { log(LogLevel::Info, msg); }
    void error(const std::string& msg) { log(LogLevel::Error, msg); }

protected:
    bool enabled_ = true;
};

// Template class
template<typename T>
class Optional {
public:
    Optional() : has_value_(false) {}
    explicit Optional(T value) : value_(std::move(value)), has_value_(true) {}

    bool has_value() const { return has_value_; }

    T& value() {
        if (!has_value_) {
            throw std::runtime_error("No value present");
        }
        return value_;
    }

    const T& value() const {
        if (!has_value_) {
            throw std::runtime_error("No value present");
        }
        return value_;
    }

    T value_or(T default_value) const {
        return has_value_ ? value_ : default_value;
    }

private:
    T value_;
    bool has_value_;
};

// Free function declarations
void initialize();
void shutdown();
int process_data(const StringList& data, Callback callback);

// Inline function
inline int square(int x) {
    return x * x;
}

// Template function
template<typename T, typename U>
auto add(T a, U b) -> decltype(a + b) {
    return a + b;
}

} // namespace utils

#endif // SAMPLE_H
