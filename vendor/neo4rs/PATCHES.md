# db-tool patch to neo4rs 0.9.0-rc.10

## Source and license

- Full published crates.io archive: `neo4rs-0.9.0-rc.10.crate`.
- SHA-256: `b433ba6b38f863f3f1db1cd2b99da742c5db961d7fec9553663331b5db3bc453`.
- Upstream commit: `19f244ae7800ac084f0679e89c2606b799be7538`, subdirectory `lib` (preserved `.cargo_vcs_info.json`).
- Preserve every archive file, including integrationtests, examples, original manifests and Cargo.lock; documentation macros include integrationtests during normal compilation.
- Cargo.toml declares MIT. Original README also references MIT and Apache-2.0 choices; this copy uses MIT.
- The published archive and the inspected upstream tree contain no standalone LICENSE-MIT or LICENSE-APACHE. Added LICENSE-MIT records standard MIT terms with attribution derived from upstream Cargo.toml authors and contributors; it is an added notice, not an unmodified upstream file. Original Cargo.toml and README license declarations remain unchanged.

## Only upstream source patch

`src/stream.rs`, in `impl DetachedRowStream`:

```rust
/// Column names in Bolt RUN SUCCESS order, including results with no rows.
pub fn fields(&self) -> &BoltList {
    &self.stream.fields
}
```

The caller reads BoltType::String values from this immutable list before fetching records. The driver integration test checks RETURN aliases [z,a,m] and an empty result with headers [x,y].

## Alternatives checked (2026-10-15)

- Pinned upstream query.rs reads RUN SUCCESS `fields` and passes that BoltList directly into RowStream::new, but both stream types keep it private.
- Row::keys() uses BoltMap/HashMap and cannot preserve RETURN order; it is unavailable for an empty result.
- Current upstream `main` at `3295dad484ad79644d4cdee9f82bba64d6d6dd53` still declares 0.9.0-rc.10 and has no public stream-header getter (`lib/src/stream.rs` inspected directly).
- Parsing/rewriting RETURN text or issuing a second query cannot provide the general RUN metadata contract and would add a parser or an extra execution. A separate Bolt client would duplicate existing protocol, auth and value handling.

Keep the pinned dependency plus this getter until an upstream release exposes stream headers. Then remove the local copy after the same real-Neo4j order/empty-result regressions pass.
