# Plan: Stored-playlist support, mirroring mikMPD

Status: awaiting approval — no code changes yet.

Goal: bring winrmpc's playlist feature set up to parity with **mikMPD**
(`C:\Users\mikae\mikMPD`), including the **"Playing from &lt;playlist&gt;"**
marker in Now Playing. This mirrors mikMPD's setup, adapted from its
SwiftUI/`MPDStore` shape to winrmpc's iced (Message / update / view) shape.

---

## 0. What mikMPD does (the thing we're mirroring)

- **Library → Playlists list** (`PlaylistListView`): all stored playlists,
  filterable; swipe-to-delete, long-press-to-rename, toolbar **+** to *save the
  current queue as a playlist* (disabled when the queue is empty).
- **Playlist detail** (`PlaylistDetailView`): art (first track) + track count +
  total duration; **Play** (replace queue and play) and **Add** (append)
  buttons; a track list where tapping a row plays that track *in the playlist's
  context*; per-row "add to another playlist"; reorder + delete rows.
- **Shared "Add to Playlist" sheet** (`AddToPlaylistSheet`): reachable from Now
  Playing, albums, queue, and search. New-playlist field + list of existing
  playlists; appends the given URI(s).
- **"Playing from &lt;name&gt;" caption** in Now Playing
  (`NowPlayingView` + `MPDStore.playbackContext`): a client-side heuristic. MPD
  has **no** native "the queue came from playlist X" concept, so mikMPD keeps an
  in-memory `playbackContext: String?` that is:
  - **set** to the playlist name when the queue is loaded/played *from* a
    playlist (`loadPlaylist(replace|play)`, `playPlaylist(at:)`);
  - **cleared** to `nil` whenever the queue is mutated in a way that breaks that
    context (`clear`, `add`, `addAndPlay`, `enqueue(replace)`);
  - **not** cleared by reordering or deleting rows within the loaded queue;
  - **ephemeral** — not persisted; after a restart it's `nil` (unknown) even if
    the queue still matches a playlist. That's acceptable and matches mikMPD.

MPD commands mikMPD uses: `listplaylists`, `listplaylistinfo`, `load`,
`playlistadd`, `playlistdelete`, `playlistmove`, `rename`, `rm`, `save`, `clear`,
`play <pos>`.

---

## 1. What winrmpc already has

Client methods (`src/mpd/client.rs`) — **read side is done**, unused by the UI:

| Method | MPD command |
|---|---|
| `list_playlists() -> Vec<PlaylistInfo>` | `listplaylists` |
| `list_playlist(name) -> Vec<Song>` | `listplaylistinfo` |
| `save_playlist(name)` | `save` |
| `delete_playlist(name)` | `rm` |
| `load_playlist(name)` | `load` |

`PlaylistInfo { name, last_modified }` exists in `src/mpd/types.rs`. `Song`
already carries `file`, `pos`, `id`, `duration()`, and the display fallbacks the
detail view needs.

**The only current UI reference is broken:** `src/ui/views/browser.rs:115`
maps a `DirectoryEntry::Playlist(p)` to `Message::QueueAddUri(p.name)`, whose
handler calls `client.add(&name)`. `add` expects a *song URI*, not a playlist
name, so clicking a playlist in the Browser does the wrong thing. This plan
fixes that as a side effect (§7).

---

## 2. New client methods (`src/mpd/client.rs`)

Add the write-side commands mikMPD has that we lack. **Always** route names and
URIs through `escape()` (same rule as every other command here).

```rust
pub async fn playlist_add(&self, name: &str, uri: &str) -> MpdResult<()>;      // playlistadd "name" "uri"
pub async fn playlist_delete(&self, name: &str, pos: u32) -> MpdResult<()>;    // playlistdelete "name" pos
pub async fn playlist_move(&self, name: &str, from: u32, to: u32) -> MpdResult<()>; // playlistmove "name" from to
pub async fn rename_playlist(&self, old: &str, new: &str) -> MpdResult<()>;    // rename "old" "new"
```

`load_playlist`, `save_playlist`, `delete_playlist`, `list_playlist(s)`,
`clear`, and `play_pos` already exist and cover the rest. No change to
`protocol.rs`/`commands.rs` — these are all simple OK/ACK commands.

---

## 3. App state (`src/ui/app.rs`, `App` struct)

Follow the existing library patterns (`albums`, `album_songs: HashMap<…>`,
`selected_album`, `settings_renaming`/`settings_rename_input`):

```rust
// Playlists
playlists: Vec<PlaylistInfo>,                 // populated on entering the Playlists view
playlist_songs: HashMap<String, Vec<Song>>,   // name -> tracks, like album_songs
selected_playlist: Option<String>,

// "Playing from" heuristic — ephemeral, mirrors mikMPD's playbackContext
playing_from_playlist: Option<String>,

// UI: save-queue-as-playlist + rename dialogs (reuse the settings-rename idiom)
new_playlist_name: String,                    // text of the "save queue as" / "new playlist" field
playlist_renaming: Option<String>,            // Some(old_name) while renaming
playlist_rename_input: String,

// UI: shared "Add to Playlist" picker (see §6c)
add_to_playlist_uris: Option<Vec<String>>,    // Some(uris) while the picker view is open
```

All ephemeral — **nothing new in `AppConfig`/TOML** (playlists live on the MPD
server; the "playing from" marker is intentionally not persisted).

---

## 4. Messages (`src/ui/message.rs`)

New `Message` variants, grouped:

```rust
// === Playlists ===
PlaylistsLoaded(Vec<PlaylistInfo>),
PlaylistSelected(String),                 // navigate to detail + load its songs
PlaylistSongsLoaded(String, Vec<Song>),
PlaylistPlay(String),                     // replace queue + play  → sets playing_from
PlaylistAppend(String),                   // append (no context change)
PlaylistPlayAt(String, u32),              // play track at pos, in playlist context
PlaylistDelete(String),
PlaylistRemoveSong(String, u32),          // playlistdelete
PlaylistMoveSong(String, u32, u32),       // playlistmove (optional, phase 2)
SaveQueueAsPlaylist,                       // uses new_playlist_name
NewPlaylistNameChanged(String),
StartRenamePlaylist(String),
RenamePlaylistInput(String),
ConfirmRenamePlaylist,
CancelRenamePlaylist,
// shared "Add to Playlist" picker
OpenAddToPlaylist(Vec<String>),           // stash uris, navigate to picker
AddToPlaylistConfirm(String),             // add stashed uris to this existing playlist
AddToNewPlaylist,                          // add stashed uris to new_playlist_name
CloseAddToPlaylist,
```

New `View` variants:

```rust
pub enum View {
    // …existing…
    Playlists,
    PlaylistDetail(String),
    AddToPlaylist,      // shared picker (or render as an overlay — see §6c)
}
```

---

## 5. The "Playing from" heuristic — set & clear points (the crux)

This is the part that makes Now Playing show the playlist, and it must mirror
mikMPD's lifecycle exactly. In `App::update`:

**Set** `self.playing_from_playlist = Some(name)`:
- `PlaylistPlay(name)` — after `clear` + `load` + `play 0`.
- `PlaylistPlayAt(name, pos)` — after `clear` + `load` + `play pos`.

**Clear** `self.playing_from_playlist = None` in every handler that mutates the
queue outside a playlist load (these already exist — add one line each):
- `QueueClear`, `QueueAddUri`, `QueueAddAndPlay`, `QueueAddOnly`, `PlaySong`
- `PlayAlbum`, `QueueAlbum`
- `BrowseAddToQueue`, `SearchAddToQueue`, `RadioPlay`, and the CD play handlers
- `PlaylistAppend` — appending is *not* a context (matches mikMPD's `add` path)

**Do NOT clear** on `QueueRemove` / queue reorder — reordering or removing rows
within the loaded queue keeps the marker (matches mikMPD).

Because it's set/cleared in the update handlers (not derived from polling), a
`Tick`/`QueueUpdated` never disturbs it. It naturally survives as long as the
user keeps playing the loaded playlist and evaporates the moment they queue
something else — exactly mikMPD's behaviour.

> Design note: this is a heuristic, not ground truth. We accept the same
> limitations mikMPD does (lost on restart; a manual `load` from the Browser
> that we route through `PlaylistAppend` won't mark context unless it's the
> Play action). Keeping it identical to mikMPD is the point.

---

## 6. Views

### 6a. `src/ui/views/playlists_list.rs` (new) — `View::Playlists`
Mirror `albums_list.rs`. A scrollable list of `playlists`; each row navigates
via `Message::PlaylistSelected(name)`. Header holds:
- a **"Save queue as playlist"** `text_input` (bound to `new_playlist_name`) +
  button → `SaveQueueAsPlaylist` (disable when the queue is empty);
- inline **rename** using the `settings_renaming`/`ConfirmRename` idiom already
  in `settings_view` (`StartRenamePlaylist` swaps the row for a text field);
- a **delete** button per row → `PlaylistDelete`.
Empty-state text mirrors mikMPD's "No Playlists" copy.

### 6b. `src/ui/views/playlist_detail.rs` (new) — `View::PlaylistDetail(name)`
Model on `album.rs` (same header/back/art/list shape):
- Header: first track's art (reuse `art_key`/`art_handles`), track count, total
  duration (sum of `Song::duration()`), **Play** → `PlaylistPlay(name)`,
  **Add** → `PlaylistAppend(name)`.
- Track rows: click → `PlaylistPlayAt(name, pos)`; a small **⊕** → queue single
  (`QueueAddOnly(file)`); a **☰**/"Add to Playlist" → `OpenAddToPlaylist(vec![file])`;
  a **✕** → `PlaylistRemoveSong(name, pos)` then reload.
- (Phase 2) reorder → `PlaylistMoveSong`. iced 0.13 has no drag-reorder widget;
  do it with up/down buttons rather than porting SwiftUI `onMove`.

### 6c. Shared "Add to Playlist" picker — `View::AddToPlaylist`
iced 0.13 has no native modal sheet. Two options:
- **(Recommended) dedicated view** pushed on the nav stack: `OpenAddToPlaylist`
  stashes the URIs in `add_to_playlist_uris` and `NavigateTo(View::AddToPlaylist)`;
  the view shows a "new playlist" field (`AddToNewPlaylist`) + the list of
  existing playlists (`AddToPlaylistConfirm(name)`); either action does the adds
  and `GoBack`. Simplest, consistent with the app's existing nav-stack model.
- **Alternative:** an overlay via iced's `stack` widget. More faithful to the
  "sheet" feel but more layout work; defer unless desired.

Wire the entry points mikMPD has: a button/row action in Now Playing, album,
queue, and search that emits `OpenAddToPlaylist(uris)`.

### 6d. Now Playing marker (`src/ui/views/now_playing.rs`)
- Extend `now_playing::view(...)` with a `playing_from: Option<&str>` param
  (thread `self.playing_from_playlist.as_deref()` from `app.rs` render arm at
  `src/ui/app.rs:1296`).
- In the info column, after the album `link` (around line 78), when
  `Some(name)`: a clickable `link` "▤ Playing from {name}" → color
  `TEXT_MUTED`, size ~13, action `Message::PlaylistSelected(name)`.
- Keep the stable-skeleton rule that file already documents (lines 25-28): add
  the row inside the existing `info_items` vec so branch shapes don't diverge.

---

## 7. Navigation, sidebar, and the Browser fix

- **Sidebar** (`src/ui/widgets/sidebar.rs`): add
  `nav_button("Playlists", View::Playlists, current_view)` in the library group
  (after "Genres" or "Browse").
- **`on_view_enter`** (`src/ui/app.rs:1639`): `View::Playlists` fires
  `list_playlists()` → `PlaylistsLoaded`. `PlaylistSelected` navigates to
  `PlaylistDetail` and fires `list_playlist(name)` → `PlaylistSongsLoaded`
  (cache into `playlist_songs`, like `album_songs`).
- **Render arm** (`src/ui/app.rs:1296` `view()` match): add `View::Playlists`,
  `View::PlaylistDetail(name)`, `View::AddToPlaylist`.
- **Fix the Browser** (`src/ui/views/browser.rs:115`): change the
  `DirectoryEntry::Playlist(p)` action from the broken
  `Message::QueueAddUri(p.name)` to `Message::PlaylistSelected(p.name)` (open
  detail) — or `PlaylistAppend` if we prefer one-tap enqueue. Opening detail is
  the safer, more discoverable choice.

---

## 8. Validation & escaping

- Port mikMPD's `validatePlaylistName` (`Models.swift:344`): trim; reject empty
  or names containing `/ \ \n \r`. Use it in `SaveQueueAsPlaylist`,
  `AddToNewPlaylist`, and `ConfirmRenamePlaylist`; surface failures via
  `last_error` (the app already has this field + an error path).
- Every new client method interpolates through `escape()` — covered by the
  existing `escape` injection tests' guarantees; add one unit test per new
  method's command string if we want parity with the existing `client.rs` tests.

---

## 9. Suggested implementation order

| # | Item | Size | Risk |
|---|------|------|------|
| 1 | Client write methods (`playlist_add/delete/move`, `rename_playlist`) + tests | S | none |
| 2 | State + Messages + `View` variants | S | none |
| 3 | Playlists list view + sidebar nav + `on_view_enter` load | M | low |
| 4 | Playlist detail view (Play/Add/play-at/remove) | M | low |
| 5 | "Playing from" marker (state set/clear + Now Playing row) | S | low (heuristic — verify clear points) |
| 6 | Save-queue-as-playlist + rename/delete | S | low |
| 7 | Shared "Add to Playlist" picker + wire Now Playing/album/queue/search | M | low |
| 8 | Fix Browser playlist entry | XS | none |

Items 1-5 are the core parity (playlists browsable + playable + the marker) and
could ship as one minor bump (e.g. **0.5.0**, since it adds a `View` and user
feature). 6-8 fold in the same release or a follow-up.

---

## 10. Testing

- **Unit** (inline `#[cfg(test)]`, I/O-free): command-string formation for the
  four new client methods (mirror the `escape` tests), and `validate_playlist_name`.
- **Manual QA:**
  - Save queue → appears in list → open → Play replaces queue → Now Playing
    shows "Playing from &lt;name&gt;".
  - Tap a track in detail → plays that track, marker still shows.
  - Queue an album / add a song / clear → marker disappears.
  - Reorder/remove a row in the queue → marker persists.
  - Rename / delete playlist reflects on the server (`listplaylists`).
  - Add-to-playlist from Now Playing, album, queue, search.
  - Browser playlist row opens detail (no more silent `add` of the name).
