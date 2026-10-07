# Physical input validation

The automated native replay verifies delivered pressure, tilt, button and timing data, persistence, and UI routing. It does **not** establish physical tablet reliability, palm rejection, or pen-to-screen latency. Those remain unverified until a physical trial is recorded. Existing Intuos BT S/GNOME Bluetooth incidents are documented separately; a workaround is not a passing stability result.

## Inspect input in Folio

Use a throwaway notebook. Open Help → **Start input check**, then draw normally. The nonmodal banner shows delivered frame counts, contact pressure range, maximum observed tilt, eraser frames and pad events. **Stop check** freezes the aggregates. **Save input report…** saves a local JSON snapshot. Reset or Resume starts a new trial. Closing the check discards its in-memory aggregates. Ordinary input does not collect these aggregates.

The report includes contact-motion interval percentiles, observed buttons, invalid frames, and the latest delivered event-to-CPU-canvas-paint timing. Timing sample vectors are capped at 100,000 each. Multiple events before one paint are coalesced to the latest event. This CPU measure excludes tablet transport, GPU presentation, compositor scheduling and display scanout. A zero frame count means no rich tablet frames were delivered during the trial; mouse input does not count as tablet evidence. Counts and ranges alone never imply a pass.

For raw, opt-in recording, launch Folio with a **new** output path:

```sh
FOLIO_PEN_RECORD=/tmp/folio-trial-unique.jsonl FOLIO_PROFILE_INK=1 \
  folio --data-dir /tmp/folio-input-trial
python3 scripts/analyze-input-check.py --recording /tmp/folio-trial-unique.jsonl
```

Quit normally before analyzing so the final CPU timing summary is written. The bounded queue drops recording samples if the writer cannot keep up; the summary includes `dropped_recordings`. Preserve the raw file and saved GUI report with the trial. Raw positions can reveal writing patterns; files stay local unless you choose to share them.

## Record the environment

The procedure is tablet agnostic. Run it for the devices/transports/desktops you support; mark features absent from a device as not applicable. No tablet was connected during development of these diagnostics.

For each trial record the Folio commit/build, package type, distribution, kernel, compositor/desktop and version, native Wayland or X11/XWayland session, GPU/driver, display refresh rate, scale, tablet model/firmware, pen, USB or Bluetooth transport and battery state. Record whether optional tablet compatibility settings were active. Use a unique trial ID in reports, video and notes. Include a USB baseline before testing Bluetooth when possible.

## Physical checks

1. Map the tablet across the chosen monitor. Mark corners and center at 100% and fractional scale. Check offset and rotation at several app zoom levels, after moving/resizing the window, and across monitors. Save a screenshot of the targets and observed marks.
2. Draw at low, medium and high pressure, slowly and quickly. Check continuity and width response; then tilt in each direction. Compare physical behavior to the report's ranges. A pen without tilt/eraser support should be marked not applicable.
3. Test hover, tip down/up, barrel buttons, pen eraser and every configured pad action. Lift the pen, leave proximity, disconnect/reconnect and sleep/wake. Check for ghost strokes, stuck tools and unexpected drawing through dialogs or toolbars. Exercise undo after each interruption.
4. Rest a palm while writing and navigating. Document unintended marks or gestures and the compositor's touch/palm settings. Folio's tablet-frame inspection cannot certify palm rejection; test the complete touchscreen/device/desktop combination.
5. Try native Wayland and X11/XWayland where supported, on GNOME and another relevant desktop. Repeat multi-monitor and fractional-scale checks in each environment. Do not reuse a passing result from another transport/session.
6. Run a 10-minute mixed writing/navigation trial, then a one-hour trial for each supported wireless transport. Where relevant, include the previously reported Intuos BT S/GNOME configuration. Include idle/reconnect cycles, monitor compositor/app logs, and reopen the notebook to check saved stroke count and raw data. Record disconnects, crashes and recovery behavior, including failures. Do not change global power, Bluetooth or desktop settings just to obtain a pass without reporting the change.

For each row, retain **pass**, **fail**, **not applicable**, or **not run**, observed evidence, and the exact configuration. Any required not-run row leaves the configuration unvalidated. The application report does not auto-assign these outcomes.

## Measure physical latency with a camera

Film the pen tip contacting the tablet/screen and the first visible ink in the same high-speed recording (240 fps or better if available). Include the whole event, stable focus, known actual frame rate and the display. For a non-screen tablet, both tablet contact and the monitor must be visible. Avoid variable-frame-rate footage unless timestamps have been normalized correctly; the helper below assumes a constant frame rate.

Capture at least ten separate strokes at several screen locations and pressure levels, at the start and end of the stability trial. Annotate the first frame with physical contact and the first frame with newly visible ink. Save integer indices in a CSV:

```csv
contact_frame,ink_frame
```

Populate the rows from the actual video, then run:

```sh
python3 scripts/analyze-input-check.py --recording saved-input-report.json \
  --camera-csv annotated-trial.csv --camera-fps 240
```

The camera result is reported separately as contact-to-visible-ink latency. Retain video, annotations, FPS source and sample count alongside median/p95/p99. Each 240 fps frame is about 4.17 ms; ambiguous contact or ink frames add annotation uncertainty. CPU timing and camera timing must not be substituted for each other. The parser's synthetic unit tests exercise units and input validation only.

## Current evidence

No new physical trial is claimed by this change. This repository provides the inspector, bounded recordings, parser, synthetic tests and protocol. Run the physical matrix and retain its artifacts before claiming a latency target, Bluetooth stability or palm-rejection support.
