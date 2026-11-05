# waits 0.1.0

> Asks whether to keep fixed waits, in languages read at the lexical tier.

One rule and a table: for each language, the idioms that pause for a fixed
time (Start-Sleep, Sys.sleep, WAITFOR DELAY, Process.sleep...). Matches
count only where they start in code, never in comments or strings, so only
languages whose comment syntax is known take part. Languages with a grammar
have their own, more precise wait rules in their packs. Adding a language is
one line in tables/waits.toml.

**Languages:** any text · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 5 ex. · 3 counter |

## fixed-wait

**Fixed wait** · `waits/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed wait is a guess about timing: too short and the code is flaky (what
it waited for is not ready yet), too long and every run is slow. CI
pipelines, deployment scripts and tests collect them. The safe form waits for
the condition itself: poll the port, the file or the job, with a time limit.

Some waits are deliberate (rate limits, back-off, pacing), so the engine
asks. keep records the wait as intentional; remove deletes the line. This
rule reads text, so it cannot tell a wait inside a polling loop from one on
its own: keep the ones that pace a loop.

Examples, documentation and benchmarks are left alone (see `file_purpose()`): a pause there is part of the demonstration.

**Patterns:** one per language from the table `wait_patterns` (32 languages: AppleScript, AutoHotkey, Batchfile, Clojure, Crystal, Dart, Dockerfile, Elixir, Erlang, F#, Fortran, GDScript, Groovy, Haskell, Julia, Lua, MATLAB, Makefile, Nim, OCaml, Objective-C, PLpgSQL, Perl, PowerShell, R, Raku, SQL, TSQL, Tcl, Visual Basic .NET, YAML, Zig).

**Facts that must hold**

- `occupies_lines(@match)`
- `file_purpose() not in ['example', 'docs', 'benchmark']`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing, a polling loop); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: PowerShell** (answer: remove)

```diff
--- a/deploy.ps1
+++ b/deploy.ps1
@@ -1,3 +1,2 @@
 Write-Host "Deploying"
-Start-Sleep -Seconds 5
 Write-Host "Done"
```

**Example: a batch file's ping trick** (answer: remove)

```diff
--- a/build.bat
+++ b/build.bat
@@ -1,4 +1,3 @@
 @echo off
 echo Building
-ping -n 6 127.0.0.1 > nul
 echo Built
```

**Example: T-SQL** (answer: remove)

```diff
--- a/nightly.sql
+++ b/nightly.sql
@@ -1,4 +1,3 @@
 SET NOCOUNT ON;
 EXEC dbo.StartJob;
-WAITFOR DELAY '00:00:05';
 EXEC dbo.CheckJob;
```

**Example: R, kept** (answer: keep)

```diff
pages <- lapply(urls, download)
Sys.sleep(2)
(unchanged)
```

**Example: a Makefile recipe** (answer: remove)

```diff
--- a/Makefile
+++ b/Makefile
@@ -1,4 +1,3 @@
 up:
 	docker compose up -d
-	sleep 10
 	./migrate.sh
```

**Counterexample: a commented-out wait** (no finding)

```powershell
# Start-Sleep -Seconds 5
Write-Host "Done"
```

**Counterexample: inside a here-string** (no finding)

```powershell
$script = @"
Start-Sleep 5
"@
```

**Counterexample: a language with its own pack** (no finding)

```text
import time
time.sleep(5)
```

