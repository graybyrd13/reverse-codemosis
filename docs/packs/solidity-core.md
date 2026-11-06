# solidity-core 0.1.0

> Explanations for Solidity: authorization with tx.origin, selfdestruct.

**Languages:** solidity · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`selfdestruct`](#selfdestruct) selfdestruct | hazard | explains | 1 ex. · 1 counter |
| [`tx-origin-auth`](#tx-origin-auth) Authorization with tx.origin | hazard | explains | 1 ex. · 1 counter |

## selfdestruct

**selfdestruct** · `solidity-core/selfdestruct`

category: hazard · action: explains · CWE-477 · tags: security, deprecated

`selfdestruct` was deprecated by EIP-6049, and since EIP-6780 (the Cancun upgrade) it only sends the
contract's ether away, except in the transaction that created the contract: the code and storage stay.
Code that relies on it to disable a contract no longer works, and anyone able to call it can still drain
the balance. Use an explicit pause or withdrawal pattern. Nothing is changed automatically.

**Shape**

```scheme
(call_expression function: (expression (identifier) @function) (#eq? @function "selfdestruct")) @call
```

**Example: 1**

```text
pragma solidity ^0.8.0;

contract Box {
    address payable owner;

    function close() public {
        selfdestruct(owner);
    }
}
```

**Counterexample: 1** (no finding)

```text
pragma solidity ^0.8.0;

contract Box {
    bool closed;

    function close() public {
        closed = true;
    }
}
```


## tx-origin-auth

**Authorization with tx.origin** · `solidity-core/tx-origin-auth`

category: hazard · action: explains · CWE-863 · tags: security, swc-115

`tx.origin` is the account that signed the transaction, however many contracts it passed through. A
malicious contract the owner is tricked into calling can then call this one, and `tx.origin` is still the
owner (SWC-115). Use `msg.sender` for authorization. Nothing is changed automatically: replacing it changes
who may call, which is a design decision.

`tx.origin == msg.sender`, which only checks that the caller is not a contract, is not reported.

**Shape**

```scheme
(binary_expression
  left: (expression (member_expression object: (identifier) @object property: (identifier) @property))
  operator: ["==" "!="]
  right: (expression) @other
  (#eq? @object "tx") (#eq? @property "origin")) @cmp
(binary_expression
  left: (expression) @other
  operator: ["==" "!="]
  right: (expression (member_expression object: (identifier) @object property: (identifier) @property))
  (#eq? @object "tx") (#eq? @property "origin")) @cmp
```

**Facts that must hold**

- `text(@other) != 'msg.sender'`

**Example: 1**

```text
pragma solidity ^0.8.0;

contract Wallet {
    address owner;

    function withdraw(uint amount) public {
        require(tx.origin == owner);
        payable(msg.sender).transfer(amount);
    }
}
```

**Counterexample: only callers that are not contracts** (no finding)

```text
pragma solidity ^0.8.0;

contract Mint {
    function mint() public {
        require(tx.origin == msg.sender);
    }
}
```

