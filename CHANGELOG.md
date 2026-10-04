# Changelog

## 0.2.0

Resen's first downloadable release, including the complete terminal research desk
and the UI review fixes.

- Add a checksum-verified shell installer for macOS and Linux on x86_64 and ARM64.
  Upgrades replace the executable atomically after version and help checks.
  Existing research, settings and credentials are preserved.
- Publish portable static Linux archives, native macOS archives and a Windows ZIP,
  with SHA-256 checksums and source/version metadata.
- Keep setup, research and strategy controls visible in compact terminals.
- Add scrollable help and clear next actions in empty research/source/lab views.
- Improve compact archives, price context, evidence origins and study metrics.
- Keep running-job phase and cancellation visible on every workspace page.
- Repair cursor visibility, pasted searches and Unicode word deletion.
- Dismiss errors without discarding forms; reset asset selection after setup changes.
- Add 21 UI regressions and installer failure-path tests; exercise real executables,
  cancellation and terminal restoration.

Model-backed research and vendor/LEAN connections require the user's credentials,
local services and data entitlements. Demo fixtures remain explicitly synthetic.

## 0.1.0

Initial source implementation: Ratatui workspace, model connections, evidence-led
research, local archive, historical studies, CSV import, and reviewed LEAN execution.
