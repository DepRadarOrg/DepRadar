# `depradar.toml` Configuration Reference

When `depradar scan` runs, it checks the current working directory for a project-level configuration file named `depradar.toml`. If present, its values serve as defaults for scanning runs.

Command-line flags always override file values, and file values override built-in defaults:

$$\text{CLI Flags} > \text{\texttt{depradar.toml}} > \text{Built-in Defaults}$$

---

## Annotated Example

```toml
# depradar.toml

# Severity threshold at which `depradar scan` exits with code 1.
# Options: "none", "low", "medium", "high", "critical"
# Default: "high"
fail_on = "high"

# Output format for scan results.
# Options: "text", "json"
# Default: "text"
format = "text"

# Glob patterns for lockfile paths to exclude from scanning.
# Patterns match paths relative to the scanned directory root.
# Default: []
exclude = [
  "vendor/**",
  "third_party/**",
  "**/test/fixtures/**"
]

# Restrict dependency scanning to specific ecosystems.
# Options: "npm", "pypi", "cargo", "go", "maven", "rubygems", "packagist"
# Default: [] (scans all supported ecosystems)
ecosystems = ["npm", "cargo"]

# Project internal package names to monitor for dependency confusion.
# Any matching package found on a public registry triggers a finding.
# Default: []
private_packages = [
  "@my-org/core-utils",
  "internal-auth-service"
]
```

---

## Configuration Keys Reference

| Key | Type | Default | Overriding CLI Flag | Description |
| :--- | :--- | :--- | :--- | :--- |
| `fail_on` | `string` | `"high"` | `--fail-on <LEVEL>` | Sets the minimum finding severity that causes `depradar scan` to exit with status code `1`. When set to `"none"`, the scan command will never exit non-zero due to findings. |
| `format` | `string` | `"text"` | `-f`, `--format <FORMAT>` | Determines stdout presentation. `"text"` renders a human-readable terminal table; `"json"` emits structured JSON lines suitable for CI pipelines and scripts. |
| `exclude` | `array of strings` | `[]` | `--exclude <GLOB>` | List of glob patterns matching lockfile paths to ignore during scanning. The `--exclude` flag may be passed multiple times on the command line. |
| `ecosystems` | `array of strings` | `[]` (all) | `--ecosystem <NAME>` | Restricts discovery to the specified package ecosystems. When empty, all supported lockfiles are parsed. The `--ecosystem` flag may be repeated on the command line. |
| `private_packages` | `array of strings` | `[]` | *(Config file only)* | Defines proprietary or internal package names. The dependency confusion module checks each against public registries (such as npm) to identify public naming collisions. |

---

## Value Details

### `fail_on`
Controls process exit code behavior for CI/CD gates:
* `"none"`: Process always exits with `0` on successful scan completion, regardless of findings.
* `"low"`: Exits with code `1` if any finding of severity `Low`, `Medium`, `High`, or `Critical` is encountered.
* `"medium"`: Exits with code `1` for `Medium`, `High`, or `Critical` findings.
* `"high"` *(default)*: Exits with code `1` only if `High` or `Critical` severity issues are detected.
* `"critical"`: Exits with code `1` strictly for `Critical` severity vulnerabilities or attacks.

### `format`
* `"text"` *(default)*: Pretty-printed console output including tables, colors, confidence scores, and advisory references.
* `"json"`: Single JSON payload sent to stdout. Diagnostics and progress indicators remain on stderr.

### `ecosystems`
Supported ecosystem identifier strings:
* `"npm"`: Node.js packages (`package-lock.json`, `npm-shrinkwrap.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`)
* `"pypi"`: Python packages (`Pipfile.lock`, `poetry.lock`, `requirements*.txt`)
* `"cargo"`: Rust crates (`Cargo.lock`)
* `"go"`: Go modules (`go.sum`)
* `"maven"`: Java / JVM artifacts
* `"rubygems"`: Ruby gems (`Gemfile.lock`)
* `"packagist"`: PHP Composer packages (`composer.lock`)

---

## File Discovery & Error Handling

* **Location:** DepRadar searches for `depradar.toml` in the current working directory where the command is executed.
* **Optional:** If no `depradar.toml` exists in the directory, DepRadar proceeds silently with built-in defaults.
* **Validation:** If `depradar.toml` exists but cannot be read or contains unknown keys/invalid syntax, DepRadar fails early with an error:
  * `could not read depradar.toml: <io_error>`
  * `invalid depradar.toml: <toml_parse_error>`
