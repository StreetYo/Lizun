# ADR 0005: Installing widgets from GitHub, a zip archive or a folder

Status: accepted (2026-10-08). How to install and publish is described in
[widgets.md](../widgets.md#installing).

## Context

Installing a widget meant copying its folder into `%APPDATA%\app.lizun.desktop\widgets\` and
pressing Reload. Nobody finds that folder by themselves, and a widget with a broken manifest was
only skipped with a line in the log. Widgets need a way to spread without a store of our own:
authors already keep their code on GitHub, and users already know how to share a link.

A widget runs in a sandbox and gets only the permissions and network origins its manifest lists
(ADR 0001), so the manifest is what the user has to see before agreeing to install.

## Decision

- **Three sources, one path.** Settings > Widgets takes a GitHub link, a `.zip` archive or a
  folder (picked in a dialog or dropped on the window). A GitHub link is turned into a zip
  download from github.com, so all three end up as files in a staging folder and go through the
  same checks.
- **GitHub links as people copy them.** A repository, `/tree/<ref>[/<folder>]`, a manifest's
  `/blob/` link, a release page and a zip attached to a release. No GitHub API calls, so there is
  no rate limit and no token. Only public repositories.
- **Check before anything is installed.** The widget is unpacked into
  `%APPDATA%\app.lizun.desktop\widget-staging\` with limits on size and file count, zip entries
  that would land outside the folder and symbolic links are skipped, and the manifest is
  validated with the same rules as at startup, plus the entry page must exist. An id taken by a
  widget that ships with Lizun is refused.
- **The user agrees after seeing the permissions.** A widget that passes is shown with its
  author, origin, permissions and network access; it moves into the widgets folder only when the
  user presses Install. This is the one moment the user can judge a widget before it runs.
- **One folder per id.** Installing puts the widget in `widgets/<id>/`; an installed widget with
  the same id is replaced (shown as an update with the old version), and a failed move puts the
  old copy back. Settings and stored data are keyed by id and are kept on update and removal.
- **Removal from Settings** for installed widgets only. Widgets that ship with Lizun and the
  repository's development widgets cannot be removed there.
- **HTTP with the system TLS.** Downloads use `ureq` with native-tls (SChannel on Windows) and
  the system root certificates, which avoids a second TLS stack and works behind corporate
  proxies that install their own root.

## Alternatives considered

| Option | Why not |
|---|---|
| Install right after validation, without a confirmation | The user would not see what the widget gets before it runs; with a link from a stranger that is the whole decision. |
| A widget store or index file | Needs hosting and moderation; links cover sharing for now, and an index can later point to the same links. |
| `git clone` | Requires git on the user's machine and pulls the whole history. |
| GitHub API (latest release, default branch name) | Rate-limited without a token (60 requests an hour per IP); github.com archive links do the same without it. |
| Installing from any URL | GitHub gives stable, inspectable sources; other hosts can come later behind the same staging and checks. |

## Consequences

- There are no automatic updates: installing the same link again updates the widget. Keeping
  the source link per widget to offer an Update button is a natural next step.
- A link to a branch installs whatever is on it at that moment; authors who want fixed versions
  publish releases.
- A branch name with a slash cannot be given in a `/tree/` link.
- The app gains three dependencies: `ureq` (HTTP), `zip` (archives) and the Tauri dialog plugin
  (file and folder pickers, allowed only in the settings window).
