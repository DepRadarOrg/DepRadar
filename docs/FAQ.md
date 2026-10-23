# Frequently Asked Questions

This FAQ answers common questions about DepRadar, its bounty program, and how to report findings. For full details, see the [README](../README.md), the contract source, and [SECURITY.md](../SECURITY.md).

## General

### Is this mainnet?

The README and deployment notes describe which networks the project targets. Check the README's deployment section for the currently supported network and contract addresses. Do not assume a deployment is production-ready unless the README says so explicitly. Testnet and mainnet addresses should never be mixed up when reporting issues.

### What is DepRadar?

DepRadar is a dependency-radar project. The README describes its goals, components, and how the contract and off-chain pieces fit together. Start there for an overview.

### Where do I start contributing?

Read [CONTRIBUTING.md](../CONTRIBUTING.md) first. It explains how to set up the repository, run tests, and open pull requests. Comment on the relevant issue to be assigned before starting work on a bounty.

## Bounties and Claims

### Who decides whether a claim is valid?

The maintainers listed in the repository decide whether a bounty claim is accepted. Acceptance is based on the issue's acceptance criteria and the contribution guidelines. If you disagree with a decision, comment on the issue or PR with specifics and request a review.

### What counts as a finding?

A finding is a verified security or correctness issue that is reproducible and within scope as defined in [SECURITY.md](../SECURITY.md). Speculative concerns without a reproduction, issues in out-of-scope components, and already-known issues generally do not qualify. Follow the disclosure process in SECURITY.md when reporting.

### How do I claim a bounty?

Comment on the issue asking to be assigned. Once assigned, implement the change, verify every command and link you document, add a `PROGRESS.md` entry, and open a pull request whose description includes `Closes #<issue number>`.

### Can I work on an issue that is already assigned?

No. Wait for the current assignee to finish or for the issue to be unassigned. Coordinate in the issue thread if you believe work has stalled.

### What are the acceptance criteria for docs issues?

Documentation changes must have all commands and links verified, a `PROGRESS.md` entry, and a PR description that closes the issue. Keep changes minimal and focused on the stated tasks.

## Security

### How do I report a vulnerability?

Do not open a public issue for a security vulnerability. Follow the reporting instructions in [SECURITY.md](../SECURITY.md), which describe the private disclosure channel and expected response times.

### Are findings in dependencies in scope?

It depends on the scope rules in SECURITY.md. Check that document for which components and dependencies are covered before reporting.

### Will I be credited for a valid report?

Credit and any rewards are described in SECURITY.md and the bounty terms. Refer to those documents for the current policy.

## Contributing Code

### How do I run the tests?

Follow the test instructions in [CONTRIBUTING.md](../CONTRIBUTING.md) and the README. Run the full suite before opening a pull request.

### Where should I log my changes?

Add an entry to `PROGRESS.md` describing what you changed and why, as required by the acceptance criteria.

### Do I need to sign my commits?

Follow the commit and sign-off rules in CONTRIBUTING.md. If it does not mention signing, follow the conventions of recently merged pull requests.
