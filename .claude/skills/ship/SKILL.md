---
name: ship
description: Ship the current changes to main via a branch + PR + merge, with NO version bump or release. Use when the user says "ship", "ship it", "ship the changes", or wants non-release changes (tests, docs, refactors, bug fixes that don't warrant a tagged release) merged to main.
---

# ship

Merge the current working changes to `main` through a clean branch → PR → merge flow. **No** version bump, tag, GitHub release, or binaries — that is the `release` skill's job.

## When to use

- Test additions, docs, refactors, internal fixes with no user-facing binary change.
- Anything the user describes as "ship" rather than "release".

If the change is user-facing and should produce new downloadable binaries, use the **release** skill instead. If unsure which the user wants, ask.

## Binaries

Ship publishes none. `.github/workflows/release.yml` only builds on a `vX.Y.Z` tag push or a manual run, and neither happens here.

**If you need a Windows `.exe` to test a shipped change**, trigger the workflow by hand — it builds Linux and Windows artifacts and publishes nothing (its `publish` job is gated on `refs/tags/`):

```bash
gh workflow run release.yml --ref main
gh run watch
gh run download <run-id> -D ./ci-artifacts
```

Note this only works once `release.yml` is on the **default branch** — GitHub
doesn't offer manual runs for a workflow that lives only on a feature branch.

## Environment notes (this repo)

- Repo root: `/home/mikael/git/winrmpc`. Shell is **bash** — use `git commit -F -` with a heredoc for multi-line messages.
  - *If running this on Windows/PowerShell instead*: multiline strings are `@'...'@` here-strings, and `gh pr create` chokes on inline multiline markdown — write the body to a temp file and use `--body-file`.
- `gh` CLI must be authenticated (`gh auth status`).
- Default branch is `main`.

## Steps

1. **Review what's changing.** `git status` and `git diff --stat` to confirm the scope. If the branch already holds a run of finished commits, ship those as they are rather than squashing history into one.

2. **Pick a branch name** from the change type — `tests/<topic>`, `docs/<topic>`, `fix/<topic>`, `refactor/<topic>`, `improve/<topic>`:
   ```bash
   git checkout -b <branch>
   ```

3. **Verify before committing.** When code changed:
   ```bash
   cargo test
   cargo check --all-targets    # zero warnings is the standard here
   ```

4. **Stage and commit.** Stage the specific files involved rather than a blanket `git add -A` unless that is genuinely the scope. Explain the *why* in the body:
   ```bash
   git commit -F - <<'EOF'
   <short summary line>

   <what changed and why it needed changing>

   Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
   EOF
   ```

5. **Push the branch.**
   ```bash
   git push -u origin <branch>
   ```

6. **Open the PR.** Body should carry a `## Summary` and, for code changes, a short `## Test plan`:
   ```bash
   gh pr create --title "<title>" --body-file pr_body.md && rm pr_body.md
   ```

7. **Merge, delete the branch, sync main.**
   ```bash
   gh pr merge <number> --merge --delete-branch
   git checkout main && git pull
   ```

8. **Report** the merged PR URL and confirm local `main` is up to date.

## Pitfalls

- If a commit hook fails, **never amend** — fix the issue and create a new commit.
- Don't bump the version here. A version bump with no tag or release leaves `Cargo.toml` claiming a release that doesn't exist.
- `docs/status.md` is the session-state file; if this ship changes where the branch stands, update it as part of the same PR rather than leaving it stale.
