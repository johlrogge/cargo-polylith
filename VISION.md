# Vision — cargo-polylith

## What cargo-polylith is

cargo-polylith is a port of the [Polylith](https://polylith.gitbook.io/polylith/) architecture
to Rust/Cargo workspaces. The four building blocks — components, bases, projects, and the
development project — and their relationships are unchanged. The values are unchanged.
The adaptations are forced by Rust's semantics, not by preference.

---

## Guiding principle: humble deviations

When our model diverges from Polylith's, we follow a discipline:

1. First ask: do we understand *why* Polylith made this choice?
2. Link to the relevant Polylith documentation or source.
3. Explain what Rust's semantics force us to do differently.
4. Do not claim our deviation is an improvement unless the Rust compiler itself provides a
   stronger guarantee than the deviation costs.

> "When we deviate from the polylith model the assumption is that we need to improve our
> understanding of polylith rather than that polylith needs to be improved."

Rust is not Clojure. Our target audience is Rust developers who value the things Rust gives
them — static types, exhaustive pattern matching, fearless concurrency. We respect those
values, but we do not use them as an excuse to abandon Polylith's architectural discipline.

---

## Core values (unchanged from Polylith)

- **One workspace, many deployable artifacts.** All bricks live together; the development
  project builds everything in one pass.
- **Bricks are the unit of development and reuse, not libraries.** A component is not a
  published crate — it is a named, swappable piece of functionality.
- **No version coupling between bricks.** Internal bricks use `path = "..."` dependencies —
  no brick ever references another brick's version number. Brick version fields exist because
  Cargo requires them; they are internal bookkeeping, not compatibility contracts.
- **Bricks depend on interfaces.** Components and bases may depend on other components, but
  they do so by referencing the interface (crate name), never a specific implementation.
  Projects select which implementation is active via path dependencies aliased to the interface name.
- **Fast feedback.** The development project builds all bricks; `cargo polylith check` requires
  no compilation.

---

## Versioning stance

Polylith explicitly rejects per-component versioning
([reference](https://polylith.gitbook.io/polylith/architecture/2.4-libraries)).
Components at HEAD are coherent with each other by definition; independent versions would
create the very coupling the architecture is designed to avoid.

cargo-polylith respects this principle while acknowledging a Rust/Cargo reality: every
`Cargo.toml` *must* declare a version. Rather than fighting Cargo's requirement, we find
value in both models:

- Internal bricks always declare `path = "..."` dependencies — never a crates.io version.
- Components are never individually published to crates.io.
- The workspace as a whole carries a **distro version** — the release of the bundle,
  analogous to a Linux distribution release.

Two versioning modes let teams choose the level of discipline that fits their needs:

- **Relaxed** (default): All bricks share the workspace version via `version.workspace = true`.
  One version, one number, zero friction. This is the closest alignment with Polylith's
  original stance — brick versions exist only because Cargo demands them.
- **Strict**: Each brick owns its version as a **change-tracking signal** — not as a
  published API contract or inter-brick compatibility promise. Brick versions record
  the *kind* of change (patch, minor, major) during development. At release time,
  `cargo polylith bump` walks the dependency graph and computes a per-project semver
  recommendation from these accumulated signals.

The strict mode is a humble deviation (see guiding principle above): Cargo forces a version
field to exist, and Rust's `pub` surface gives us a precise definition of "interface change"
via AST comparison. We use these Rust-native affordances to turn a mandatory field into
useful information, without introducing inter-brick coupling or publishing.

See [ADR-001](docs/adr/001-versioning-model.md) for the full design.

---

## Rust-forced adaptations

Each deviation from the Clojure reference implementation is listed here with its justification.

| Concept | Clojure polylith | cargo-polylith | Why |
|---|---|---|---|
| **Interface declaration** | Namespace structure — same namespace = same interface | `[package.metadata.polylith] interface = "..."` in Cargo.toml | Rust has no namespace-based interface; explicit metadata is unambiguous and prevents typos from creating phantom interfaces |
| **Interface enforcement** | `poly check` verifies matching public APIs across implementations | Rust compiler enforces full type compatibility when you swap implementations; `cargo polylith check` is a structural pre-flight | The compiler is more expressive and authoritative than namespace matching — we defer to it for the definitive check |
| **Implementation switching** | Named profiles in `deps.edn` select which source directories are compiled | Path dependency aliased to the interface name in a project's `[dependencies]`; `package = "..."` when the crate name differs from the alias | Direct path deps need no registry indirection — the selection is explicit and local to each project |
| **Profile / workspace scope** | In Clojure polylith, named profiles select which source directories are compiled; the `development` profile compiles all bricks | The root `Cargo.toml` `[workspace] members` list controls compile scope; each project's `[dependencies]` selects which component implementation is active via path-dep aliasing | Cargo's workspace model natively encodes compile scope via `members`; implementation selection is local to each project — no registry indirection needed |
| **Development project** | Dedicated `development/` project at workspace root | The root workspace itself | Cargo's workspace model is already the right structure; a wrapping project would add ceremony without benefit |
| **Stub-first development** | Default profile uses the primary implementation | Root workspace includes all implementations; each project's `[dependencies]` selects which is active | Enables fast tests without heavy dependencies; the choice is explicit per project |

---

## What we do not compromise on

These are the architectural rules cargo-polylith enforces or warns about. They are not
negotiable adaptations — they are the Polylith model itself:

- The four building blocks and their relationships (component, base, project, development project)
- Bricks depend on interfaces (crate names), never on specific implementations
- The development project (root workspace) contains all bricks
- A base must expose a library API (`src/lib.rs`) and must not have `src/main.rs`
- `cargo polylith check` is a fast, compilation-free structural pre-flight

---

## Polylith as an alternative to traits and generics

Rust developers instinctively reach for traits when they need swappable behaviour:

```rust
trait Storage { fn save(&self, item: Item); }
struct PostgresStorage;
struct InMemoryStorage;
impl Storage for PostgresStorage { ... }
impl Storage for InMemoryStorage { ... }
```

This is the right tool when multiple implementations must coexist in the same running process
(e.g. routing requests to different backends simultaneously). But much of what looks like
runtime variation in application code is actually **build-time variation**:

- In-memory storage for tests, Postgres for production
- Stub HTTP client in dev, real client in prod
- Simplified audio backend for CI, PipeWire for deployment

For build-time variation, traits pull in machinery that isn't needed:

| Pattern | Abstraction | Dispatch | Heap | Signature complexity |
|---|---|---|---|---|
| `dyn Trait` | yes | vtable (runtime) | `Box<dyn>` per value | leaks into callers |
| generics `T: Trait` | yes | monomorphized (compile) | no | bounds infect every caller |
| Polylith component swap | yes | direct call (compile) | no | none — callers see plain functions |

With Polylith, a component exposes plain public functions. A consumer calls them directly.
At build time, a project declares `interface-name = { path = "...", package = "..." }` to
select which component implementation is compiled in. The compiler enforces compatibility.
No vtable.
No `Box`. No generic bounds propagating through every function that touches the abstraction.

The trade-off is explicit: **one implementation per binary**. That is rarely a constraint
for application code. It is the wrong tool when you genuinely need runtime polymorphism.

This insight is particularly relevant for Rust: a common source of `dyn Trait`, `Arc<dyn>`,
and generic type parameter sprawl in application codebases is build-time variation that
Polylith can handle more simply and at zero runtime cost.

---

## Audience note

cargo-polylith is for Rust developers who want architecture discipline in large workspaces.
It does not assume familiarity with Clojure or the original Polylith tool. The documentation
references Cargo concepts first and Polylith concepts as the inspiration, not the prerequisite.

Developers who do know Polylith will find the building blocks immediately recognisable. The
adaptations table above is intended to be readable by someone who knows Polylith deeply —
each row explains a choice, not just states it.
