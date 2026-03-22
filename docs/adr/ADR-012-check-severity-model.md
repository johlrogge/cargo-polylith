# ADR-012: Hard Violations vs. Warnings in `check`

## Status
Accepted

## Context
`cargo polylith check` detects many types of workspace issues, ranging from structural problems that would prevent compilation to style violations that represent best-practice guidance. Treating all issues as hard errors makes CI too rigid; treating all issues as warnings makes the tool too passive to enforce structural integrity.

## Decision
Violations are divided into two severity levels. Hard violations exit with code 1 and block CI: these are issues that indicate real structural breakage (e.g. `missing-lib`, `missing-impl`, `dep-key-mismatch`). Warnings exit with code 0 and guide without blocking: these cover best-practice issues (e.g. `orphan-component`, `no-base`, `missing-interface`, `base-has-main`).

## Consequences
CI pipelines can gate on hard violations without being blocked by in-progress or intentional structural choices. Users get actionable guidance from warnings without forced remediation. The severity of any given check can be revisited — `missing-interface` was initially a hard violation and was downgraded to a warning. Adding new checks defaults to warning until field experience justifies promotion to hard violation.
