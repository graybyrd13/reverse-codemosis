# Fact vocabulary

The functions, template filters and edit operations that rules can use.
Every fact is conservative: when it cannot be sure, it answers in the
direction that blocks a correction. See [PACK_FORMAT.md](PACK_FORMAT.md).

This file is generated from `codemosis facts`; regenerate it with
`UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo`.

```text
Facts every language can use
  text(nodes) -> string                          Source text from the start of the first node to the end of the last.
  kind(nodes) -> string                          Grammar kind of the first node, e.g. 'identifier'.
  count(nodes | list) -> number                  Number of nodes in a capture, or items in a list.
  exists(nodes) -> bool                          True when the capture holds at least one node.
  empty(nodes) -> bool                           True when the capture holds no nodes.
  field(nodes, name) -> nodes                    Children of the first node stored under a grammar field.
  field_count(nodes, name) -> number             How many children of the first node are stored under a field.
  children(nodes) -> nodes                       Named children of the first node, excluding comments.
  parent(nodes) -> nodes                         Parent of the first node.
  next(nodes) -> nodes                           Next named sibling of the last node, skipping comments.
  prev(nodes) -> nodes                           Previous named sibling of the first node, skipping comments.
  siblings_after(nodes) -> nodes                 All named siblings after the last node, excluding comments.
  ancestor(nodes, kind...) -> nodes              Nearest ancestor of the first node with one of the kinds.
  inside(nodes, kind...) -> bool                 True when the first node has an ancestor of one of the kinds.
  contains(nodes, kind...) -> bool               True when any node, or any descendant, has one of the kinds.
  has_comment(nodes) -> bool                     True when a comment lies inside the span of the nodes.
  has_error(nodes) -> bool                       True when the nodes contain a syntax error.
  multiline(nodes) -> bool                       True when the nodes span more than one line.
  same_line(a, b) -> bool                        True when a and b start on the same line.
  occupies_lines(nodes) -> bool                  True when the nodes are alone on their lines (only whitespace around them).
  line(nodes) -> number                          1-based line where the first node starts.
  matches(text, regex) -> bool                   True when the regular expression matches anywhere in the text.
  starts_with(text, prefix) -> bool              Text prefix test.
  ends_with(text, suffix) -> bool                Text suffix test.
  lower(text) -> string                          Lower-cased text.
  len(text | list) -> number                     Length of text in characters, or of a list.
  in_table(table, key) -> bool                   True when the pack table contains the key.
  table(table, key) -> list                      The list stored under a key of a pack map table (empty when absent).
  lookup(table, key) -> string                   The first string stored under a key of a pack map table (empty when absent).
  choose(condition, a, b) -> value               a when the condition is true, otherwise b.
  subset(list, list) -> bool                     True when every item of the first list is in the second.
  concat(text...) -> string                      The texts joined together.
  bang(text) -> string                           The C-style negation of an expression: `!x`, parenthesised unless x is a single name, member access or call.
  count_matches(text, regex) -> number           How many times the regular expression matches in the text.
  file_matches(regex) -> number                  How many times the regular expression matches in the whole file, in code only (not comments or strings) when the file's comments are known.
  kind_at(capture) -> string                     What the capture starts in: 'code', 'comment', 'string', or 'unknown' when the file's comments and strings cannot be told apart.
  language() -> string                           The file's language, as GitHub names it: 'Python', 'Shell', 'PowerShell'...
  language_kind() -> string                      GitHub's type for the file's language: 'programming', 'markup', 'data' or 'prose'.
  tier() -> string                               How deeply the file is understood: 'deep' (syntax tree), 'lexical' (comments and strings) or 'recognized'.
  file_name() -> string                          Name of the file being examined, e.g. '__init__.py'.
  file_path() -> string                          Path of the file relative to the project root.

Template filters
  trim                   Remove surrounding whitespace.
  lower                  Lower-case the text.
  upper                  Upper-case the text.
  line                   The 1-based line number of the capture instead of its text.
  strip_parens           Remove one pair of parentheses that encloses the whole text.
  unescape_braces        Turn '{{' into '{' and '}}' into '}'.
  bang                   The C-style negation `!x`, parenthesised unless x is a single name, member access or call.
  drop_string_prefix     Remove a letter (the argument) from a string literal's prefix: ${s|drop_string_prefix:f}.
  add_string_prefix      Add a letter (the argument) to a string literal's prefix: ${s|add_string_prefix:f}.

Edit operations
  replace                replace a capture with `with`
  delete                 delete a capture's text
  delete_lines           delete the whole lines a capture occupies
  delete_item            delete a list item with its comma
  insert_before          insert `with` before a capture
  insert_after           insert `with` after a capture
  insert_lines_before    insert lines before a capture's line, at its indentation
  insert_lines_after     insert lines after a capture's line, at its indentation
  add_import             add an import statement where the language puts imports

python facts (py.*)
  py.bound(name) -> bool                         True when anything in the file binds or declares the name (assignment, import, def, class, parameter, loop target, global...).
  py.bindings(name) -> number                    How many places in the file bind the name.
  py.references(name) -> number                  How many places in the file read the name (string interpolations included).
  py.mentioned(name) -> bool                     True when the name appears as a word inside any string or comment (e.g. __all__, getattr, doctests, type comments).
  py.dynamic_names() -> bool                     True when the file can create or look up names at run time: star imports, globals(), locals(), vars(), exec(), eval(), __import__(), or a module-level __getattr__.
  py.builtin(name) -> bool                       True when the name is a Python builtin.
  py.attributes(name) -> list                    For every read of the name, the attribute accessed on it (`json.dumps` gives 'dumps'); a read that is not an attribute access gives ''.
  py.is_reference(nodes) -> bool                 True when the identifier reads a name (rather than binding it, or being an attribute or keyword name).
  py.removal_preserves_scope(nodes) -> bool      True when deleting the statements cannot change how any name resolves: they contain no yield, global or nonlocal at their own level, and inside a function every name they bind is also bound elsewhere in that function.
  py.first_statement(block) -> nodes             The first statement of a block that is not its docstring (empty when there is none).
  py.is_docstring(nodes) -> bool                 True when the node is (or is the string of) a module, class or function docstring.
  py.top_level(nodes) -> bool                    True when the statement sits directly in the module, not inside a block.
  py.imported_elsewhere(name) -> bool            True when another file of the project imports the name from this module, or reads `module.name`.
  py.defined_in_project(name, module?) -> bool   True when another file of the project defines the name at module level, or the project has a module or package of that name: importing it from the standard library would then be a guess. Bindings that import the name from `module` itself (`import json`, `from pathlib import Path`) are not counted.
  py.string_prefix(string) -> string             The lower-cased prefix letters of a string literal: '', 'f', 'rb'...
  py.placeholders_bound(string) -> bool          True when a plain string contains at least one {placeholder}, every brace belongs to a well-formed placeholder, and each placeholder's name is bound in a scope visible from the string.
  py.yields_bool(expr) -> bool                   True when the expression always evaluates to True or False: `not x`, comparisons using only is / is not / in / not in, boolean literals, and and/or of such expressions.
  filter py.as_bool                              The expression if it always yields a bool, otherwise bool(expression).
  filter py.negate                               The logical negation of the expression, parenthesised as needed.

javascript, typescript, tsx facts (js.*)
  js.defines(name) -> bool                       True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

go facts (go.*)
  go.defines(name) -> bool                       True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

java facts (java.*)
  java.defines(name) -> bool                     True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

c, cpp facts (c.*)
  c.defines(name) -> bool                        True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

csharp facts (cs.*)
  cs.defines(name) -> bool                       True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

ruby facts (rb.*)
  rb.defines(name) -> bool                       True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

php facts (php.*)
  php.defines(name) -> bool                      True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

bash facts (sh.*)
  sh.defines(name) -> bool                       True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

kotlin facts (kt.*)
  kt.defines(name) -> bool                       True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

scala facts (scala.*)
  scala.defines(name) -> bool                    True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

lua facts (lua.*)
  lua.defines(name) -> bool                      True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.

swift facts (swift.*)
  swift.defines(name) -> bool                    True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.
```
