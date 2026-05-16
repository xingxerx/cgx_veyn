# Contributing to VEYN

Thank you for your interest in contributing. VEYN is a local-first daemon —
correctness, security, and minimal footprint matter more than feature count.

---

## Getting Started

1. **Read the architecture docs** — [`CANON.md`](CANON.md) and [`ROADMAP.md`](ROADMAP.md) before starting.
2. **Check open issues** — look for `good first issue` or `help wanted` labels.
3. **Fork → branch → PR** — branch from `main`, keep PRs focused.

---

## Development Setup

```bash
git clone https://github.com/<you>/cgx-veyn
cd cgx-veyn
# See INSTALL.md for system prerequisites
cargo build
VEYN_MOCK=true VEYN_NO_AUTH=true cargo run -p veyn-core
```

---

## Code Style

- **Rust edition 2021**, `cargo fmt` before committing (enforced by CI).
- `cargo clippy -- -D warnings` must pass.
- No `unwrap()` in production paths — use `?` or `anyhow::Context`.
- No unnecessary `clone()` — prefer `Arc` for shared state.
- Comments only for non-obvious WHY, not WHAT. Keep them short.
- No half-finished stubs — either implement or leave a `// TODO(issue#123):` with a linked issue.

---

## Commit Messages

Follow the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
feat(compression): add temporal debounce per device class
fix(auth): reject empty Bearer tokens
docs(install): add Windows prerequisites
refactor(dispatcher): extract compression engine into own module
```

Scope is the crate or subsystem affected: `core`, `adapters`, `plugins`,
`schemas`, `sdk`, `auth`, `compression`, `api`, `docs`.

---

## Pull Request Checklist

- [ ] `cargo fmt` applied
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test` passes (where applicable)
- [ ] `CHANGELOG.md` updated under `[Unreleased]`
- [ ] No new `unwrap()` in non-test code
- [ ] Security-sensitive changes reviewed against OWASP Top 10

---

## Security Issues

**Do not open a public issue for security vulnerabilities.**
Email the maintainers directly (address in `Cargo.toml` or repository settings)
or use GitHub's private vulnerability reporting.

---

## Adding a New Adapter

1. Add a new file in `veyn-adapters/src/<name>.rs`.
2. Implement the `VeynAdapter` trait.
3. Expose it from `veyn-adapters/src/lib.rs`.
4. Wire it up in `veyn-core/src/main.rs` behind a config/env flag.
5. Add the enable flag to `veyn.toml.example` and `.env.example`.
6. Document prerequisites in `INSTALL.md`.

---

## Adding a WASM Plugin

See the `sdk/` crate and the `plugins/garmin-connect/` example. Plugins must:
- Implement `veyn_init`, `veyn_poll`, `veyn_alloc`, `veyn_free` exports.
- Use the `veyn_register_plugin!(MyPlugin)` macro from `veyn-plugin-sdk`.
- Ship a `plugin.toml` manifest alongside the `.wasm` binary.

---

## License

By contributing, you agree that your contributions will be licensed under the
same Elastic License 2.0 terms as the project. See [`LICENSE`](LICENSE).
