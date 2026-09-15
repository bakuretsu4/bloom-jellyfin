# Security policy

## Reporting a vulnerability

Please **don't open a public issue** for a security problem. Report it privately instead, through
GitHub: open the repository's **Security** tab and choose **Report a vulnerability**.

Include what you found, how to reproduce it, and what it could let someone do. You'll get a reply
as soon as possible; this is a personal project, so it may take a few days. Once it's fixed, you'll
be credited in the release notes unless you'd rather not be.

## Supported versions

Only the latest release gets security fixes.

## What's in scope

Things like:

- Bloom leaking your Jellyfin sign-in token, server address or other details anywhere they shouldn't
  go (other hosts, Discord, logs, notifications).
- A malicious or compromised server, or anything on your network, getting Bloom to run code, read or
  write files outside its own folders, or make requests it shouldn't.
- The sign-in token file being readable by other users on the computer.

Not in scope: vulnerabilities in Jellyfin itself (report those to the
[Jellyfin project](https://github.com/jellyfin/jellyfin/security)), or in libraries Bloom bundles,
unless Bloom uses them unsafely.

## How Bloom handles your credentials

- Your password is sent only to your server when you sign in, and never stored.
- The token your server returns is kept in `~/.local/share/dev.bloom.app/accounts.json`, readable
  only by your user (mode 600, in a folder with mode 700). With "Stay signed in" off, it stays in
  memory only.
- The token is sent only to the server it came from. It never reaches the page's code, Discord,
  notifications or any other host.
