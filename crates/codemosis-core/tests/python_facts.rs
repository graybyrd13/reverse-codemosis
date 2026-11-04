//! The Python facts that corrections depend on. Each one must answer
//! conservatively: when in doubt, block the correction.

use codemosis_core::eval::{eval, Bindings, EvalCtx, Value};
use codemosis_core::lang;
use codemosis_core::tables::Tables;
use codemosis_core::{expr, Project, SourceFile};

fn project(files: &[(&str, &str)]) -> Project {
    let py = lang::by_id("python").unwrap();
    Project::in_memory(
        files
            .iter()
            .map(|(p, t)| SourceFile::parse(*p, py, t.to_string()).unwrap())
            .collect(),
    )
}

fn fact(project: &Project, file: usize, src: &str) -> bool {
    let tables = Tables::default();
    let ctx = EvalCtx {
        file: &project.files[file],
        project,
        tables: &tables,
        bindings: Bindings::new(),
    };
    match eval(&expr::parse(src).unwrap(), &ctx).unwrap() {
        Value::Bool(b) => b,
        other => panic!("{src} gave {other:?}"),
    }
}

#[test]
fn binding_forms_are_all_seen() {
    let p = project(&[(
        "m.py",
        r#"
import os.path
from json import loads as parse
def f(a, b=1, *args, c: int = 2, **kw):
    global G
    for i, (j, k) in []:
        pass
    with open("x") as fh:
        pass
    try:
        pass
    except ValueError as err:
        pass
    [w for w in []]
    if (n := 1):
        pass
    lam = lambda q: q
class C:
    pass
match p:
    case [first, *rest]:
        pass
    case Point(x=px) as pt:
        pass
"#,
    )]);
    for name in [
        "os", "parse", "f", "a", "b", "args", "c", "kw", "G", "i", "j", "k", "fh", "err", "w", "n", "lam", "q", "C",
        "first", "rest", "px", "pt",
    ] {
        assert!(fact(&p, 0, &format!("py.bound('{name}')")), "{name} should be bound");
    }
    for name in ["json", "loads", "path", "Point", "x", "p", "open"] {
        assert!(
            !fact(&p, 0, &format!("py.bound('{name}')")),
            "{name} should not be bound"
        );
    }
}

#[test]
fn reads_inside_fstrings_count_as_references() {
    let p = project(&[("m.py", "import os\nprint(f\"{os.sep}\")\n")]);
    assert!(fact(&p, 0, "py.references('os') == 1"));
}

#[test]
fn dynamic_name_lookup_is_detected() {
    for src in [
        "from x import *\n",
        "exec('a = 1')\n",
        "globals()['a'] = 1\n",
        "def __getattr__(name):\n    pass\n",
    ] {
        let p = project(&[("m.py", src)]);
        assert!(fact(&p, 0, "py.dynamic_names()"), "{src}");
    }
    let p = project(&[("m.py", "def f(o):\n    return o.exec()\n")]);
    assert!(!fact(&p, 0, "py.dynamic_names()"));
}

#[test]
fn defined_in_project_ignores_imports_of_the_same_module() {
    let p = project(&[
        ("app.py", "print(json.dumps({}))\n"),
        ("tests/test_app.py", "import json\nfrom pathlib import Path\n"),
        ("util.py", "from mylib import Path as P\nformat_table = None\n"),
    ]);
    // `import json` elsewhere is the same module, not a rival definition.
    assert!(!fact(&p, 0, "py.defined_in_project('json', 'json')"));
    assert!(!fact(&p, 0, "py.defined_in_project('Path', 'pathlib')"));
    // A definition at module level elsewhere is a rival.
    assert!(fact(&p, 0, "py.defined_in_project('format_table')"));
    // A project module with the name is a rival.
    assert!(fact(&p, 0, "py.defined_in_project('util', 'util')"));
}

#[test]
fn imported_elsewhere_sees_reexports() {
    let p = project(&[
        ("pkg/helpers.py", "from os import sep\n"),
        ("pkg/main.py", "from pkg.helpers import sep\n"),
    ]);
    assert!(fact(&p, 0, "py.imported_elsewhere('sep')"));
    assert!(!fact(&p, 1, "py.imported_elsewhere('sep')"));
}

fn add_import(text: &str, stmt: &str) -> String {
    let p = project(&[("m.py", text)]);
    let f = &p.files[0];
    let e = f.lang.add_import(f, stmt).unwrap();
    codemosis_core::edit::apply(text, &[e]).unwrap()
}

#[test]
fn imports_go_to_the_right_place() {
    // Alphabetically into the standard-library group, before project imports.
    assert_eq!(
        add_import("\"\"\"Doc.\"\"\"\n\nimport io\n\nfrom app import x\n", "import json"),
        "\"\"\"Doc.\"\"\"\n\nimport io\nimport json\n\nfrom app import x\n"
    );
    assert_eq!(
        add_import("import io\nimport os\n", "import json"),
        "import io\nimport json\nimport os\n"
    );
    // Joining an existing single-line from-import.
    assert_eq!(
        add_import(
            "from typing import List\n\nx: List = []\n",
            "from typing import Optional"
        ),
        "from typing import List, Optional\n\nx: List = []\n"
    );
    // No imports yet: after the docstring, set off by blank lines.
    assert_eq!(
        add_import("\"\"\"Doc.\"\"\"\n\nx = 1\n", "import json"),
        "\"\"\"Doc.\"\"\"\n\nimport json\n\nx = 1\n"
    );
    // A third-party import goes after the absolute imports.
    assert_eq!(
        add_import("import os\n\nfrom . import y\n", "import requests"),
        "import os\nimport requests\n\nfrom . import y\n"
    );
}

#[test]
fn already_imported_names_are_refused() {
    let p = project(&[("m.py", "from typing import Optional\n")]);
    let f = &p.files[0];
    assert!(f.lang.add_import(f, "from typing import Optional").is_err());
}
