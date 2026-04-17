const std = @import("std");
const mem = @import("std").mem;

/// WHY: Custom allocator wrapper so we can track peak memory usage
/// during builds and fail early if we exceed the budget.
const TrackedAllocator = struct {
    inner: std.mem.Allocator,
    allocated: usize,
    peak: usize,

    /// Create a tracked allocator wrapping any inner allocator.
    pub fn init(inner: std.mem.Allocator) TrackedAllocator {
        return .{ .inner = inner, .allocated = 0, .peak = 0 };
    }

    // NOTE: Does not free — only resets counters.
    pub fn resetStats(self: *TrackedAllocator) void {
        self.allocated = 0;
        self.peak = 0;
    }
};

fn buildTarget(alloc: std.mem.Allocator, name: []const u8) !void {
    const msg = try std.fmt.allocPrint(alloc, "Building target: {s}\n", .{name});
    defer alloc.free(msg);
    std.debug.print("{s}", .{msg});
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    var tracker = TrackedAllocator.init(alloc);
    try buildTarget(alloc, "libcore");
    try buildTarget(alloc, "main");
    tracker.resetStats();
}
