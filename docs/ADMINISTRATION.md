# Administration

The moderator area is available at `/mod`. Public pages intentionally contain only a compact `Mod` link.

## Development login

When `DEVELOPMENT_MODE=true`, Adelia maintains this loopback-only account:

- username: `admin`
- password: `mod`

The application refuses development mode unless both the bind address and public URL are local. Never attempt to use this convenience account on an internet-facing site.

## Production administrator

Set `DEVELOPMENT_MODE=false`, choose a unique password of at least 12 characters, and run:

```sh
export ADELIA_ADMIN_PASSWORD='a-long-unique-password'
./adelia admin admin
unset ADELIA_ADMIN_PASSWORD
```

The password is Argon2-hashed before storage. The temporary environment variable should not be placed in `.env`, shell history, service files, or documentation.

Adelia's schema supports `admin` and `moderator` roles. Version 0.1 provides the administrator creation command but does not yet include a public account-management screen.

## Moderator navigation

- **View site** opens the public landing page.
- **Reports** lists open visitor reports.
- **Pending** lists posts waiting for approval.
- **Threads** shows recent approved posts and threads.
- **Boards** lists boards and their controls.
- **News** publishes site news.

Dashboard panels link to the same dedicated pages; clicking a navigation item does not keep unrelated dashboard panels open.

## Board settings

Each board has independent settings:

### Lock all posting

Blocks new threads and replies. Existing pages remain readable.

### Require approval

New submissions are saved as pending and do not appear on public board, thread, catalog, or archive pages until approved. The submitter receives a confirmation rather than a public post URL.

Approval regenerates the affected thread and board. Reject an unwanted pending post by deleting it.

### Require shared posting password

Reading stays public, but every new thread or reply must include the board's shared password.

- Passwords must contain 12 to 128 characters.
- Adelia stores an Argon2 hash, not the submitted password.
- Leave the password field blank while the option remains enabled to keep the existing password.
- Enter a value to replace it.
- Clear the checkbox to remove the requirement.

This is useful for a club, class, workshop, or small invited posting group. It is not individual user authentication: everyone with the shared value has the same posting access.

## Creating boards

Only administrators can create boards through the interface. Board URIs:

- contain 1 to 32 lowercase letters, numbers, underscores, or hyphens;
- begin with a letter or number;
- become the public path, such as `/projects/`.

Choose stable URIs because version 0.1 does not provide board rename or deletion controls.

## Moderating posts

Open a board from `Mod → Boards` to see its posts. Each post has compact actions:

- **Edit** changes name, subject, and body.
- **Edit image** can keep, remove, or replace the current upload.
- **Delete** removes a reply or an entire thread.
- **Lock/Unlock** controls replies to a thread.
- **Sticky/Unsticky** keeps a thread above ordinary bump ordering.
- **Approve** publishes a pending submission.

Replacing an image runs the same MIME, byte-size, dimension, and decode-safety validation as a public upload. Deleting a thread removes its replies through the database relationship and removes associated media from disk.

Moderator writes use CSRF protection and regenerate relevant static pages before returning success.

## Reports

Visitors select one or more posts, enter a reason, and submit a report. Adelia stores no reporter IP address or derived identifier. Only one open report per post is retained, preventing repeated queue entries for the same unresolved post.

Dismissing a report preserves the post and closes that queue entry. Deleting the reported post also removes reports through the database relationship.

## Secure tripcodes

A poster can establish a repeatable public identity by entering:

```text
Display Name##a-private-secret-with-12-or-more-characters
```

The private portion is used with `APP_SECRET` to derive a 16-character public tripcode. It is never stored. A stable `APP_SECRET` is therefore important: rotating it changes every future secure tripcode.

Tripcodes establish continuity of a secret, not a verified real-world identity. Community members must decide what trust to place in one.

## Banners

Put one or more 300×100 PNG, JPEG, GIF, or WebP files in `web/assets/banners/` and restart Adelia. Each board-scoped page requests `/banner/<board>`, which randomly redirects to one of the indexed static files.

The redirect is not cached; the selected image can be cached normally.

## Rebuilding public pages

Adelia rebuilds affected pages after successful writes. Rebuild everything after changing templates, banners, starter display data, or site-wide presentation:

```sh
adelia rebuild
```

When running from source:

```sh
cargo run -- rebuild
```

## Backups

A complete backup consists of:

1. the PostgreSQL database;
2. `data/uploads/`;
3. the deployment `.env` stored securely.

`generated/` can be recreated with `adelia rebuild`, but backing it up can shorten recovery. Test restoration on another machine before relying on any backup procedure.
