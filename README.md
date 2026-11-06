# Emby for lemonfiber

**Plays your library on TVs, phones and browsers.** This is Emby Server as a
lemonfiber plugin: a media server the household watches on, and the identity
source its members sign in with, in place of the bundled Jellyfin.

Without it, files on disk, no way to watch them.

## What it runs

Two services, declared in [`plugin.toml`](plugin.toml).

| Service | Image | What it is |
| --- | --- | --- |
| `emby` | `docker.io/emby/embyserver`, `4.10.2.0`, by digest | Emby Server itself. It answers other services in its own API, which the manifest declares as `native = "emby"`, on port 8096 inside the stack's network. |
| `emby-adapter` | `ghcr.io/lemonfiber/emby-adapter`, by digest | The adapter. It speaks lemonfiber's `media.serve@1` and `identity.source@1` contracts on port 8080 and asks `emby` for every answer. |

Neither service publishes a port to the household. `emby` publishes 8097 on this
machine's loopback, and lemonfiber reaches the adapter on the loopback at a port
the engine assigns. The household reaches Emby through the stack's stream guard.

Emby's image drops to the ids it reads from `UID` and `GID`, so it runs as the
operator and its files share an owner with the adapter beside it.

The plugin is never part of the default bundle. Emby has been closed source since
3.6, so the manifest declares its licence as `Proprietary`, and an operator
chooses to install it.

## What the adapter holds

- **The plugin's key**, which lemonfiber mints for this plugin alone and writes
  to `config/emby-adapter/lemonfiber.key`. The adapter answers nothing without
  it, except the guard's question about an item, which carries a member's own
  token and is answered by asking Emby with that token.
- **Emby's administrator credential**, which lemonfiber writes to
  `config/emby-adapter/upstream.json` as `{"username", "password"}`, from the
  password it minted for this server. The adapter reads it for every sign-in,
  never writes it, and never puts any of it in a log or an answer.

Both files are owner-only. lemonfiber runs the adapter as the operator's uid, so
the adapter can read them; the image's own user cannot. The image is distroless,
non-root, with no shell and no package manager.

## How Emby is set up

The first run goes over `identity.source`: the adapter names the first account
and gives it the administrator's password lemonfiber minted, turns Emby's
automatic port mapping off, completes the wizard, and hides the account from
Emby's sign-in screen. Every account it invites is hidden too, so Emby names no
account to a device that asks who is on it.

Emby holds a parental rating on one scale of its own, the same in every country,
rather than as an age. The adapter reads each level as the age the British
certificates Emby files at it are for, and writes an age as the highest level
read at or below it.

What Emby has no setting for, the adapter answers as unsupported rather than
pretending: Emby holds no list of addresses trusted to name a client and no list
of origins a browser may read it from, no outside service holds its accounts, and
it signs no device in by a code.

## What it needs

- A lemonfiber that installs plugins whose services speak contracts.
- Disk space for both images, and for Emby's own data in `config/emby`.
- Access to `docker.io` and `ghcr.io` the first time it starts.
- No accounts anywhere else.

## Install

```sh
git clone https://github.com/lemonfiber/plugin-emby
lemonfiber plugin install ./plugin-emby --dry-run
lemonfiber plugin install ./plugin-emby
```

`--dry-run` settles everything a real install settles and writes nothing.

## What is proved here

- **Conformance.** Every case of both contracts is recorded under
  [`conformance/`](conformance), answered by this repository's adapter in front of
  Emby at the pinned digest. `lemonfiber plugin conform` judges them without
  running anything. `lemonfiber plugin record .` takes them, and the `recordings`
  check takes them again on every pull request with the adapter built from the
  tree and compares.
- **Proofs.** Three `[[proof]]` entries in the manifest hold the pinned Emby to
  answering which server it is to anybody, and to refusing its account list and
  its catalogue to anybody who presents nothing. Their recordings are in
  [`fixtures/`](fixtures).
- **The adapter.** The crate's tests drive every operation against a fake Emby,
  run the declaration through a guard that enforces it against the adapter's own
  answers, and CI holds the crate to the core's lints and to every reachable line
  being reached.

## Licence

Emby Server is proprietary software of Emby LLC. This repository is under the
licence in [`LICENSE`](LICENSE).
