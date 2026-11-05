-- MARK_COMMENT
module Greeter where

greeting :: String
greeting = "MARK_STRING"

greet :: String -> String
greet name = greeting ++ ", " ++ name
