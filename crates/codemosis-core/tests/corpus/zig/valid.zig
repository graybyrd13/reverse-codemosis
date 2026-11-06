// MARK_COMMENT
const greeting = "MARK_STRING";

pub fn greet(name: []const u8) []const u8 {
    _ = name;
    return greeting;
}
