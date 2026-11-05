// SPDX-License-Identifier: MIT
// MARK_COMMENT
pragma solidity ^0.8.0;

contract Greeter {
    string public greeting = "MARK_STRING";

    function greet() public view returns (string memory) {
        return greeting;
    }
}
