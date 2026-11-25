# Glossary

Short definitions of the terms DepRadar uses in code, findings and reports.
Each entry links to the implementation so a reader can jump from the word to
the behaviour without grepping the tree.

## Naming attacks

**Typosquat** — a package name copied from a popular package with a small
edit (one character swapped, dropped or doubled) so that a typo in an install
command resolves to the attacker's package. DepRadar scores every candidate
name against the popular-package corpus and reports the closest match.
Implementation: `src/typosquat/scanner.rs` (`score_package`), entry point
`src/bin/depradar.rs` (`depradar` name check).

**Combosquat** — an attacker name built by gluing a trusted brand onto a
generic suffix or prefix (`lodash-pay`, `pay-lodash`). Detection splits the
candidate on `-`, `_` and `.` and flags segments that match a known package.
Implementation: `src/typosquat/combosquat.rs`.

**Homoglyph** — a name that renders identically (or nearly) to a trusted one
because it uses look-alike characters: Cyrillic `а` for Latin `a`, `0` for
`o`, `rn` for `m`. Reported as `homoglyph_match` in the scan result.
Implementation: `src/bin/depradar.rs` (`homoglyph_match` in the JSON output),
`src/pipeline.rs`.

**Damerau-Levenshtein distance** — the edit distance DepRadar uses to decide
how "squatted" a name is. It counts insertions, deletions, substitutions and
**transpositions** of adjacent characters, so `lodahs` scores 1 instead of 2.
Design note: `docs/issue-23-damerau-levenshtein.md`.

**Ecosystem-scoped scoring** — the same name can be safe on npm and hostile on
PyPI, because popularity differs per registry. Scores are computed per
`ecosystem_scope` rather than globally, and PyPI names are normalised (case,
`-` vs `_`, `.`) before comparison. Design note:
`docs/issue-24-ecosystem-scoped-scoring.md`,
`docs/issue-25-pypi-name-normalization.md`.

## Scan pipeline

**`lockfile_ast`** — the parsed intermediate form of a lockfile (npm
`package-lock.json`, `Cargo.lock`, `poetry.lock`, …) before it is walked for
findings. One parser per ecosystem, all producing the same shape so the
scoring stage is ecosystem-agnostic. Entry point:
`src/bin/depradar/repo_commands.rs` (`repo-scan`).

**`finding_kind`** — the discriminant on a finding that says *which* detector
fired (`typosquat`, `homoglyph_match`, `combosquat`, `cve`, …). Downstream
report writers switch on it; new detectors must add a variant rather than
reuse an existing one.

**`suppression_rule`** — a user-declared exemption (`.depradarignore`) that
marks a finding as accepted. Suppressed findings are still emitted with their
rule attached so audits can tell "clean" from "waived".

## Bounty and review flow

**`proof_ref`** — the hash reference attached to a repo-scan submission that
pins *exactly which* scanned state produced the report. The contract command
passes it as `--proof_ref=<hash>`; reviewers can re-run the scan against that
state and get the same findings.
Implementation: `src/bin/depradar/repo_commands.rs`.

**`review_window`** — the period after a scan is submitted during which the
maintainer (or a reviewer) can inspect findings before payout is settled. A
finding outside the window is not eligible for a bounty even if it turns out
to be correct later.

**PROGRESS.md entry** — every merged change records its issue number and one
line of scope in `PROGRESS.md`. The file is the project's audit trail for
bounty accounting; a PR without an entry is not reviewable. See
`CONTRIBUTING.md` → *Checklist*.

## Related reading

- [Architecture](architecture.md) — how the scan, scoring and report stages fit together
- [Threat model](threat-model.md) — what DepRadar does and does not defend against
- [Severity](severity.md) — how a finding maps to a severity level
- [FAQ](FAQ.md) — payout, assignment and review questions
</content>
