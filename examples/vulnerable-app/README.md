# vulnerable-app

A deliberately vulnerable, **non-installable** sample project used by DepRadar
documentation, tutorials, and tests.

> Do not publish or use this project. It pins known-vulnerable dependency
> versions on purpose and includes a typo-squatted package name that does not
> correspond to a real npm package.

## What is in here

- `package.json` / `package-lock.json` (npm, lockfile v3)
- `lodash@4.17.4` (pinned in `package-lock.json`'s `node_modules/lodash`) — an
  old, vulnerable release flagged by DepRadar's CVE module.
- `lodashz` — a dependency name that is not published on npm and is one
  character away from `lodash`. DepRadar's typosquat detector reports it as:
  `lodashz: likely typosquat` / `edit distance 1 from popular package "lodash"`.
  `lodashz` has no resolved entry in `package-lock.json` because no such
  package exists.

## Expected DepRadar findings

1. **CVE / known vulnerability** — `lodash` `4.17.4` is reported as a known
   vulnerable version (see OSV for `lodash`).
2. **Typosquat** — `lodashz` is reported as a likely typosquat of the popular
   package `lodash` (edit distance 1).
3. **Missing resolution** — `lodashz` has no lockfile entry, because the name
   does not exist on npm; scanning the manifest still surfaces the name.

## Verify

```bash
cd examples/vulnerable-app
python3 -c "import json; d=json.load(open('package-lock.json')); print(d['packages']['node_modules/lodash']['version'])"
cargo run --bin scan -- lodashz
```

The first command prints `4.17.4`; the second prints the typosquat finding.
