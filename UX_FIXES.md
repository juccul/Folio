# UX fixes

Each numbered audit finding receives its own commit and push. Validation is recorded alongside the fix.

| Audit | Change | Validation |
| --- | --- | --- |
| 1 | Shared draft undo/redo with grouped typing, Unicode word navigation, and bounded history. Inline text changes continue to use durable document history after the field closes. | Field history and Unicode editing regressions. |
| 2 | Wrapped multiline drafts, wheel scrolling, horizontal overflow scrolling, and caret/IME positioning that follows the edited text. | Six field regressions passed, including scroll clamping after deletion. |
| 3 | Nonblocking page preview errors with retry; persistent library/editor save warning with Retry saving across affected documents. | Missing-image and save-retry regressions passed; UI build checked. |
| 4 | Equation dialogs retain drafts and inline errors while rendering; Apply waits for completion; cancelled or superseded renders cannot insert later. | Invalid LaTeX, cancellation, and successful retry regression passed; UI build checked. |
| 5 | Controller-wide Trash write protection, read-only editor chrome, Restore/View Trash actions, and usable panning. | Content, page, metadata, history, and equation mutation regression passed; UI build checked. |
| 6 | Provisional library imports stay out of storage until content succeeds or the user writes; pending/failed states offer retry and removal while retaining annotations. | All 22 management regressions passed, including failure, storage, retry, and annotation preservation; UI build checked. |
| 7 | Durable stale-calculation errors, visible canvas markers, updating state, and contextual retry; successful updates clear stale state. | Failed answer retained across reopen, explicit retry, and successful recovery regression passed; UI build checked. |
| 8 | Independent eraser size; explicit Whole stroke, Ink segments, and Whole object modes; ink modes protect media; legacy segment preference migrates. | Four extended editing regressions and three settings regressions passed; UI build checked. |
| 9 | Explicit search states and local errors, query-associated results, accurate page numbers/counts, and result-limit disclosure. | Latest-query, stale-result clearing, empty input, and local failure regression passed; UI build checked. |
| 10 | Changed handwriting marks search annotations stale while preserving reviewed drafts; page notice offers review/re-recognition; undo and reopen retain the correct state. | Nearby ink, review, undo/redo, source removal, validation, and reopen regression passed; UI build checked. |
| 11 | Stable parent/child folder tree, depth-aware indentation, persisted collapsible branches, complete path labels, and navigable breadcrumbs. | Shuffled hierarchy, sibling order, depth, and full-path regression passed; UI build checked. |
| 12 | Shared ID-backed folder destination picker with full paths, filtering, a root destination, selected destination state, and disabled invalid folder moves. | Identity, duplicate names in different paths, descendant rejection, and root move regressions passed; UI build checked. |
| 13 | Shared trimmed folder-name validation and case-insensitive sibling uniqueness for create, rename, and move; inline errors retain drafts. | Blank names, normalization, sibling conflicts, different-parent names, failed mutations, and reopen regressions passed; UI build checked. |
| 6 (follow-up) | Autosave toggles cannot persist untouched import placeholders; removing a failed import remains safe when the previous document needs asynchronous loading. | Import recovery regression expanded and passed. |
| 14 | Recent uses a persisted, bounded opening history; browsing an older document moves it to the top without changing its edit timestamp or undo history. | Open/edit distinction, ordering, restart, and Trash exclusion regression passed; UI build checked. |
| 15 | New document and Ctrl+N open a writable page immediately; optional setup is separate, names are optional there, paper choices are remembered, and UI terminology is consistent. | All 25 management regressions passed, including remembered settings/restart and malformed-default recovery; UI build checked. |
