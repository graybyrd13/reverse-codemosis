# powershell-core 0.1.0

> Explanations for PowerShell: running strings as code.

**Languages:** powershell · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`invoke-expression`](#invoke-expression) Invoke-Expression | hazard | explains | 1 ex. · 2 counter |

## invoke-expression

**Invoke-Expression** · `powershell-core/invoke-expression`

category: hazard · action: explains · CWE-95 · tags: security, injection

`Invoke-Expression` (alias `iex`) evaluates a string as PowerShell. If any part of the string comes from
input, a file or the network, whoever controls it runs code with this script's rights. Call the command
directly, use the call operator `&` with arguments, or a script block. Nothing is changed automatically:
the safe form depends on what the string contains.

Tests, examples and documentation are left alone: what they evaluate is fixed by their author.

**Shape**

```scheme
(command command_name: (command_name) @name) @cmd
```

**Facts that must hold**

- `matches(text(@name), '(?i)^(invoke-expression|iex)$')`
- `file_purpose() in ['library', 'script']`

**Example: 1**

```text
$command = Read-Host "Command"
Invoke-Expression $command
```

**Counterexample: 1** (no finding)

```text
$name = Read-Host "Name"
Get-Process -Name $name
```

**Counterexample: a test builds and runs its own command** (no finding)

```powershell
$command = "Get-Date"
Invoke-Expression $command
```

