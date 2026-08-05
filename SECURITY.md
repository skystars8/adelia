# Security policy

Adelia is preview software for self-hosted communities. Operators are responsible for deployment security, updates, backups, infrastructure logs, and local law/policy requirements.

## Supported versions

Security fixes target the latest tagged release and the current default branch. Older commits may not receive backports.

## Report a vulnerability privately

Do not open a public issue containing exploit details, credentials, private posts/uploads, session values, or database contents.

Use the repository's **Security → Advisories → Report a vulnerability** feature. Repository owners should enable GitHub private vulnerability reporting before announcing a public release.

If private reporting is not enabled, open a public issue titled `Security contact requested` without technical details or secrets, and ask the owner to provide a private channel.

Include:

- affected version or commit;
- affected component and deployment style;
- clear reproduction steps or a minimal proof of concept;
- realistic impact;
- required preconditions;
- suggested remediation when known.

## Responsible testing

- Test against an installation you own or have explicit permission to assess.
- Do not access other people's data.
- Do not disrupt a public community.
- Stop after demonstrating the minimum evidence required.
- Give the owner a reasonable opportunity to investigate before disclosure.

No response-time or reward guarantee is offered.

## Deployment reports

Many reports are configuration problems rather than application vulnerabilities. Include sanitized Nginx, systemd, Docker, and Adelia settings when relevant, but remove:

- `DATABASE_URL` passwords;
- `APP_SECRET`;
- moderator passwords and cookies;
- private uploads and posts;
- backup contents;
- domain-provider, VPS, SSH, or API credentials.

## Privacy scope

Adelia does not store IP addresses or IP-derived identifiers. Infrastructure outside Adelia—including hosting providers, CDNs, firewalls, Nginx changes, monitoring agents, and backup systems—can still record network metadata. Report infrastructure privacy concerns to the responsible operator.
