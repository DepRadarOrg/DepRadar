# The `issue_ref` format

Every bounty on the router carries an `issue_ref` string (up to 256 bytes,
see `MAX_REF_LEN` in
[`contracts/deprader_router/src/lib.rs`](../contracts/deprader_router/src/lib.rs)).
The contract treats it as opaque text. To let the scanner, the CLI and the
dashboard link a bounty back to the finding it pays for, DepRadar uses one
standard format for refs built from a finding:

```text
<ecosystem>:<package>[@<version>]#<kind>[:<detail>]
```

| Part | Meaning |
| --- | --- |
| `ecosystem` | `npm`, `pypi`, `crates.io`, `go`, `maven`, `rubygems` or `packagist`. Lowercase when written; parsed case-insensitively. |
| `package` | The package name exactly as the registry knows it, including an npm scope (`@babel/core`) or a Maven `group:artifact`. No whitespace. |
| `version` | The pinned version, when the lockfile records one. Omitted for unpinned dependencies. |
| `kind` | `typosquat`, `vulnerability`, `deprecated` or `license`, matching [`FindingKind`](../src/model.rs). |
| `detail` | Optional. The advisory ID for a `vulnerability`, the impersonated package for a `typosquat`, or the offending license for a `license` finding. |

## Examples

```text
npm:expresss@1.0.0#typosquat:express
pypi:requests@2.19.0#vulnerability:GHSA-x84v-xcm2-53pg
npm:@babel/core@7.0.0#deprecated
maven:org.apache.logging.log4j:log4j-core@2.14.1#vulnerability:GHSA-jfh8-c2jp-5v3q
crates.io:some-crate@0.1.0#license:GPL-3.0-only
```

## Parsing rules

1. The ecosystem is everything before the first `:`.
2. The kind starts after the first `#`; an optional `:` after the kind
   separates the detail, which runs to the end of the string.
3. In between, the version starts after the last `@`, unless that `@` is the
   first character (an npm scope).

Refs that don't follow the format — such as older free-form refs like
`npm:expresss typosquat of express (demo)` — are not rejected by the
contract. Parsers return "no match" for them and callers fall back to
showing the raw text.

## Implementations

- Rust: `format_issue_ref`, `parse_issue_ref` and `Finding::issue_ref` in
  [`src/model.rs`](../src/model.rs).
- TypeScript: `formatIssueRef` and `parseIssueRef` in
  [`frontend/src/lib/format.ts`](../frontend/src/lib/format.ts).

Both are covered by round-trip tests; keep them in sync when changing the
format.
