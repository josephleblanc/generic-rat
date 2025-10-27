Title: Folder Upload (U hotkey) — Root Cause and Fix

Summary
- Problem: Pressing `u` should let the user pick a local folder and preview its files in-memory, but selection was failing.
- Root cause: The directory picker (and `input.click()` fallback) must be invoked inside an active user gesture. Our code launched the pickers from an async task spawned after the key event returned, losing the user-gesture context.
- Fix: Trigger the picker synchronously inside the key event handler, then await the resulting Promise off the main event path to mount files in memory.

Root Cause Details
- Browsers require a user activation (a direct user gesture like a click or keypress) to open native pickers (e.g., `window.showDirectoryPicker()`), and often also for `input[type=file].click()`.
- In `src/lib.rs`, the key handler used `spawn_local(async move { event_state.handle_events(key_event).await; })` to process hotkeys. This means `handle_events` ran on the next microtask, after the event loop unwound, so calls to `showDirectoryPicker()` happened outside the original keypress gesture.
- Result: Browsers reject the picker with errors like “Must be handling a user gesture to show a file picker,” or the fallback `input.click()` silently does nothing.

What Changed
1) Handle `u` synchronously in the JS event callback
   - In `main()`, we now intercept `KeyCode::Char('u')` in the `on_key_event` closure and call a new function `start_pick_and_mount(state)` immediately, preserving the user gesture.
   - Other keys continue to be handled via the existing async `handle_events` pathway.

2) Promise-returning bindings for the pickers
   - Added Promise-returning externs for both pickers so they can be called synchronously and awaited later:
     - `fn pick_rust_crate_promise() -> js_sys::Promise;`
     - `fn pick_rust_crate_fallback_promise() -> js_sys::Promise;`
   - This ensures `showDirectoryPicker()` or `input.click()` fires within the gesture, while file reading and state updates happen asynchronously after selection.

3) Parsing and mounting results
   - Added `parse_js_file_array(js: JsValue) -> Result<Vec<FileEntry>, JsValue>` to convert the JS array of `{path, bytes}` records into Rust structs.
   - `start_pick_and_mount` awaits the Promise via `JsFuture`, parses results, writes them to an `InMemoryVfs`, and calls `rebuild_previews()` to update the UI.
   - Existing `gather_files()` and `mount_picked_crate()` continue to work; `gather_files()` now reuses the new parser.

Files Touched
- src/lib.rs
  - Intercept `u` key synchronously and call `start_pick_and_mount`.
  - Add Promise-returning externs for pickers and a parsing helper.
  - Factor parsing logic into `parse_js_file_array` and reuse it.

Why This Works
- The picker is invoked during the actual key event handler (a user gesture), satisfying browser requirements. The Promise resolution and virtual-file-system mounting happen later without blocking the event.

How To Test (locally)
- Run: `trunk serve` (localhost is a secure context; required for `showDirectoryPicker`).
- Press `u`:
  - On Chromium/Edge: Native directory picker opens via File System Access API.
  - On other browsers: Fallback `webkitdirectory` file input opens.
- Choose a folder with text files. The preview area updates to show filename and a short snippet. Status line shows “Loaded N files. Press E to export.”
- Press `e` to download a zip of the in-memory VFS.
- Press `l` to load `assets/sample.txt` from the server (unchanged behavior).

Notes and Limitations
- Cross-browser: `showDirectoryPicker` is Chromium-friendly; fallback uses `webkitdirectory` which works in Chromium and Safari; Firefox support varies and may need manual enablement. The fallback mitigates this.
- Hidden/binary filters: `index.html` filters hidden folders (e.g., `.git`) and common binary extensions to keep previews readable.
- Memory: All selected files load into memory. For large projects, consider size limits or progressive loading.
- Script placement: The script block resides after `</html>`. Browsers still execute it, but you may move it into `<body>` for strict HTML validity. Not required for this fix.

Suggested Follow-ups (optional)
- Add a visual button to trigger upload as an alternative to the `u` hotkey.
- Show per-file sizes and a total byte count in the status area.
- Add a cap on max file size and/or total bytes to avoid accidental huge loads.

Outcome
- Users can now press `u` to select a folder, and files are read into an in-memory VFS for use in the WebAssembly/Rust app. Previews render as intended, and export still works.

