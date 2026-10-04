# Publishing Resen

Releases are native builds of the committed source. Do not upload unrelated
local `dist/` archives: their recorded revision may predate the tag.

1. Update `Cargo.toml`, the matching `Cargo.lock` package version, `CHANGELOG.md`
   and `docs/releases/vVERSION.md`. Keep installation examples current.
2. Run formatting, Clippy, Rust tests, optimized build and `make qa`.
   Push focused commits and wait for all `Verify` jobs to pass. Dispatch
   `Release artifacts` on main for a native packaging preflight; it exercises
   all five platforms without publishing a draft or changing any tag.
3. Create and push the matching annotated tag:

   ```sh
   git tag -a v0.2.1 -m "Resen v0.2.1"
   git push origin v0.2.1
   ```

4. `Release artifacts` checks the tag/version agreement and builds five native
   packages: Linux x86_64/ARM64 musl, macOS Intel/Apple Silicon and Windows x86_64.
   Every job runs formatting, Clippy, tests and the optimized build. Unix jobs
   exercise installer failure paths, actual executable PTY interactions and
   subprocess cancellation. Linux jobs also execute on Alpine and Debian.
5. The collector rejects missing/duplicate assets or metadata with a wrong
   version, target, revision or binary hash. It creates `SHA256SUMS` for all five
   archives plus `install.sh`, then uploads a **draft** GitHub release.
   Review the successful jobs, notes and seven uploaded assets before publishing:

   ```sh
   gh release view v0.2.1 --repo desenyon/resen
   gh release edit v0.2.1 --repo desenyon/resen --draft=false --latest --verify-tag
   ```

6. `Verify public installation` downloads public assets on the same five native
   platforms without release credentials. It verifies checksums, tag metadata,
   installed bytes, version/help and an offline study. Unix jobs run the real
   downloaded binary through both terminal/process suites. If needed, dispatch
   it manually with the published tag:

   ```sh
   gh workflow run install.yml --repo desenyon/resen -f version=v0.2.1
   ```

7. Verify the unauthenticated `releases/latest/download/install.sh` URL and run
   the README command in a clean installation directory. Check the public
   workflow, remote main/tag, release visibility and working tree before
   reporting completion.

Re-running the installer is idempotent. Failures before the final atomic rename
leave the existing binary intact; application state is not touched by upgrades.
Release checks prove the packaged application and offline workflow, not access
to every user's model account, vendor keys or LEAN/data entitlements. macOS
distribution is currently unsigned and unnotarized.
