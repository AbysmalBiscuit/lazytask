# Cutting a release

LazyTask uses a single **tag-driven release pipeline**: you push a `v*`
tag, GitHub Actions builds binaries for every supported platform, and a
GitHub Release is created automatically with the assets attached.

## TL;DR

```bash
# 1. bump the version in Cargo.toml + Cargo.lock
$EDITOR Cargo.toml                # change `version = "0.1.0"` to "0.2.0"
cargo check                       # refreshes Cargo.lock with the new version
git add Cargo.toml Cargo.lock
git commit -m "release: v0.2.0"

# 2. tag and push
git tag v0.2.0
git push origin master
git push origin v0.2.0
```

That's it. About 10 minutes later the
[Releases page](https://github.com/osamamahmood/lazytask/releases)
will have a new `v0.2.0` release with seven downloadable binaries
attached.

## What happens automatically

The `Build` workflow ([.github/workflows/build.yml](../.github/workflows/build.yml))
fires on any push to a `v*` tag. For each entry in its build matrix it:

1. Checks out the tagged commit.
2. Installs the stable Rust toolchain + the matrix target.
3. Runs `cargo build --release --target=<matrix.target>`.
4. Compresses the binary with UPX (skipped on macOS — UPX doesn't support
   Mach-O reliably).
5. Tarballs the binary as `lazytask-<target>.tar.gz` and writes a
   `lazytask-<target>.sha256` next to it.
6. Uploads both files as workflow artifacts.
7. **On a tag ref**, also uploads them to the GitHub Release for that tag,
   creating the Release on the first matrix job and appending assets on
   the rest. GitHub auto-fills the release body from commit messages
   between this tag and the previous one.

### Targets currently built

| Platform | Target triple | Runner |
|---|---|---|
| macOS Intel | `x86_64-apple-darwin` | `macos-13` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `macos-14` |
| Linux x86_64 (glibc) | `x86_64-unknown-linux-gnu` | `ubuntu-latest` |
| Linux x86_64 (musl, static) | `x86_64-unknown-linux-musl` | `ubuntu-latest` |
| Windows x86_64 (GNU) | `x86_64-pc-windows-gnu` | `windows-latest` |
| Windows x86_64 (MSVC) | `x86_64-pc-windows-msvc` | `windows-latest` |
| Windows i686 (32-bit) | `i686-pc-windows-msvc` | `windows-latest` |

Adding more targets (e.g. `aarch64-unknown-linux-gnu`) is a one-block
addition to the `matrix.include` list.

## Versioning

LazyTask follows [SemVer](https://semver.org/): `MAJOR.MINOR.PATCH`.

- **PATCH** (`v0.1.0 → v0.1.1`) — bug fixes, clippy cleanups, doc-only
  changes.
- **MINOR** (`v0.1.0 → v0.2.0`) — new features that don't break the user
  experience or config schema.
- **MAJOR** (`v0.x.y → v1.0.0`) — breaking config changes, incompatible
  data-dir layouts, or first stable promise.

Tags with a hyphen (`v0.2.0-alpha.1`, `v0.2.0-rc.1`) are auto-marked as
**pre-releases** on GitHub by the `prerelease: ${{ contains(github.ref, '-') }}`
condition in `build.yml`.

## Pre-release checklist

Before tagging, run locally:

```bash
# 1. Working tree clean
git status

# 2. Tests green
cargo test

# 3. Format clean (CI uses nightly rustfmt)
cargo +nightly fmt --all -- --check

# 4. No clippy regressions
cargo clippy --all-targets -- -W warnings

# 5. Release build still produces a working binary
cargo build --release
./target/release/lazytask --version
```

If any of those are red, fix them before tagging — pushing a tag against
broken code wastes a 10-minute CI cycle and leaves a half-finished
release that you have to delete manually.

## Manually triggering a release build

If you need to re-run the build for an existing tag (e.g. you edited
`build.yml` itself and want to re-attach binaries):

1. Go to **Actions → Build → Run workflow**
2. Pick the tag from the dropdown
3. Click **Run workflow**

The `workflow_dispatch:` trigger in `build.yml` enables this.

## Crates.io (optional, future)

Publishing to [crates.io](https://crates.io) would let users install with
`cargo install lazytask`. Not currently set up. To enable:

```bash
# One-time:
# 1. Sign in at https://crates.io and create an API token
# 2. Add it as the `CRATES_IO_TOKEN` repo secret on GitHub
# 3. Add a `cargo publish` step in build.yml gated on `startsWith(github.ref, 'refs/tags/')`

# Each release:
cargo publish --dry-run    # locally first
cargo publish              # actually push
```

The dry-run is important because crates.io publishes are **immutable** —
you cannot edit or delete a published version, only yank it (which still
leaves the binary in the registry).

## Rolling back a release

If a release is broken:

1. **Don't delete the tag** — that doesn't actually remove the binaries
   from clients that already downloaded them, and breaks the auto-changelog
   for the next release.
2. **Mark the GitHub Release as a pre-release** (or delete only the
   Release, keeping the tag). On the [Releases page](https://github.com/osamamahmood/lazytask/releases),
   click the release → Edit → flip "Set as a pre-release" or delete.
3. **Fix forward**: bump the patch version (`v0.2.0 → v0.2.1`), tag, push.

## Future improvements

Things not yet wired up but worth considering once the project grows:

- **Homebrew tap** — would let macOS users `brew install lazytask`.
  Easiest path is `goreleaser` or a hand-written formula in a
  `homebrew-tap` repo updated by a workflow.
- **AUR package** — Arch Linux users expect this. Usually a community
  contributor will do it once the project is on crates.io.
- **A separate `release.yml`** that creates a draft Release first, lets
  you edit the notes before binaries arrive. The current pipeline is
  simpler (auto-publish) but less editorial control.
- **Changelog automation** — `git-cliff` or `release-please` for proper
  per-release CHANGELOG.md instead of relying on GitHub's auto-generated
  notes.
