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
- **No semantic versioning between bricks.** Internal bricks use `path = "..."` dependencies.
  Semver applies only at the project boundary when consuming external crates. Components are
  never individually published to crates.io.
- **Components depend on interfaces, not on each other directly.** Only bases wire components
  together.
- **Fast feedback.** The development project builds all bricks; `cargo polylith check` requires
  no compilation.

---

## Semantic versioning stance

Polylith explicitly rejects per-component versioning
([reference](https://polylith.gitbook.io/polylith/architecture/2.4-libraries)).
Components at HEAD are coherent with each other by definition; independent versions would
create the very coupling the architecture is designed to avoid.

cargo-polylith adopts the same position:

- Internal bricks always declare `path = "..."` dependencies — never a crates.io version.
- Components are never individually published to crates.io.
- Semver applies only when the workspace consumes external crates.
- The workspace as a whole carries a version (for releasing the tool or a library surface).

This is a deliberate alignment with Polylith, not a Cargo limitation. Cargo supports path
dependencies natively; we choose them for bricks.

---

## Rust-forced adaptations

Each deviation from the Clojure reference implementation is listed here with its justification.

| Concept | Clojure polylith | cargo-polylith | Why |
|---|---|---|---|
| **Interface declaration** | Namespace structure — same namespace = same interface | `[package.metadata.polylith] interface = "..."` in Cargo.toml | Rust has no namespace-based interface; explicit metadata is unambiguous and prevents typos from creating phantom interfaces |
| **Interface enforcement** | `poly check` verifies matching public APIs across implementations | Rust compiler enforces full type compatibility when you swap via `[patch]`; `cargo polylith check` is a structural pre-flight | The compiler is more expressive and authoritative than namespace matching — we defer to it for the definitive check |
| **Implementation switching** | Named profiles in `deps.edn` select which source directories are compiled | `[patch.crates-io]` in a project workspace Cargo.toml | Cargo `[patch]` is the closest analog — compile-time substitution of one crate for another; more explicit, same intent |
| **Development project** | Dedicated `development/` project at workspace root | The root workspace itself | Cargo's workspace model is already the right structure; a wrapping project would add ceremony without benefit |
| **Stub-first development** | Default profile uses the primary implementation | Root workspace uses lightweight/stub components; production projects patch in real implementations | Enables fast tests without heavy dependencies; maps naturally to Cargo's path dependency model |

---

## What we do not compromise on

These are the architectural rules cargo-polylith enforces or warns about. They are not
negotiable adaptations — they are the Polylith model itself:

- The four building blocks and their relationships (component, base, project, development project)
- No direct component-on-component dependencies — components are wired together only by bases
- The development project (root workspace) contains all bricks
- A base must expose a library API (`src/lib.rs`) and must not have `src/main.rs`
- `cargo polylith check` is a fast, compilation-free structural pre-flight

---

## Audience note

cargo-polylith is for Rust developers who want architecture discipline in large workspaces.
It does not assume familiarity with Clojure or the original Polylith tool. The documentation
references Cargo concepts first and Polylith concepts as the inspiration, not the prerequisite.

Developers who do know Polylith will find the building blocks immediately recognisable. The
adaptations table above is intended to be readable by someone who knows Polylith deeply —
each row explains a choice, not just states it.
