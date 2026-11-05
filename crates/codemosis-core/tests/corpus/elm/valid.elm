module Greeter exposing (greet)

-- MARK_COMMENT


greeting : String
greeting =
    "MARK_STRING"


greet : String -> String
greet name =
    greeting ++ ", " ++ name
