<?php
// MARK_COMMENT
const GREETING = "MARK_STRING";

function greet(string $name): string
{
    return GREETING . ", " . $name;
}
