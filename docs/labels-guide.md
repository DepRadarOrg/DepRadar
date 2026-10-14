# Issue and PR Labels Guide

## Issue Labels

### Difficulty
| Label | Description |
|-------|-------------|
| `good first issue` | Beginner-friendly task, suitable for new contributors |
| `Stellar Wave` | Funded bounty task — payout on merge via Soroban escrow |

### Area
| Label | Description |
|-------|-------------|
| `frontend` | Web frontend (React/TypeScript) |
| `contract` | Soroban smart contract (Rust) |
| `cli` | Command-line interface |
| `scanner` | Package scanning engine |
| `cve` | CVE database integration |
| `typosquat` | Typosquatting detection |
| `docs` | Documentation |
| `ci` | Continuous integration |
| `testing` | Test coverage |
| `design` | UI/UX design |

### Type
| Label | Description |
|-------|-------------|
| `enhancement` | New feature or improvement |
| `bug` | Something is broken |
| `documentation` | Documentation improvement |
| `security` | Security-related change |
| `performance` | Performance improvement |

## PR Labels

PRs inherit labels from the issues they resolve. Additionally:

| Label | Description |
|-------|-------------|
| `needs-review` | Awaiting maintainer review |
| `approved` | Approved by maintainer |
| `changes-requested` | Changes requested by reviewer |

## Workflow

1. Find an issue with `good first issue` + `Stellar Wave` labels
2. Fork the repo, create a branch, make changes
3. Open a PR referencing the issue (`closes #N`)
4. Address review feedback
5. On merge — bounty is paid via Soroban escrow + Drips

## Questions?

Open an issue with the `documentation` label.
