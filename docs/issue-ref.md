# Standard `issue_ref` Format for Findings

## Overview

DepRadar contracts and clients use an `issue_ref` string (stored on-chain in `Bounty.issue_ref`) to describe the vulnerability or security finding for which a reward is offered.

To allow off-chain scanners, CLI tools, automated bots, and the frontend dashboard to deterministically parse, link, and display finding details, DepRadar adopts a standard finding reference grammar.

---

## Grammar

```text
<ecosystem>:<package>[@<version>]#<kind>[:<advisory>]
```

### Components

| Field | Required | Description | Examples |
|---|---|---|---|
| `<ecosystem>` | **Yes** | Registry/package ecosystem in lowercase. | `npm`, `pypi`, `crates`, `rubygems`, `go` |
| `<package>` | **Yes** | Name of the affected package. Supports scoped/namespaced packages. | `expresss`, `requests`, `@fastify/deepmerge` |
| `<version>` | *Optional* | Specific version affected (prefixed by `@`). If omitted, applies to all versions or package identity. | `1.0.0`, `2.31.0` |
| `<kind>` | **Yes** | Classification of the security finding (prefixed by `#`). | `typosquat`, `cve`, `malicious`, `yanked`, `license` |
| `<advisory>` | *Optional* | Additional finding context (prefixed by `:` after kind). For CVEs, this is the advisory ID (GHSA/CVE). For typosquats, this is the legitimate target package. | `express`, `GHSA-9265-m339-ch94`, `CVE-2023-12345` |

---

## Canonical Examples

### 1. Typosquat targeting a popular package
```text
npm:expresss@1.0.0#typosquat:express
```
* **Ecosystem:** `npm`
* **Package:** `expresss`
* **Version:** `1.0.0`
* **Kind:** `typosquat`
* **Target:** `express`

### 2. Known CVE Vulnerability
```text
npm:@fastify/deepmerge@1.3.0#cve:GHSA-9265-m339-ch94
```
* **Ecosystem:** `npm`
* **Package:** `@fastify/deepmerge`
* **Version:** `1.3.0`
* **Kind:** `cve`
* **Advisory:** `GHSA-9265-m339-ch94`

### 3. Malicious Package (Unversioned)
```text
pypi:requests-auth-fake#malicious
```
* **Ecosystem:** `pypi`
* **Package:** `requests-auth-fake`
* **Version:** None (`null`)
* **Kind:** `malicious`
* **Advisory:** None (`null`)

---

## Implementations

### Rust (`deprader::model`)

Exposes `FindingRef`, `format_issue_ref`, and `parse_issue_ref`:

```rust
use deprader::model::{FindingRef, format_issue_ref, parse_issue_ref};

let finding = FindingRef {
    ecosystem: "npm".to_string(),
    package: "expresss".to_string(),
    version: Some("1.0.0".to_string()),
    kind: "typosquat".to_string(),
    advisory: Some("express".to_string()),
};

let ref_str = format_issue_ref(&finding);
assert_eq!(ref_str, "npm:expresss@1.0.0#typosquat:express");

let parsed = parse_issue_ref(&ref_str).unwrap();
assert_eq!(parsed, finding);
```

### TypeScript (`frontend/src/lib/format.ts`)

Mirrors the parser and formatter for the web dashboard and CLI:

```typescript
import { formatIssueRef, parseIssueRef } from "./format";

const finding = parseIssueRef("npm:expresss@1.0.0#typosquat:express");
console.log(finding.package); // "expresss"
console.log(finding.advisory); // "express"
```

---

## Backward Compatibility

Existing free-form demo strings (e.g. `npm:expresss typosquat of express (demo)`) do not match the `#<kind>` delimiter. Frontend and client utilities fall back gracefully to raw text when `parse_issue_ref` returns `None` / `null`.
