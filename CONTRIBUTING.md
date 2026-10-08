# Contributing

Thanks for your interest in ISK Works. It is a one-maintainer project, so please read this first.

## Before you start

- **Bugs and ideas**: open an issue. For bugs, include what you did, what you expected, and what
  happened. If it involves numbers, include the item, the runs and your facility setup.
- **Pull requests**: open an issue first for anything bigger than a small fix, so we can agree on the
  approach before you spend time on it. Unsolicited large PRs may be closed.
- **Security issues**: never in a public issue. See [SECURITY.md](SECURITY.md).

## Development setup

See [Running locally](README.md#running-locally). In short: `cp .env.example .env`, `make db-up`,
import an SDE, then `make api` and `make web`.

Before opening a PR, run:

```bash
make verify
```

If you touched storage code, also run the Postgres-backed tests with the database up:

```bash
cargo test --workspace -- --ignored
```

CI runs all of the above on GitHub-hosted runners. Workflows from first-time contributors wait for
maintainer approval before they run.

## Conventions

- **Tests first.** New behaviour comes with a test that failed before the change. Rust API tests in
  `apps/iskworks-api/tests/` use in-memory fakes of the repository traits; tests that need a real
  database use `#[sqlx::test]` and are marked
  `#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]`. Web tests use Vitest and
  Testing Library in `__tests__/` folders next to the code.
- **Layering.** `iskworks-core` holds pure domain logic with no I/O, `iskworks-storage` the Postgres
  repositories, and `iskworks-app` the services that combine them. Route handlers stay thin.
- **Migrations are append-only.** Never edit or delete a migration that has been merged: sqlx
  checksums every applied file, and changing one breaks existing databases. Add a new migration instead.
- **Web UI.** Use the shared `OperationalTable` components for dense tabular views. Show money compactly
  (`formatIskCompact`) with the exact value in the `title` attribute.
- **Commits.** Conventional style (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `ci:`, optionally
  scoped like `fix(web):`), one logical change per commit.
- **Comments** explain why, not what, and must make sense without any internal context.

## AI-assisted contributions

Welcome, since the project itself is built that way (see [Built with AI](README.md#built-with-ai)).
You are responsible for every line you submit: read it, run it, and make sure the tests prove
it works. Mention in the PR description that an agent helped.

## License of contributions

By contributing, you agree that your contributions are licensed under the project's license,
the [GNU AGPL v3.0 or later](LICENSE).
