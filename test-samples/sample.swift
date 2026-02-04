// Swift sample file for tree-sitter syntax highlighting test
// Phase 1 language support

import Foundation

// Protocol definition with associated type for generic HighlightCapture
protocol Highlightable {
    associatedtype CaptureType: Highlightable
    var scopeName: String { get }
    func highlight(in range: Range<Int>) -> [HighlightCapture<CaptureType>]
}

// Enum with associated values
enum TokenType: Equatable {
    case keyword(String)
    case identifier(String)
    case literal(LiteralKind)
    case punctuation(Character)
    case comment(isMultiline: Bool)

    enum LiteralKind {
        case string, number, boolean, `nil`
    }
}

// Struct with generics
struct HighlightCapture<T: Highlightable> {
    let range: Range<Int>
    let scope: T
    let metadata: [String: Any]

    init(range: Range<Int>, scope: T, metadata: [String: Any] = [:]) {
        self.range = range
        self.scope = scope
        self.metadata = metadata
    }
}

// Class with inheritance and property wrappers
@propertyWrapper
struct Clamped<Value: Comparable> {
    private var value: Value
    let range: ClosedRange<Value>

    var wrappedValue: Value {
        get { value }
        set { value = min(max(newValue, range.lowerBound), range.upperBound) }
    }

    init(wrappedValue: Value, _ range: ClosedRange<Value>) {
        self.range = range
        self.value = min(max(wrappedValue, range.lowerBound), range.upperBound)
    }
}

class SyntaxHighlighter: Highlightable {
    let scopeName: String
    private var cache: [Int: [TokenType]] = [:]

    @Clamped(0...100) var maxIterations: Int = 50

    init(language: String) {
        self.scopeName = "source.\(language)"
    }

    func highlight(in range: Range<Int>) -> [HighlightCapture<SyntaxHighlighter>] {
        var captures: [HighlightCapture<SyntaxHighlighter>] = []

        for position in range {
            if let tokens = cache[position] {
                for token in tokens {
                    let capture = HighlightCapture(
                        range: position..<(position + 1),
                        scope: self
                    )
                    captures.append(capture)
                }
            }
        }

        return captures
    }

    // Async/await support
    func parseAsync(_ code: String) async throws -> [TokenType] {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global().async {
                let tokens = self.tokenize(code)
                continuation.resume(returning: tokens)
            }
        }
    }

    private func tokenize(_ code: String) -> [TokenType] {
        var tokens: [TokenType] = []

        let keywords = ["func", "class", "struct", "enum", "protocol", "let", "var"]
        for keyword in keywords where code.contains(keyword) {
            tokens.append(.keyword(keyword))
        }

        return tokens
    }
}

// Extension with protocol conformance
extension SyntaxHighlighter: CustomStringConvertible {
    var description: String {
        "SyntaxHighlighter(\(scopeName))"
    }
}

// Actor for thread-safe state
actor HighlighterCache {
    private var entries: [String: [TokenType]] = [:]

    func get(_ key: String) -> [TokenType]? {
        entries[key]
    }

    func set(_ key: String, tokens: [TokenType]) {
        entries[key] = tokens
    }

    func clear() {
        entries.removeAll()
    }
}

// Main execution with async context
@main
struct TreeSitterDemo {
    static func main() async {
        let highlighter = SyntaxHighlighter(language: "swift")

        let sampleCode = """
        func greet(name: String) -> String {
            return "Hello, \\(name)!"
        }
        """

        do {
            let tokens = try await highlighter.parseAsync(sampleCode)
            print("Parsed \(tokens.count) tokens")

            for token in tokens {
                switch token {
                case .keyword(let kw):
                    print("Keyword: \(kw)")
                case .identifier(let id):
                    print("Identifier: \(id)")
                default:
                    print("Other token: \(token)")
                }
            }
        } catch {
            print("Error: \(error)")
        }
    }
}
