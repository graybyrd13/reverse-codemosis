// Copy the JSON Schemas of Reverse Codemosis's file formats into the
// extension, which validates codemosis.toml, codemosis.lock, pack.toml,
// proof certificates and baselines with them.
const fs = require("fs");
const path = require("path");

const from = path.join(__dirname, "..", "..", "..", "schemas");
const to = path.join(__dirname, "..", "schemas");
fs.mkdirSync(to, { recursive: true });
for (const name of fs.readdirSync(from).filter((n) => n.endsWith(".json"))) {
  fs.copyFileSync(path.join(from, name), path.join(to, name));
}

// The licences ship with the extension too.
for (const name of ["LICENSE-MIT", "LICENSE-APACHE"]) {
  fs.copyFileSync(path.join(__dirname, "..", "..", "..", name), path.join(__dirname, "..", name));
}
