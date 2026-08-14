# Cross-platform correctness + row-action clarity — 0.4.2

Umbrella for five plans written together in one investigation pass. They are
grouped because three of the five turn out to share a single root cause —
**the app was built and tested on Windows, and every place where a platform
difference is silent rather than loud went unnoticed.** Nothing here is a
crash; everything here is something that quietly does less on macOS and Linux
than it does on Windows, with no log line and no visible error.

| # | Plan | Severity |
|---|---|---|
| 1 | [App icon on macOS and Linux](app-icon-cross-platform.md) | macOS: no icon at all. Wayland: no icon at all. X11/Windows: fine |
| 2 | [Persistent storage discoverability](persistent-storage-cross-platform.md) | Works everywhere; **undiscoverable** on macOS, and silently disabled if the path can't resolve |
| 3 | [Current-song highlighting in every song list](current-song-highlighting.md) | All platforms; Queue is the only list that does it |
| 4 | [Row-action button affordance](row-action-affordance.md) | **Glyphs render as tofu boxes on macOS and Linux** — the buttons are literally unreadable, not merely unclear |
| 5 | [Network fetching on all three OSes](network-fetch-cross-platform.md) | Linux build needs OpenSSL headers; MusicBrainz User-Agent is a placeholder that risks a block |

## The shared root cause

Three findings are the same mistake in three places — a Windows-only
resource named directly, with a silent fallback when it's absent:

- **`src/ui/widgets/link.rs:36`** — `Font::with_name("Segoe UI Symbol")`. The
  code's own comment says the bundled iced default font lacks `▶`/`＋`. On
  macOS and Linux that font does not exist, iced falls back to the default,
  and the fallback is exactly the font the comment says renders tofu. This is
  plan 4's real cause and it is why the user reads the buttons as
  "hard to understand what they actually do".
- **`src/main.rs`** — `icon: icon::make_icon()` is wired to winit's
  `set_window_icon`, which is a **documented no-op on macOS** and an **empty
  no-op on Wayland**. Plan 1.
- **`Cargo.toml`** — `reqwest` on default features pulls `native-tls`, i.e.
  `openssl-sys` on Linux. It's schannel on Windows and Security.framework on
  macOS, so only the Linux build gains a system dependency. Plan 5.

In each case the Windows path is the one that happens to work, and the other
two platforms degrade without saying so. The recurring fix shape is the same
too: **make the fallback loud, or remove the platform-specific dependency
entirely** (bundle the font, generate the icon assets, use rustls).

## Ordering

Plans 4 and 5 are the ones that change behaviour a user notices immediately,
and both are small. Plan 3 is self-contained and touches the most files. Plan
1 needs packaging work that has no natural home in the current release flow
and is the only one that may slip past 0.4.2.

Suggested order: **4 → 5 → 3 → 2 → 1**.

## Landing

All five are being planned on `improve/cross-platform-and-ui`. Implementation
commits land on that branch; the branch merges into a `release/v0.4.2` branch
when the batch is ready, following the flow in CLAUDE.md's "Release Process"
(or the `release` skill).

## Verification reality check

**None of this has been verified on real macOS or Linux hardware.** Every
claim below is read out of the source of `iced 0.13.1`, `iced_winit 0.13.0`,
`winit 0.30.13`, `directories 6` and `Cargo.lock` as vendored in
`~/.cargo/registry`, with file and line references given so each one can be
re-checked. The findings are strong (several are explicit no-op function
bodies with explanatory comments), but each plan ends with a
**"How to confirm on the real OS"** section, and those steps are the actual
acceptance criteria.
