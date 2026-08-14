---
name: release
description: Cut a full versioned release — bump version, branch + PR + merge to main, tag, and let CI build and attach the Linux and Windows binaries. Use when the user says "release", "cut a release", "create a release", or wants a new versioned binary published with a user-facing change.
---

# release

Publish a new versioned release of winrmpc: version bump → branch → PR → merge → tag → **CI builds and attaches both binaries**.

This is the heavier sibling of the **ship** skill; use it when the change is user-facing and should produce downloadable binaries.

## The binaries are built by CI, not by you

`.github/workflows/release.yml` owns the build. Pushing a `vX.Y.Z` tag makes it:

1. run `cargo test --all-targets` on **ubuntu, windows and macOS**, and fail the release if any of them fails,
2. build `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`,
3. create the GitHub release (or upload into one that already exists) with both assets:
   - `winrmpc-vX.Y.Z-linux-x86_64.tar.gz` — binary + `packaging/linux/` + README + LICENSE
   - `winrmpc-vX.Y.Z-windows-x86_64.exe`

**Do not try to build the `.exe` locally on Linux.** There is no cross toolchain in this repo's assumptions — it would need `cargo-xwin` + `clang`/`lld`, or mingw. Building on `windows-latest` also keeps the MSVC ABI earlier releases shipped.

**A local `cargo build --release` is still worth doing** as a compile check, but its output is not what gets published.

## Before releasing: get an .exe to test

The release should not be the first time the Windows build is exercised. Trigger the workflow manually — it builds the same artifacts and **publishes nothing** (the `publish` job is gated on `refs/tags/`):

```bash
gh workflow run release.yml --ref <branch>
gh run watch                      # or: gh run list --workflow=release.yml
gh run download <run-id> -D ./ci-artifacts
```

> **`workflow_dispatch` only works once `release.yml` is on the default branch.**
> GitHub will not offer a manual run for a workflow that exists solely on a
> feature branch — `gh workflow run` fails with "could not find any workflows".
> The `--ref` flag chooses which branch's *code* to build, but the workflow
> itself must already be on `main`.
>
> So the first time round, the order is: **merge the workflow to `main` first**
> (via `ship`), then dispatch a manual run from whatever branch you want to
> test, then tag. After that it is available for every future release.

## Environment notes (this repo)

- Repo root: `/home/mikael/git/winrmpc`. Shell is **bash** — use `git commit -F -` with a heredoc for multi-line messages.
  - *If running this on Windows/PowerShell instead*: multiline strings are `@'...'@` here-strings, and `gh pr create` / `gh release create` choke on inline multiline markdown — write the body to a temp file and use `--body-file` / `--notes-file`.
- `gh` CLI must be authenticated (`gh auth status`).
- Default branch is `main`.

## Steps

1. **Determine the version.** Read the current one from `Cargo.toml`. Bump per semver intent: patch for fixes/small features, minor for larger ones. Confirm with the user if it isn't obvious from their request.

2. **Branch and bump.**
   ```bash
   git checkout -b release/vX.Y.Z
   ```
   Set `version = "X.Y.Z"` in `Cargo.toml` **and** keep the `## Current Version` line in `CLAUDE.md` in sync.

3. **Verify locally.**
   ```bash
   cargo test
   cargo build --release
   ```
   Zero warnings is the standard here (`cargo check --all-targets` with `RUSTFLAGS=-D warnings` is what CI enforces). If the change touches the MPD/Snapcast protocol or external lookups, also run the live suite against a real server:
   ```bash
   WINRMPC_TEST_MPD=host:6600 WINRMPC_TEST_SNAPCAST=host:1705 \
     cargo test -- --ignored --test-threads=1
   ```

4. **Commit** the bump plus any changes, with a body explaining the why and the trailer:
   ```bash
   git commit -F - <<'EOF'
   chore: bump to X.Y.Z

   <why this release exists, and what changed>

   Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
   EOF
   ```

5. **Push and open the PR.**
   ```bash
   git push -u origin release/vX.Y.Z
   gh pr create --title "Release vX.Y.Z" --body-file pr_body.md && rm pr_body.md
   ```

6. **Merge, delete the branch, sync main.**
   ```bash
   gh pr merge <number> --merge --delete-branch
   git checkout main && git pull
   ```

7. **Tag and push — this is what triggers the build.**
   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

8. **Write the release notes while CI builds.** The build takes several minutes, so create the release with proper notes in that window; the workflow detects an existing release and uploads into it rather than creating its own with auto-generated notes:
   ```bash
   gh release create vX.Y.Z --title "vX.Y.Z" --notes-file release_notes.md && rm release_notes.md
   ```
   Notes should be user-facing, grouped under **New features** / **Bug fixes** / **Under the hood**. If the workflow got there first, it will have used `--generate-notes`; replace them with `gh release edit vX.Y.Z --notes-file release_notes.md`.

9. **Watch the run and confirm both assets landed.**
   ```bash
   gh run watch
   gh release view vX.Y.Z --json assets --jq '.assets[].name'
   ```
   Expect exactly two names: the `-linux-x86_64.tar.gz` and the `-windows-x86_64.exe`.

10. **Report** the release URL and list the attached assets. If the Windows job failed, say so plainly — a release with only a Linux binary is a half-finished release, not a finished one.

## Pitfalls

- **Never amend after a hook failure** — create a new commit.
- **Don't hand-upload a locally built binary.** On Linux you cannot produce the `.exe` at all, and a hand-built Linux binary skips the tarball packaging (`packaging/linux/` must travel with it, or the Wayland icon can't resolve).
- **A tag push is the trigger.** Deleting and re-pushing a tag to re-run the build also re-runs `publish`; use `--clobber` semantics already in the workflow rather than deleting release assets by hand.
- The workflow installs `libxkbcommon-dev`/`libwayland-dev`/`libx11-dev` on Linux and deliberately **not** `libssl-dev`. If it ever fails on a missing libssl, something re-enabled `native-tls` in `Cargo.toml` — see CLAUDE.md's "Outbound HTTP".
