# UX fixes

Each numbered audit finding receives its own commit and push. Validation is recorded alongside the fix.

| Audit | Change | Validation |
| --- | --- | --- |
| 1 | Shared draft undo/redo with grouped typing, Unicode word navigation, and bounded history. Inline text changes continue to use durable document history after the field closes. | Field history and Unicode editing regressions. |
