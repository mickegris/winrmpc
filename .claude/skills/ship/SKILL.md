---
name: ship
description: Ship the current changes to main via a branch + PR + merge, with NO version bump or release. Use when the user says "ship", "ship it", "ship the changes", or wants non-release changes (tests, docs, refactors, bug fixes that don't warrant a tagged release) merged to main.
---

# ship

Merge the current working changes to `main` through a clean branch → PR → merge flow. **No** version bump, tag, GitHub release, or exe upload — that is the `release` skill's job.

## When to use
- Test additions, docs, refactors, internal fixes with no user-facing binary change.
- Anything the user describes as "ship" rather than "release".

If the change is user-facing and should produce a new versioned binary, use the **release** skill instead. If unsure which the user wants, ask.

## Environment notes (this repo)
- Run all commands from the repo root: `C:\Users\mikae\winrmpc`.
- `gh` CLI: `C:\Program Files\GitHub CLI\gh.exe` (or `gh` if on PATH).
- Shell is **PowerShell** — multiline strings use `@'...'@` here-strings, not bash heredocs.
- `gh pr create --body` chokes on inline multiline/markdown in PowerShell. Write the body to a temp file and use `--body-file`, then delete it.
- Default branch is `main`.

## Steps

1. **Review what's changing.** `git status` and `git diff --stat` to see the modified files and confirm the scope.

2. **Pick a branch name** from the change type, e.g. `tests/<topic>`, `docs/<topic>`, `fix/<topic>`, `refactor/<topic>`. Create it:
   ```powershell
   git checkout -b <branch>
   ```

3. **Stage and commit.** Stage the specific files involved (not blanket `git add -A` unless appropriate). Write a clear message with a body explaining the why. End with the Co-Authored-By trailer:
   ```powershell
   git add <files>
   git commit -m @'
   <short summary line>

   - <bullet on what/why>
   - <bullet>

   Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
   '@
   ```

4. **Push** the branch:
   ```powershell
   git push -u origin <branch>
   ```

5. **Open the PR.** Write the body to `pr_body.txt`, create the PR, then remove the file:
   ```powershell
   gh pr create --title "<title>" --body-file pr_body.txt
   Remove-Item pr_body.txt
   ```
   Body should have a `## Summary` and, for code changes, a short `## Test plan`.

6. **Merge, delete branch, sync main:**
   ```powershell
   gh pr merge <number> --merge --delete-branch
   git checkout main
   git pull
   ```

7. **Report** the merged PR URL and confirm local `main` is up to date.

## Pitfalls
- If a commit hook fails, **never amend** — fix the issue and create a new commit.
- Verify tests still pass (`cargo test`) before opening the PR when code changed.
