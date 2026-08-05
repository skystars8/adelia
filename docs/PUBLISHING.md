# Publishing Adelia on GitHub

This folder is designed to become a new repository. It contains no Git history or remote, so the owner can review the exact first commit.

## Before the first commit

Confirm you are inside the generic Adelia folder, not a private site checkout:

```sh
pwd
```

Review:

- `README.md` and every file under `docs/`;
- `LICENSE` and `THIRD_PARTY_NOTICES.md`;
- generic starter boards in `migrations/0001_initial.sql`;
- `.env.example`;
- the single generic banner;
- Docker and VPS examples.

Confirm these are absent:

- `.env`;
- database dumps;
- real uploads;
- generated HTML;
- `target/`;
- private domains, usernames, email addresses, filesystem paths, or credentials;
- site-specific banners and text.

## Initialize the repository

```sh
git init -b main
git add .
git status --short
git diff --cached --check
git commit -m "Initial Adelia preview release"
```

Inspect the staged list before committing. The runtime directories should contribute only their `.gitkeep` files.

Useful checks:

```sh
git ls-files
git ls-files | grep -E '(^|/)\.env$|^target/|^generated/.+|^data/uploads/.+'
```

The second command should show only `generated/.gitkeep` and `data/uploads/.gitkeep` from the ignored runtime directories, and no `.env`.

## Create the GitHub repository

Create an empty public repository in GitHub without asking GitHub to generate another README, license, or `.gitignore`. Then:

```sh
git remote add origin https://github.com/YOUR-ACCOUNT/adelia.git
git push -u origin main
```

Use SSH instead of HTTPS if that is how the account normally authenticates.

## Repository settings

Recommended initial settings:

1. Enable Issues.
2. Enable private vulnerability reporting under Security settings.
3. Enable the dependency graph and Dependabot alerts.
4. Require the CI workflow before merging into `main`.
5. Prevent force-pushes to `main`.
6. Add a short description and relevant Rust/PostgreSQL/self-hosting topics.
7. Do not add production secrets to Actions merely to run the supplied CI; its tests do not require a database.

The repository already contains structured bug/feature forms, a security policy, Dependabot configuration, and CI.

## First release

Use a preview label until the software has been exercised on a real non-development deployment:

```text
v0.1.0 - Preview
```

Attach release notes based on `CHANGELOG.md`. State tested operating systems and deployment modes precisely. Do not claim that Docker or a platform was tested when it was only syntax-checked.

## Issue handling

Ask reporters for reproduction evidence. Never ask them to post secrets or private community data. Reproduce against a clean database when possible, turn confirmed regressions into tests, and use AI assistance as an investigation tool rather than assuming every report is correct.

Security reports belong in private advisories.

## Future site-specific work

Keep private deployments in separate folders or private repositories. Pull reviewed Adelia changes into a private site; do not push the site's `.env`, uploads, backups, custom private configuration, or moderation data back to the public repository.
