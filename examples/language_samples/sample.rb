# Ruby sample file for syntax highlighting test

module Greeter
  class Person
    attr_reader :name
    attr_accessor :age

    def initialize(name, age = 0)
      @name = name
      @age = age
    end

    def greet
      puts "Hello, my name is #{@name}!"
    end

    def birthday!
      @age += 1
      yield self if block_given?
    end
  end
end

class Developer < Greeter::Person
  LANGUAGES = %w[Ruby Python JavaScript].freeze

  def initialize(name, age, languages: [])
    super(name, age)
    @languages = languages
  end

  def code(language)
    return unless LANGUAGES.include?(language)

    case language
    when "Ruby"
      puts "Writing elegant code..."
    when "Python"
      puts "import this"
    else
      puts "console.log('Hello')"
    end
  end

  private

  def secret_method
    "This is private"
  end
end

# Main execution
if __FILE__ == $PROGRAM_NAME
  dev = Developer.new("Alice", 30, languages: ["Ruby"])
  dev.greet
  dev.code("Ruby")

  3.times do |i|
    puts "Iteration #{i + 1}"
  end

  numbers = [1, 2, 3, 4, 5]
  squares = numbers.map { |n| n ** 2 }
  puts squares.inspect
end
