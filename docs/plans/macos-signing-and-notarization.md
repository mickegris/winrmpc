# Plan: sign and notarise the macOS bundle

Status: **planned** (targeting 0.4.5). Finishes the one thing
[`macos-app-bundle.md`](macos-app-bundle.md) explicitly deferred: *"Signing or
notarisation. Needs a paid account; revisit if one exists."* One now exists.

## What users hit today

The v0.4.4 `.app` is unsigned and un-notarised, so macOS refuses it on
double-click and calls it **damaged**. That word is the problem: it describes a
corrupt download, not a policy decision, so the honest reaction is to assume
the file is broken and give up. Our own workaround —

```
xattr -dr com.apple.quarantine /Applications/winrmpc.app
```

— asks someone to disable a security check by pasting a command they can't
evaluate, which is exactly the habit that makes people vulnerable elsewhere.
It's a bad thing to be teaching, and it is the *only* macOS install
instruction we currently give.

Signing removes all of it: the app opens by double-click, first time, with no
instructions at all.

## Signing and notarising are two different things, and we need both

- **Signing** attaches a *Developer ID Application* certificate. It proves who
  built the binary and that it hasn't been altered since.
- **Notarising** uploads the signed app to Apple, which scans it for malware
  and returns a ticket. Gatekeeper on the user's Mac checks for that ticket.

A signed-but-not-notarised app still gets blocked. Notarising additionally
requires the **hardened runtime** (`--options runtime`) and a **secure
timestamp** (`--timestamp`) — an app signed without either is rejected by the
notary service, not by Gatekeeper, so it fails in CI rather than on a user's
machine. That's the good failure mode.

**Stapling** (`xcrun stapler staple`) writes the ticket into the app so it
validates offline. Without it, a user opening the app while their Mac is
offline — or while Apple's OCSP responder is slow — can still be blocked. It
costs one command; skip it and the win is only mostly there.

## What has to exist before any of this runs

These are account actions in Apple's portals; nothing in this repo can create
them.

1. **A "Developer ID Application" certificate.** Not "Apple Development", not
   "Developer ID Installer" (that one signs `.pkg` installers, which we don't
   ship). Create it at developer.apple.com → Certificates, download the
   `.cer`, add it to Keychain Access, then **export it as a `.p12` with a
   password** — the `.cer` alone has no private key and cannot sign anything.
2. **The certificate's exact common name**, e.g.
   `Developer ID Application: Mikael Österdahl (TEAMID123)`. `security
   find-identity -v -p codesigning` prints it.
3. **The Team ID** — ten characters, shown in the Membership page and in the
   parenthesis above.
4. **An App Store Connect API key** (Issuer ID + Key ID + `.p8`), *or* an
   app-specific password for the Apple ID. The API key is the better choice:
   it's scoped, revocable on its own, and doesn't break when the account's
   password or 2FA changes. `notarytool` accepts either.

Then five GitHub repository secrets:

| Secret | Contents |
|---|---|
| `MACOS_CERT_P12` | the `.p12`, base64-encoded (`base64 -i cert.p12 \| pbcopy`) |
| `MACOS_CERT_PWD` | the password used on export |
| `MACOS_CERT_NAME` | the full common name from step 2 |
| `MACOS_NOTARY_KEY_ID` / `MACOS_NOTARY_ISSUER_ID` / `MACOS_NOTARY_KEY_P8` | the App Store Connect key, `.p8` base64-encoded |
| `MACOS_KEYCHAIN_PWD` | any random string; it only unlocks the throwaway CI keychain |

## The build order changes, and that part is easy to get wrong

`bundle.sh` currently ends by zipping the `.app`. Signing has to slot in
*before* that, and stapling *after* notarisation — which means the zip is made
**twice**:

1. `lipo` the universal binary, lay out the bundle. *(unchanged)*
2. `codesign` the `.app`.
3. `ditto` it to a zip — **for submission only**.
4. `xcrun notarytool submit … --wait`.
5. `xcrun stapler staple` the `.app`. This modifies the bundle.
6. `ditto` again — **this** zip is the release asset.

Getting 5 and 6 the wrong way round produces a zip that passes notarisation
and still isn't stapled, which then works on every machine that tested it
(they're online and the ticket is cached) and fails for someone else. Worth a
verification step rather than trust:

```bash
codesign --verify --deep --strict --verbose=2 dist/winrmpc.app
spctl -a -vvv -t install dist/winrmpc.app     # must say "accepted / Notarized Developer ID"
xcrun stapler validate dist/winrmpc.app
```

`spctl` is the one that actually answers "will a user's Mac open this", so it
is the acceptance check, not `codesign --verify`.

## Entitlements

**Start with none.** winrmpc is a single statically-linked Rust executable with
no plugins, no JIT and no nested frameworks; it links Metal and the other
system frameworks, which are already signed by Apple. The entitlements that
usually get added to Rust/Electron apps —
`com.apple.security.cs.disable-library-validation`,
`…allow-unsigned-executable-memory` — are workarounds for loading code the app
didn't sign, and adding them "just in case" weakens the hardened runtime for
no benefit.

If something does break under the hardened runtime it will show up as a crash
on launch, not a notarisation failure, so the acceptance check has to include
**actually opening the signed app on a Mac**.

Note that this is *not* App Sandbox and we are not adding it: sandboxing a
client that reads a user-chosen MPD server over the LAN would need entitlements
and would gain nothing outside the App Store.

## Signing must not become a release blocker

CI runs on tags. If a certificate expires (Developer ID certs last five years,
and they expire quietly) or a secret is rotated wrong, the macOS job fails and
the whole release stops — including the Linux and Windows binaries, which have
nothing to do with it.

So: **`bundle.sh` signs only when the credentials are present**, and prints a
loud warning and produces the current unsigned bundle when they aren't. That
keeps `./packaging/macos/bundle.sh` working on a developer Mac with no
certificate, keeps fork PRs building (secrets aren't exposed to forks at all),
and turns an expired certificate into a degraded release rather than no
release. The workflow should still *report* which it produced, so an
accidentally unsigned release is visible in the run log rather than discovered
by a user.

## Documentation that has to change with it

Four places currently tell people to strip the quarantine attribute, and all
four become wrong — worse, they'd keep teaching the habit after it was
unnecessary:

- `README.md` (the macOS download section)
- `packaging/macos/bundle.sh`'s closing note
- `docs/plans/macos-app-bundle.md` § "Gatekeeper"
- the release-notes template in `.claude/skills/release/SKILL.md`, which
  instructs every future release to repeat the warning

## Steps

- **A.** Account side: certificate, `.p12`, API key, five GitHub secrets. *(user)*
- **B.** `bundle.sh`: optional `codesign`/`notarytool`/`stapler` stage, the
  two-zip ordering, and the verification commands.
- **C.** `release.yml`: import the `.p12` into a temporary keychain, write the
  `.p8`, pass the credentials through, and log signed-vs-unsigned.
- **D.** Strip the quarantine instructions from all four places above.
- **E.** Acceptance: download the release asset on a real Mac, unzip, and
  double-click. It must open with **no** dialog beyond the ordinary
  "downloaded from the internet" confirmation, and `spctl -a -vvv` must say
  *Notarized Developer ID*.

## Not doing

- **A `.dmg`.** A zip that opens on double-click is enough; a dmg needs
  `create-dmg` or `hdiutil` scripting and a background-image design pass.
- **A Homebrew cask.** Wants a signed app first — which this gives it — but
  also a stable release cadence and a maintained formula.
- **Signing the Linux or Windows binaries.** Windows Authenticode is a separate
  certificate, a separate cost, and a separate plan.
