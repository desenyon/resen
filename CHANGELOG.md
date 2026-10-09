# Changelog

## Unreleased

- Search and page through the full archive in the terminal and CLI, with exact
  status filtering, JSON results, stable ordering and unreadable-record diagnostics.
- Migrate old databases transactionally to schema 2; reject unknown future versions
  and recover every abandoned run without a 200-record limit.
- Preserve corrupt JSON and mismatched IDs while keeping healthy history available.
- Share worker supervision and checkpoint policy across CLI and TUI jobs; preserve
  queued output on cancellation and retry failed final saves without replacing data.
- Checkpoint stalled streams by elapsed time and text volume, independently of UI ticks.
- Reject empty normalized symbols and document archive migration, backups, querying,
  configuration and recovery limits. Add migration/lifecycle regressions and real
  process checks for stalled output and hard-kill recovery.

## 0.2.2

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

## 0.2.1 — unpublished candidate

All five native preflight builds passed. Package inspection then found a Windows
Visual C++ runtime DLL dependency. Version 0.2.2 links the runtime statically and
checks PE imports before packaging and during public installation QA.

## 0.2.0 — unpublished candidate

The Linux packaging gate rejected a shell condition under its older ShellCheck.
No release was published. Version 0.2.1 uses an explicit condition and adds native
preflight packaging without changing existing tags.

## 0.1.0

Initial source implementation: Ratatui workspace, model connections, evidence-led
research, local archive, historical studies, CSV import, and reviewed LEAN execution.
