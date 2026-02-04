// Zig sample file for syntax highlighting test

const std = @import("std");

pub const Color = enum {
    red,
    green,
    blue,
};

pub const Point = struct {
    x: f32,
    y: f32,

    pub fn init(x: f32, y: f32) Point {
        return Point{ .x = x, .y = y };
    }

    pub fn distance(self: Point, other: Point) f32 {
        const dx = self.x - other.x;
        const dy = self.y - other.y;
        return @sqrt(dx * dx + dy * dy);
    }
};

const LinkedList = struct {
    const Node = struct {
        data: i32,
        next: ?*Node,
    };

    head: ?*Node,
    allocator: std.mem.Allocator,

    pub fn init(allocator: std.mem.Allocator) LinkedList {
        return LinkedList{
            .head = null,
            .allocator = allocator,
        };
    }

    pub fn push(self: *LinkedList, value: i32) !void {
        const node = try self.allocator.create(Node);
        node.* = Node{
            .data = value,
            .next = self.head,
        };
        self.head = node;
    }
};

fn fibonacci(n: u32) u64 {
    if (n <= 1) return n;

    var prev: u64 = 0;
    var curr: u64 = 1;
    var i: u32 = 2;

    while (i <= n) : (i += 1) {
        const next = prev + curr;
        prev = curr;
        curr = next;
    }

    return curr;
}

pub fn main() !void {
    const stdout = std.io.getStdOut().writer();

    // Print fibonacci numbers
    var i: u32 = 0;
    while (i < 10) : (i += 1) {
        try stdout.print("fib({d}) = {d}\n", .{ i, fibonacci(i) });
    }

    // Use Point struct
    const p1 = Point.init(0.0, 0.0);
    const p2 = Point.init(3.0, 4.0);
    try stdout.print("Distance: {d}\n", .{p1.distance(p2)});

    // Color enum
    const color: Color = .green;
    switch (color) {
        .red => try stdout.print("Red!\n", .{}),
        .green => try stdout.print("Green!\n", .{}),
        .blue => try stdout.print("Blue!\n", .{}),
    }
}

test "fibonacci test" {
    try std.testing.expectEqual(@as(u64, 0), fibonacci(0));
    try std.testing.expectEqual(@as(u64, 1), fibonacci(1));
    try std.testing.expectEqual(@as(u64, 55), fibonacci(10));
}
