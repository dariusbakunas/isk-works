# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub:
**[Report a vulnerability](../../security/advisories/new)** (Security tab → "Report a vulnerability").
Do not open a public issue.

Include what you found, how to reproduce it, and what an attacker could do with it. You should get
a reply within a week. Please give a reasonable amount of time to release a fix before you disclose
the issue publicly.

## Scope

Areas where a report is especially valuable:

- Authentication and sessions: EVE SSO login and character linking, session cookies, invite codes.
- Workspace isolation: one account reading or changing another account's data.
- Token handling: encryption of stored ESI refresh tokens, token leakage in logs or responses.
- The web container's runtime configuration and anything that could inject script into the app.

Out of scope: issues that need an already-compromised server or database, denial of service against
a self-hosted instance, and findings in third-party dependencies that are not exploitable here
(report those upstream).

## Supported versions

Only the latest release receives security fixes. Self-hosters should run a recent image tag.
