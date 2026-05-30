# Contributing to FluxaPay

Thank you for your interest in contributing to FluxaPay! This document provides guidelines and information for contributors.

## Changelog Requirements

All pull requests must include an entry in `CHANGELOG.md` that documents the changes made, unless explicitly exempted with the `skip-changelog` label.

### Changelog Format

We follow the [Keep a Changelog](https://keepachangelog.com/) standard for documenting changes. The changelog is organized with an `Unreleased` section at the top, divided into the following categories:

- **Added** — for new features
- **Changed** — for changes to existing functionality
- **Fixed** — for bug fixes
- **Removed** — for removed features or functionality

#### Example Entry

```markdown
## Unreleased

### Added
- Reentrancy guard for `process_refund_internal` and `settle_payment` functions

### Changed
- Updated `PaymentStream` struct to include minimum rate floor validation

### Fixed
- Fixed race condition in token transfer logic
```

### When to Use `skip-changelog`

The `skip-changelog` label should be applied to pull requests that do not require changelog documentation, such as:

- Internal refactoring without user-facing changes
- Documentation updates
- CI/CD improvements
- Test-only changes
- Minor typo fixes

### How to Apply the Label

When creating or updating a pull request:

1. If your changes do not affect user-facing functionality, add the `skip-changelog` label to the PR
2. The changelog-check CI job will pass if either:
   - `CHANGELOG.md` was modified in your PR, **or**
   - The PR has the `skip-changelog` label

If your PR fails the changelog check and you believe it should be exempt, add the label and re-run the CI.

## Submitting Changes

When submitting a pull request:

1. Ensure all tests pass
2. Update `CHANGELOG.md` with your changes (unless using `skip-changelog`)
3. Follow existing code style and conventions
4. Provide a clear description of your changes
