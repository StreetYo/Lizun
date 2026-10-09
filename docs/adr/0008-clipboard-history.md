# ADR 0008: Clipboard history

Status: accepted (2026-10-10).

## Context

People copy something, copy something else, and want the first thing back. Windows has its own
clipboard history (Win+V), but it is off by default, opens away from where the user looks and
keeps nothing across a restart unless synced to a Microsoft account. The island is already the
place for things kept at hand (the file tray), so the history belongs there as a tab.

## Decision

- **A Clipboard tab in the open island**, next to Widgets and Tray. Entries are cards, newest
  first, in a row that scrolls sideways (the mouse wheel scrolls it too): the start of a text,
  a picture with its size, or copied files, and how long ago it was copied. Files show as a
  2×2 grid the way phones show albums: up to four files fill it; with more, three are shown and
  the last cell says how many others there are (+N). A single file fills the card, with its name
  below. Pictures among them show their preview; moved or deleted files are dimmed. A click
  puts the entry back on the clipboard and it moves to the top; the cross on a card removes it;
  the trash button in the header clears everything after a second click. The tab is gone while the history is off.
- **The core watches the clipboard** (`src-tauri/src/clipboard.rs`, Windows glue in
  `src-tauri/src/platform/win/clipboard.rs`). A thread compares the clipboard's sequence number
  twice a second, which costs nothing, and reads only when it changes. Files come first
  (`CF_HDROP`, which Explorer and most apps put there; some add the paths as text too), then
  text, then a picture from the PNG format (keeps transparency) or the bitmap. Other formats
  are not kept. A copy counts as its first kind only: when that kind is turned off, nothing is
  kept, so files that come with their paths as text do not turn into a text entry.
- **Files are kept as their paths**, like the file tray; the files stay where they are. A click
  puts the paths that still exist back on the clipboard as a copy (`Preferred DropEffect` set
  to copy), so pasting in Explorer, a messenger or a mail copies the files again. Files that
  were cut also come back as a copy, since the originals are gone after the first paste.
  Copies of more than 10,000 files are skipped.
- **Copying the newest entry again changes nothing; copying an older one moves it to the top.**
  Texts and file lists are compared as they are, pictures by size and a hash of their pixels.
- **Private clips are never read**: Lizun skips the clipboard while it holds
  `ExcludeClipboardContentFromMonitorProcessing`, `Clipboard Viewer Ignore` or
  `CanIncludeInClipboardHistory` set to 0, the formats password managers add and Windows' own
  history honours. Texts over 1 MB and pictures over 40 megapixels are skipped.
- **Settings** (Settings > Clipboard, stored in `preferences.json`): the history on or off (on
  by default; turning it off deletes it), which kinds it keeps (text, pictures and files, each
  with its own switch, all on by default; turning one off deletes its entries, and the last one
  cannot be turned off), how long entries stay (1 hour, 1 day by default, 1 week,
  30 days or forever) and a limit on the number of entries (on by default, 50, from 1 to 1000),
  which can be turned off. Lifetime and limit work together: an entry goes as soon as either
  rule says so. Expired entries are dropped every 30 seconds and whenever the settings change.
- **The history survives a restart**: `history.json` in the `clipboard` folder of the app data
  folder, with a PNG file per picture. The island gets previews only (the first 300 characters
  of a text, the path of a picture, served through the asset protocol); a click copies the
  whole entry from the core.

## Alternatives considered

| Option | Why not |
|---|---|
| Reading Windows' own clipboard history (`Clipboard.GetHistoryItemsAsync`) | Works only while the user has turned it on, and gives Lizun no say over lifetime or limit. |
| `AddClipboardFormatListener` with a hidden window | Instant, but needs a window and a message loop for what a sequence number checked twice a second does as well. |
| The `arboard` crate | Adds the `image` crate and its codecs; Lizun needs only text, bitmaps and PNG, and the private-clip formats need Win32 calls anyway. |
| Keeping the history in the island page (`localStorage`) | The page cannot see the clipboard while it is in the background, and pictures do not fit there. |

## Consequences

- The history is stored unencrypted in the user's profile, like the file tray's paths. The
  settings say that it stays on this computer, and turning it off deletes it.
- Passwords copied from apps that do not mark them private (a text editor, a web page) are kept
  until they expire; a short lifetime limits that.
- Rich text and HTML are not kept; a text copied from Word comes back as plain text.
- A file entry is only a list of paths: if the files are moved or deleted, it cannot bring them
  back.
- Clicking an entry only copies it: the user still pastes it themselves, since the island never
  takes the focus from their app.
