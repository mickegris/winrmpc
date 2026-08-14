# Persistent storage on all three OSes

Part of [cross-platform-and-ui-0.4.2](cross-platform-and-ui-0.4.2.md).

## The short answer

**Persistence does work on macOS and Linux.** The files were there during the
macOS test — they're in a directory Finder hides by default, under a name that
doesn't contain the string "winrmpc" in the form anyone would search for. This
plan is therefore mostly about **discoverability and failure-loudness**, not
about making storage work.

## Where the files actually are

Both paths come from one call shape (`src/config/settings.rs:208-216`):

```rust
ProjectDirs::from("com", "winrmpc", "winrmpc")
```

With `directories 6`, that resolves to:

| | config (`config.toml`) | cache (`winrmpc.redb`) |
|---|---|---|
| **Windows** | `%APPDATA%\winrmpc\winrmpc\config\` | `%LOCALAPPDATA%\winrmpc\winrmpc\cache\` |
| **Linux** | `~/.config/winrmpc/` | `~/.cache/winrmpc/` |
| **macOS** | `~/Library/Application Support/com.winrmpc.winrmpc/` | `~/Library/Caches/com.winrmpc.winrmpc/` |

macOS is the odd one out for two compounding reasons, which together are
almost certainly the whole of the reported symptom:

1. `~/Library` is **hidden in Finder by default** (⇧⌘. toggles it, or
   Go → Go to Folder).
2. The directory is named `com.winrmpc.winrmpc`, not `winrmpc` — `directories`
   uses the reverse-DNS bundle-style id on macOS, assembled from the three
   `ProjectDirs::from` arguments. Searching Finder or Spotlight for "winrmpc"
   does not obviously lead there, and Spotlight does not index `~/Library` by
   default anyway.

Note also that on macOS `config_dir()` and `data_dir()` are the **same**
directory (`Application Support`) — that's the platform convention, not a bug,
and nothing in the app depends on them differing.

CLAUDE.md documents only the Windows path today. That should be a table.

## Real failure modes worth fixing

Discoverability aside, there are three places where storage can fail or
silently not happen, and none of them tells the user:

### 1. `ProjectDirs::from` returning `None` disables persistence in silence

`config_dir()`/`cache_dir()` both return `Option<PathBuf>`. If home resolution
fails (no `$HOME` on Linux, an unusual service/sandbox context), then:

- `AppConfig::config_path()` → `None` → `load_from(None)` returns
  `Self::default()` and **returns before ever attempting a write**
  (`settings.rs:242-244`). `save_to(None)` is likewise a silent `Ok(())`
  (`settings.rs:301` — the `if let Some(path)` just falls through). Every
  setting the user changes appears to work and is gone at restart, with no log
  line at all.
- `cache_dir()` → `None` → `app.rs:211-212` falls back to
  `PathBuf::from("./cache")`, i.e. **relative to the working directory**. From
  a Finder or desktop launch that is very often `/`, where the create will
  fail; `Store::open` then wipes and retries, then quietly runs on
  `InMemoryBackend`.

**Fix**: log at ERROR in both `None` branches, with the specific consequence
("settings will not be saved this session"). These are one-line additions and
they turn a silent class of bug into something the in-app Log view shows.

### 2. The in-memory `Store` fallback is logged at WARN and never surfaced

`Store::open` (`store/mod.rs:62-88`) degrades correctly — wipe and rebuild,
then `InMemoryBackend` — but the final fallback means **no art, lyrics, bios or
play history persist for the whole session**, and the only trace is a
`tracing::warn!`. It should be ERROR (the app is running with a core feature
off), and `Store` should expose an `is_persistent() -> bool` so the Settings
view can say so.

### 3. `save()` failures on the config path are partly swallowed

`save_to` propagates `create_dir_all`/`write` errors, but several call sites do
`config.save().ok()`. That's defensible for the frequent auto-saves, but on a
read-only or permission-denied config dir the user gets no signal at all. Worth
a survey of `\.save()` call sites and a shared `save_and_log()` helper that
logs the error once rather than dropping it.

## Plan

### A. Show the paths in the app

Add a **Storage** section to `App::settings_view()` (Settings has no view
module — it's a method on `App`, per CLAUDE.md):

```
Storage
  Config   ~/Library/Application Support/com.winrmpc.winrmpc/config.toml   [Open folder]
  Cache    ~/Library/Caches/com.winrmpc.winrmpc/winrmpc.redb   (12 MB)     [Open folder]
```

The **Open folder** buttons use the `open` crate — already a dependency, used
by `widgets/link.rs` for URLs — via `open::that(dir)`. That maps to Finder on
macOS, Explorer on Windows, `xdg-open` on Linux, and answers the original
question directly from inside the app.

Show the resolved path even when it doesn't exist yet, and show an explicit
"unavailable — settings will not be saved" line when `config_path()` is `None`.
The cache row can reuse the existing `Store::art_cache_bytes` readout that
already backs Settings → Cache.

### B. Log the resolved paths at startup

One INFO line each for config and cache at `App::new`, so they land in the
in-app Log view and in stderr for dev runs. This is the zero-UI version of A
and is worth having even after A ships.

### C. Make the silent paths loud

The three fixes described above: ERROR on `None` from either directory helper,
ERROR (not WARN) plus an `is_persistent()` flag on the in-memory `Store`
fallback, and a `save_and_log()` helper for the `.ok()` call sites.

### D. Environment overrides

Optional but cheap, because `load_from(Option<PathBuf>)` and
`save_to(Option<&Path>)` are **already** the path-injectable cores (they exist
for the `config/settings.rs` tests). Honour `WINRMPC_CONFIG_DIR` and
`WINRMPC_CACHE_DIR` when set. Useful for portable installs, for running two
profiles against two servers, and for reproducing a user's config without
touching the real one.

### E. Documentation

- CLAUDE.md: replace the Windows-only path with the three-row table above.
- README: a short "Where your settings and cache live" section — this is the
  question that prompted the plan, so it belongs somewhere a user reads.

## Explicitly not doing

**Not changing the `ProjectDirs::from("com", "winrmpc", "winrmpc")`
arguments.** A friendlier macOS directory name would orphan every existing
install's config and cache, and would need a migration path that reads the old
location. The cache is disposable but the config is not — servers, radio
stations and saved partitions all live there. Discoverability is better solved
by A than by moving files out from under people.

## How to confirm on the real OS

- **macOS**: launch, change a setting, quit, relaunch — the setting persists.
  Then `ls -la ~/Library/Application\ Support/com.winrmpc.winrmpc/` and
  `ls -la ~/Library/Caches/com.winrmpc.winrmpc/` to see `config.toml` and
  `winrmpc.redb`. After A, the Settings → Storage row should print exactly
  those two paths and the Open folder button should land in the right place.
- **Linux**: same, at `~/.config/winrmpc/config.toml` and
  `~/.cache/winrmpc/winrmpc.redb`.
- **Windows**: confirm the existing paths are unchanged.
- **Failure path**: run with `HOME=` unset (Linux) and check the new ERROR
  lines appear in the Log view rather than the app silently not saving.
