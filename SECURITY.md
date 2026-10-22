# Security Policy

## Supported Versions

Muspice has not been released yet: there are no tags, GitHub releases, or distributed builds.
Until the first release, only the latest commit on the `main` branch is supported, and fixes land there.

| Version       | Supported          |
| ------------- | ------------------ |
| Latest `main` | :white_check_mark: |
| Older commits | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability in Muspice, please report it responsibly.

### How to Report

1. **Do NOT create a public GitHub issue** for security vulnerabilities
2. Use [GitHub's private vulnerability reporting](https://github.com/ban367/muspice/security/advisories/new)

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Response Timeline

- Initial response: within 72 hours
- Status update: within 1 week
- Fix timeline: depends on severity

### Scope

This policy applies to the latest version of Muspice on the `main` branch.

## Security Considerations

Muspice is a desktop application that:

- Only accesses local files selected by the user
- Does not transmit data to external servers
- Stores its library database, settings and automatic backups in the platform-specific application data directory, and its logs in the platform-specific log directory (manual backups and exported playlists are saved where the user chooses)
