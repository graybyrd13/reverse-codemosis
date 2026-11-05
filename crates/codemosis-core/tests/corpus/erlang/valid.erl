%% MARK_COMMENT
-module(greeter).
-export([greet/1]).

greet(Name) ->
    "MARK_STRING" ++ Name.
