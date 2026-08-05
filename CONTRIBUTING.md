# Contributing to Adelia

Bug reports, documentation corrections, focused fixes, and carefully scoped improvements are welcome.

## AI-assisted work is welcome

Adelia itself was created with ChatGPT under human direction. Contributors may use ChatGPT, Codex, or other AI tools.

AI use does not remove contributor responsibility. Before submitting:

- understand the behavior being changed;
- inspect generated code and dependencies;
- run the quality checks;
- test the affected user flow;
- describe limitations and uncertainty honestly;
- remove secrets, private data, and unrelated generated files;
- disclose substantial AI assistance in the pull request.

“The AI said it works” is not a test result. Reproduction evidence and executable checks are more useful.

## Before changing code

1. Search existing issues.
2. Reproduce the behavior on the latest default branch.
3. Open an issue before a large feature, schema redesign, dependency replacement, or user-interface rewrite.
4. Keep the change focused enough to review.

Security vulnerabilities belong in private reporting; see [SECURITY.md](SECURITY.md).

## Development setup

- [Windows](docs/WINDOWS_DEVELOPMENT.md)
- [Linux](docs/LINUX_DEVELOPMENT.md)
- [Docker](docs/DOCKER.md)

## Required checks

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

When changing templates or JavaScript, also test:

- desktop and narrow mobile layouts;
- landing-page frame show/hide behavior;
- board, thread, catalog, archive, and news navigation;
- posting and reporting;
- moderator login and the affected moderation action;
- at least the default theme and one dark alternative.

When changing uploads, test each supported format and both valid and rejected size/dimension cases.

## Design invariants

Changes should preserve these properties unless a proposal explicitly explains why not:

- ordinary public reads remain static-file friendly;
- database access and regeneration stay bounded;
- PostgreSQL writes remain transactional;
- visitor IP addresses and derived identifiers are not stored;
- development credentials cannot be enabled on a public bind;
- uploads are validated from content rather than filename;
- moderator writes retain authentication, same-origin, and CSRF protections;
- public JavaScript remains dependency-light and does not require jQuery;
- mobile and desktop flows remain usable.

## Database migrations

After the first public release, never edit a migration that users may already have applied. Add a new numbered migration instead. Migrations must be safe to run once, preserve existing data unless a clearly documented destructive change is unavoidable, and be tested on both a new database and an upgraded database.

## Dependencies

Prefer the Rust standard library and existing dependencies. Explain why a new dependency is necessary, check its maintenance and license, and commit the resulting `Cargo.lock` change.

Do not add client-side frameworks for behavior that can remain small vanilla JavaScript.

## Documentation and attribution

Update documentation with behavior changes. Preserve `LICENSE`, `THIRD_PARTY_NOTICES.md`, and all files under `licenses/`. New images, themes, or copied code require a compatible license and accurate attribution.

## Pull requests

A useful pull request includes:

- the problem and intended result;
- exact behavior before and after;
- tests and manual verification performed;
- screenshots for visible changes;
- migration and rollback considerations;
- security/privacy considerations;
- AI tools used for substantial generation or analysis.

Avoid mixing formatting sweeps or unrelated cleanup with a functional fix.
