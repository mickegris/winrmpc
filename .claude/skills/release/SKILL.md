---
name: release
description: Cut a full versioned release — bump version, branch + PR + merge to main, tag, GitHub release, and upload the built exe. Use when the user says "release", "cut a release", "create a release", or wants a new versioned binary published with a user-facing change.
---

# release

Publish a new versioned release of winrmpc: version bump → branch → PR → merge → tag → GitHub release → exe upload. This is the heavier sibling of the **ship** skill; use it when the change is user-facing and should produce a downloadable binary.

## Determine the version
- Read the current version from `Cargo.toml`.
- Bump per semver intent: patch (`0.1.5 → 0.1.6`) for fixes/small features, minor for larger features. Confirm the target version with the user if it isn't obvious from their request.

## Environment notes (this repo)
- Run all commands from the repo root: `C:\Users\mikae\winrmpc`.
- `gh` CLI: `C:\Program Files\GitHub CLI\gh.exe` (or `gh` if on PATH).
- Shell is **PowerShell** — multiline strings use `@'...'@` here-strings, not bash heredocs.
- `gh pr create`/`gh release create` choke on inline multiline markdown in PowerShell — write the body/notes to a temp file and use `--body-file` / `--notes-file`, then delete it.
- Editing `Cargo.toml` may require a prior Read in-session, or just use `Set-Content`/an exact Edit on the `version = "X.Y.Z"` line.

## Steps

1. **Branch + bump.** Create `release/vX.Y.Z`, then set `version = "X.Y.Z"` in `Cargo.toml`:
   ```powershell
   git checkout -b release/vX.Y.Z
   ```

2. **Build the release binary** (also confirms it compiles; embeds the icon via build.rs):
   ```powershell
   cargo build --release   # → target\release\winrmpc.exe
   ```
   Run `cargo test` too if code changed.

3. **Commit** the bump + changes with a descriptive body and the Co-Authored-By trailer:
   ```powershell
   git add <files> Cargo.toml
   git commit -m @'
   Bump to X.Y.Z: <one-line theme>

   - <highlight>
   - <highlight>

   Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
   '@
   ```

4. **Push** and **open the PR** (body via temp file):
   ```powershell
   git push -u origin release/vX.Y.Z
   gh pr create --title "Release vX.Y.Z" --body-file pr_body.txt
   Remove-Item pr_body.txt
   ```

5. **Merge, delete branch, sync main:**
   ```powershell
   gh pr merge <number> --merge --delete-branch
   git checkout main
   git pull
   ```

6. **Tag and push the tag** (from the updated main):
   ```powershell
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

7. **Create the GitHub release** (notes via temp file), then upload the exe:
   ```powershell
   gh release create vX.Y.Z --title "vX.Y.Z" --notes-file release_notes.txt
   Remove-Item release_notes.txt
   gh release upload vX.Y.Z "target\release\winrmpc.exe"
   ```
   Notes should be user-facing: group under "New features" / "Bug fixes".

8. **Report** the release URL and confirm the exe is attached.

## Pitfalls
- If a commit hook fails, **never amend** — create a new commit.
- Make sure the release build finished before uploading; upload from `target\release\winrmpc.exe`.
- Keep the `## Current Version` line in `CLAUDE.md` in sync with the bump.
