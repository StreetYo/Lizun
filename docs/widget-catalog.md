# Widget catalog

Widgets for Lizun that you can install. Each one lives in its own repository; this page only
lists them.

## Installing a widget

1. Copy the widget's link from the table below.
2. In Lizun, open Settings > Widgets > Add a widget, paste the link and press Install.
3. Lizun checks the widget and shows its permissions and network access; it is installed only
   after you confirm.

The same link updates a widget that is already installed; its settings and data are kept.

## Widgets

| Widget | What it does | Permissions | Link |
|---|---|---|---|
| Anki | Review flash cards: your Anki decks through AnkiConnect, or decks kept in the widget | `storage`, network `http://127.0.0.1:8765`, starts Anki | [Lizun-anki](https://github.com/StreetYo/Lizun-anki) |
| Notepad | A note you can write and edit right in the island | `storage` | [lizun-notepad](https://github.com/StreetYo/lizun-notepad) |

Permissions: `storage` keeps the widget's data on this computer, `media:read` and
`media:control` see and control what is playing, `activity` shows something in the closed
island. A widget may also reach the network, only the sites its manifest lists, and start
programs its manifest names; Lizun shows them before installing.

## Adding your widget

1. Write the widget as described in [Writing a Lizun widget](widgets.md) and put it in a public
   GitHub repository, with `manifest.json` at the root.
2. Make sure it installs from the repository link and works in English (translations are
   welcome).
3. Its README says what the widget does and why it needs each permission and network site.
4. Open a pull request that adds one row to the table above, in alphabetical order.

A widget is removed from the list when it stops installing or does something its README does not
say.
