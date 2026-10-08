# UX fixes

Each numbered audit finding receives its own commit and push. Validation is recorded alongside the fix.

| Audit | Change | Validation |
| --- | --- | --- |
| 1 | Shared draft undo/redo with grouped typing, Unicode word navigation, and bounded history. Inline text changes continue to use durable document history after the field closes. | Field history and Unicode editing regressions. |
| 2 | Wrapped multiline drafts, wheel scrolling, horizontal overflow scrolling, and caret/IME positioning that follows the edited text. | Six field regressions passed, including scroll clamping after deletion. |
