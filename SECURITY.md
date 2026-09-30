# Security Policy

## Threat model

OmaKeys reads every key press and mouse click on the machine. Treat the plugin
as sensitive by default:

- The daemon reads `/dev/input/event*` globally, with no per-window scoping.
- Keystroke history is written to
  `~/.local/state/omarchy/current/plugins/skvggor.omakeys/keys.json` (mode `0600`).
  Anything running as your user can read it.
- Enable **Hide characters** in the panel to mask typed letters, digits and
  symbols before they reach the overlay or the history list.
- Use `omarchy plugin remove skvggor.omakeys --yes` to stop capture, and
  `./bin/omarchy-install-omakeys --uninstall` to remove the setgid binary.
- The daemon never talks to the network and never runs as root. It runs as your
  user with a single `setgid input` bit so it can read input devices without a
  re-login.

## Reporting a vulnerability

Please report security issues privately through GitHub's "Report a
vulnerability" button on the Security tab of
<https://github.com/skvggor/omakeys-omarchy-plugin>.

Include the affected version, the steps to reproduce, and the impact. Expect an
acknowledgement within a few days. Fixes ship as a new tagged release, and the
`release.yml` workflow publishes the daemon with a Sigstore build attestation
that the installer verifies before applying `setgid input`.

## Verifying a release

No extra tooling is needed: `./bin/omarchy-install-omakeys --install` pins the
release SHA-256 in version control and verifies the Sigstore build attestation
locally, against the Fulcio CA chain pinned in `bin/sigstore-fulcio-chain.pem`,
before it asks for `sudo`. It needs only `curl`, `jq` and `openssl` — no GitHub
account and no `gh`. If any part of the proof is missing it fails closed and
installs nothing.

`tests/installer.test.js` drives that same path against a throwaway CA, so the
verification is covered by tests rather than by manual auditing.

The `setgid input` bit is applied by a single privileged process that copies the
verified asset into a private root-owned directory, checks the digest of that
copy and installs from it. Anything running as your user can rewrite the
download, but never the bytes that gain the bit, so a substituted daemon is
rejected before an executable one exists.

That privileged step exists exactly once, and it has one caller. The digest it
verifies against always comes from version control. It is never read from the
file being installed: the file sits in a directory you own, so a digest derived
from it is supplied by the same user who could replace it, and the check would
approve whatever was there. A locally built daemon has no attestation and no
provenance tying it to the reviewed source, so there is no value to pin it
against, and no flag installs one with the bit. This is why there is no
`--build-install`: it existed, it derived its expected digest from
`target/release/omakeys-daemon`, and that made the verification circular. The
route for developers is `--build` (no root) plus `--add-input-group`, which runs
only `usermod` and costs one re-login instead.

`--add-input-group` is the offline fallback when the release cannot be fetched.
It grants the same `/dev/input` access by group membership rather than by a file
bit. Group membership reaches only new sessions, which is the trade for a path
that asks root to run no code at all.

To re-check an already downloaded asset by hand, follow
`verify_attestation()` in `bin/omarchy-install-omakeys`. `gh attestation
verify --signer-workflow` does the same thing with the Sigstore Go client, if
you already have `gh` authenticated — but it is optional, not a requirement.

## Out of scope

- Reports about the plugin capturing keys it is documented to capture.
- Vulnerabilities in upstream `evdev`, `xkbcommon` or Hyprland.
- The lack of a per-window input scope; that is a documented design choice.
