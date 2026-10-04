# UI review and upgrade plan

Review date: 3 October 2026. Scope: the existing terminal research desk.

## Evidence and priorities

Reviewed all six pages and welcome, setup, palette and import at 60×18, 80×24
and 144×46 using the executable snapshot command. Reviewed input handling,
form layouts, scrolling and job state against the current UI tests.
Native Terminal access is denied by the computer-use tool; rendered buffers
and actual executable pseudo-terminal checks are the available UI evidence.
Code graph discovery did not respond within several minutes; source reads
were used after attempting the graph tools first.

| Priority | Observed problem | Change | Acceptance |
| --- | --- | --- | --- |
| P0 | Welcome actions disappear at 60×18; fixed-size forms compete for space | Pin actions and adapt dialog content to height; window strategy fields around focus | Setup/demo, submission and focused fields visible at every supported size |
| P0 | Help clips lower instructions and cannot scroll | Scrollable help with a fixed navigation hint | End reaches form instructions; Home returns to top |
| P1 | Long fields always scroll to their bottom even when editing the start | Scroll input viewport to the cursor | Start, middle and end cursors visible in long, Unicode and multiline values |
| P1 | Paste leaves stale palette/archive selections | Reset selection on changed query; retain palette on zero results | Paste then Enter selects matching result; zero results explain next action |
| P1 | Compact archive columns leave almost no question space | Two-line compact rows with assets/status and the question | Question readable at 60×18 and 80×24; demo provenance explicit |
| P1 | Short screens lose page/context hints and busy cancellation hint | Responsive header/footer, visible job phase | Current page and cancellation visible at supported sizes |
| P2 | Empty archive search looks like a never-used archive | Distinguish no matches and explain how to clear filter | Search failure points to recovery |

## Implementation sequence

1. Repair search/paste and input visibility; add focused regressions.
2. Repair compact dialogs and help navigation; assert actionable content.
3. Improve compact archive, navigation and job status; check wide layouts too.
4. Run formatting, all-target Clippy, all tests, optimized build, PTY QA and
   child-process QA. Render and inspect final compact and wide screens.
5. Commit and push focused changes; verify GitHub CI.

## Follow-up backlog

These require separate product/design work after the current correctness pass:

- Persistent research trace and limitations accessible beside compact memos.
  The Sources page retains evidence; a compact trace view would improve discovery.
- Better connection diagnostics per evidence vendor, beyond model connectivity.
  Live acceptance requires user keys and service entitlements.
- Vertical cursor movement in multiline questions, plus explicit draft/discard UX.
- Reader controls for source and memo search, section jumps and citation navigation.
- Theme/contrast and terminal-font checks on additional real terminal emulators.
- Dedicated cloud/LEAN log ownership and richer progress stages.

Do not label configured integrations as verified; do not fabricate live data,
model outputs or backtest statistics. Keep the existing slate/mint/gold design.

## Verification record

Pending implementation and fresh checks. Prior release evidence is in QA.md;
it is not evidence for these changes.
