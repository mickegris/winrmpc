# Plan: the limits on Recently Added and Recently Played

Status: **Part A implemented** (0.4.4); **Part B not done** — Recently Played
was reported as working, so its cap stays at 30 days / 100 entries. Follow-up
to
[`recently-added-and-played-history.md`](recently-added-and-played-history.md),
which shipped both features in 0.4.2 and gave each a cap that is now the
thing being hit.

Reported symptom: *"albums I add have stopped showing up."* Both lists have a
cap, and **both fail silently** — a truncated list is indistinguishable from a
complete one, which is why this reads as "stopped working" rather than "is
full".

---

## Part A — Recently Added: the cap truncates the *wrong* end

`src/ui/app.rs:3592` (`on_view_enter`, `View::RecentlyAdded`) issues:

```
find "(modified-since '<now-30d>')" window 0:2000
```

and `Message::RecentlyAddedLoaded` (`src/ui/app.rs:1231`) then sorts the
result newest-`last_modified`-first *client-side*.

**That ordering is the bug.** MPD returns `find` matches in database order —
effectively directory traversal, documented as undefined — and `window 0:2000`
slices *that* order. Only the surviving 2000 are sorted. So once a library has
more than 2000 tracks modified inside the window, **which** 2000 you see is
decided by path order, and a freshly added album whose path sorts late is
dropped before the client ever sees it. No amount of client-side sorting
recovers a row the server never sent.

It also fails at exactly the wrong moment: under the cap everything looks
perfect, and the first time a big import crosses it the list stops updating
and stays wrong.

Two secondary problems in the same query:

- **`modified-since` is mtime, not add-time.** `cp -p`, `rsync -a` and
  restored backups all carry the original mtime forward, so files added today
  can be dated years ago and never appear. Editing a tag on an old file does
  the reverse — it resurfaces as "recently added".
- **Errors are swallowed** — `.unwrap_or_default()` turns a failed `find` into
  an empty page with nothing in the log and no toast.

### A1 — sort server-side (the actual fix)

```
find "(modified-since '<since>')" sort -Last-Modified window 0:<limit>
```

MPD's `sort` runs before `window`, and a `-` prefix makes it descending, so
the cap now truncates the **oldest** matches — which is what the view wants.
The client-side sort in `RecentlyAddedLoaded` stays, because it still has to
order the fallback path below.

`sort` on `Last-Modified` needs MPD 0.22+. Older servers answer `ACK [2@0]`,
so `find_recently_added` retries once without the `sort` clause, mirroring
`list_albums_by_artist`'s existing pre-0.21 `group` fallback. Cache the answer
on `MpdClient` (an `AtomicU8` tri-state, or `Option<bool>` behind the existing
mutex) so a legacy server isn't probed on every view entry.

### A2 — name the limit, and say when it bit

- `RECENTLY_ADDED_LIMIT` / `RECENTLY_ADDED_DAYS` as named constants next to
  the query instead of `2000` and `30` inline. The bound itself stays — an
  unbounded `modified-since` scan can outrun the socket read, which is why it
  was added.
- Raise the song limit to **5000**. With A1 in place the limit only decides
  how far back the list reaches, and 2000 tracks collapse to roughly 150
  albums — thin for a view whose whole job is "what's new".
- Widen the window to **90 days**. Costs nothing now that the cap takes the
  newest rather than an arbitrary slice.
- **Log at INFO when the result is exactly `limit`**, naming the oldest
  `last_modified` reached. That is the only signal that older additions were
  cut, and right now there is none.

### A3 — prefer `Added` over `Last-Modified` where the server has it

*Implemented.* The ladder lives on `RecentlyAddedRung` in `src/mpd/types.rs`
rather than in the client, so `query()` is a pure function and the ordering of
`sort` before `window` is unit-testable.

MPD 0.24 tracks a real database *add* time: the `added-since` filter and the
`Added` sort name. That is the tag this view actually wants, and it is immune
to the mtime problems above. The target server here is 0.24.0.

```
find "(added-since '<since>')" sort -Added window 0:<limit>
```

Same try-once-and-fall-back shape as A1, so the ladder is:
`added-since` → `modified-since` + `sort` → `modified-since` unsorted. One
cached capability flag per rung.

**Only an ACK falls through to the next rung.** A connection error propagates
instead — walking the whole ladder on a dead socket would cache a rung the
server never actually rejected, and the app would then permanently use a
weaker query than it needs to.

### A4 — surface the window in the view

`views::albums_list::view` already takes a `title`. Add an optional subtitle
line — "added in the last 90 days" — so a short list reads as a short list
rather than a broken one.

### A5 — stop swallowing the error

Route the failure through `App::toast_error` like every other user-visible
failure, instead of `.unwrap_or_default()`.

---

## Part B — Recently Played: 100 entries is small

`prune_recently_played` (`src/mpd/types.rs:768`) keeps 30 days **and** the 100
newest entries. Albums mode derives its tiles from that same list
(`recently_played_albums`), so the album view is capped by a *track* count:
100 tracks is roughly 8–12 albums, and a few days of ordinary listening ages
an album out entirely.

- **B1** — raise to **500 entries / 90 days**, as named constants with the
  reasoning attached. Cost is one redb write of a JSON `Vec` per committed
  play; at 500 entries that is a few tens of KB per write, on a store already
  holding downscaled JPEGs.
- **B2** — the pruning runs **on commit**, so raising the cap takes effect for
  new plays immediately and recovers nothing already dropped. Worth saying out
  loud in the release notes; there is no way to get the old history back.
- **B3** — not planned: making the cap a setting. It is one number that only
  matters when it is too small; the fix is a bigger default, not a knob.

---

## Tests

- `find_recently_added` builds the `sort -Last-Modified` form, and the
  fallback path builds the unsorted one (unit, on the command string).
- Capability flag is probed once, not per call.
- `prune_recently_played` tests updated to the new constants — the existing
  `caps_at_100` test becomes the guard that the constant and the assertion
  can't drift apart.
- **Live** (`live_tests.rs`, `#[ignore]`d): `live_recently_added_is_newest_first`
  — request a window *smaller* than the number of matches, assert the result
  is descending by `last_modified`, then re-query with a wider window and
  assert it surfaces nothing newer. That second half is what actually
  distinguishes "sorted" from "an arbitrary slice that happened to come back
  in order".
- **Offline, against a mock MPD** (`mock_mpd_rejecting`): the pre-0.24 and
  pre-0.22 fallbacks can only be exercised against a server that *lacks* the
  newer syntax, and the one real server available runs 0.24 — so a mock is the
  difference between "the fallback is written" and "the fallback works". Five
  tests: 0.23 lands on the middle rung and keeps its `sort`; 0.21 reaches the
  bottom rung and reports itself unsorted; the probe happens once, not per
  call; a clone shares the cached rung; and a dropped socket propagates
  instead of walking the ladder.
- **Live**: `live_recently_added_reports_which_rung_the_server_answers_on` —
  prints the highest supported rung and asserts it is cached rather than
  re-probed, so A3's ladder isn't guesswork.

### What landed

| | |
|---|---|
| `src/mpd/types.rs` | `RecentlyAddedRung` — the three query forms, the ladder, `is_server_sorted()` |
| `src/mpd/client.rs` | `find_recently_added` returns `(Vec<Song>, RecentlyAddedRung)`; `recently_added_rung: Arc<AtomicU8>` caches the probe across clones, and `connect()` clears it so an upgraded server isn't stuck on the old rung |
| `src/ui/app.rs` | `RECENTLY_ADDED_DAYS` (90) / `RECENTLY_ADDED_LIMIT` (5000); handler toasts on error, logs on truncation, warns on an unsorted rung |
| `src/ui/views/albums_list.rs` | optional `subtitle` — "· last 90 days" |

## Not doing

- Paging Recently Added. The cap exists to bound one query; a "load more"
  affordance is a different feature and this view is a glance, not a browser.
- Recording an add-time client-side for pre-0.24 servers. It would need a
  full-library scan on first run and a persistent snapshot to diff against —
  far more machinery than the mtime heuristic it would replace.
