# ADR-007: Humble Deviations Discipline

## Status
Accepted

## Context
cargo-polylith adapts the Clojure polylith model to Rust. Rust's type system, ownership model, and build system differ enough that some concepts don't translate directly. There is a risk of unjustified "improvements" that diverge from Polylith's intent, making the tool incompatible with the broader Polylith ecosystem's mental model and documentation.

## Decision
When the tool's model deviates from Clojure polylith, the deviation must be explicitly documented with a three-step justification: (1) explain why Polylith made its original choice; (2) state what Rust's semantics force us to do differently; (3) only claim an improvement if the compiler provides stronger guarantees than the original. The default assumption is "we need to better understand polylith" rather than "polylith needs improvement."

## Consequences
Deviations are traceable and defensible. Contributors cannot drift the model without documentation. The discipline creates a record useful for future alignment with the original Polylith authors. The burden of proof is on the deviator, which reduces speculative divergence from the upstream model.
