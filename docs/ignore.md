# `.depradarignore` Format Reference

DepRadar supports accepted-risk suppressions through a `.depradarignore` file placed adjacent to the scanned lockfile (or in the project root). This allows teams to acknowledge and document acceptable risks without failing CI/CD builds.

---

## File Format & Syntax

The file uses TOML syntax composed of one or more `[[ignore]]` table entries.

```toml
# .depradarignore

# Example 1: Suppress a specific advisory for a specific version with an expiry date
[[ignore]]
package = "lodash"
version = "4.17.4"
advisory = "GHSA-29mw-wpgm-hmr9"
reason = "Only used during build time; upgrade tracked in issue #123"
expires = 2026-12-31

# Example 2: Suppress all findings for an internal or accepted package
[[ignore]]
package = "internal-legacy-tools"
reason = "Vendored internal utility, verified clean by security team"

# Example 3: Version-scoped suppression without expiry
[[ignore]]
package = "axios"
version = "0.21.1"
advisory = "GHSA-cph5-m8f7-6c5x"
reason = "Server-side request forgery vector not reachable in our CLI usage"
```

---

## Fields Reference

| Key | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `package` | `string` | **Yes** | The exact name of the package to suppress. |
| `reason` | `string` | **Yes** | Non-empty explanation of why this risk is accepted. DepRadar enforces non-empty justifications for accountability. |
| `version` | `string` | *No* | When set, only findings matching this exact version string are suppressed. When omitted, all versions of the package are covered. |
| `advisory` | `string` | *No* | When set, only this specific advisory identifier (e.g. `GHSA-xxxx-xxxx-xxxx`) is suppressed. When omitted, **all findings** for the package (including typosquatting findings) are suppressed. |
| `expires` | `date` (`YYYY-MM-DD`) | *No* | The last day the suppression is active (in UTC). After this calendar date has passed, the entry stops suppressing findings and emits a warning. |

---

## Matching & Suppression Rules

1. **Specific Advisory vs. Package-Wide**:
   * If `advisory` is specified: DepRadar suppresses only vulnerability findings with a matching `advisory_id` (case-insensitive).
   * If `advisory` is omitted: DepRadar suppresses all findings for the package, including typosquat and behavioral findings.

2. **Version Pinning**:
   * If `version` is specified: The suppression only applies if the lockfile dependency has the exact same version string.
   * If `version` is omitted: All installed versions of the package match the rule.

3. **Expiry Behavior**:
   * When an `expires = YYYY-MM-DD` date is reached and passed (based on UTC system time), the rule is considered **expired**.
   * Expired entries **do not suppress findings**. DepRadar emits an advisory warning during the scan alerting developers that an ignore rule has lapsed and must be reviewed or renewed.

4. **Strict TOML Subset**:
   * DepRadar uses a deterministic, dedicated parser for `.depradarignore` that only permits:
     * `[[ignore]]` table headers
     * Known keys: `package`, `version`, `advisory`, `reason`, `expires`
     * Basic strings (`"..."`) and literal strings (`'...'`)
     * Calendar dates (`YYYY-MM-DD`)
     * `#` comments
   * Any unknown keys or unexpected syntax trigger an explicit parse error rather than being silently ignored.

---

## File Discovery

When `depradar scan <path>` runs:
* If scanning a specific lockfile (e.g. `depradar scan package-lock.json`), DepRadar looks for `.depradarignore` in the same directory as the lockfile.
* If scanning a directory (e.g. `depradar scan .`), each discovered lockfile checks its containing directory for a local `.depradarignore`.
