# MARK_COMMENT
let
  greeting = "MARK_STRING";
  greet = name: "${greeting}, ${name}";
in
greet "world"
