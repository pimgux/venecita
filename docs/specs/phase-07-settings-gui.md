# Phase 7: Settings GUI (egui) and i18n

## Goal
Anyone can configure Venecita Dock without editing TOML by hand.

## Screens
1. **Layout editor**:
   - pick a profile and layout;
   - each display in the layout is drawn to scale with its grid;
   - drag across cells to select a zone, then choose the apps for it (from the running apps
     list, or typed by hand);
   - existing rules are drawn as labelled rectangles, which can be moved, resized or deleted;
   - mark one zone as the **dynamic zone**.
2. **Displays**: shows the connected displays next to the definitions, and has an
   "Identify" button that briefly shows a big number on each screen and an
   "Assign as …" button that creates a definition from a connected display.
3. **Profiles**: create, rename, duplicate and delete profiles and layouts.
4. **Hotkeys**: a table of actions with a key-capture field and conflict warnings.
5. **General**:
   - language (Auto, English or Español), grid size, gap and launch at login;
   - ignored apps;
   - focus size and dim opacity;
   - API: enabled, port, LAN toggle (with a warning), token show/copy/rotate.

## Design
- A UI-free **view-model** in `venecita-app::settings::model` edits a `Config`, validates it
  on each change, and saves it with `toml_edit` so comments survive.
- i18n uses Fluent (`.ftl` files in `assets/i18n/{en,es}/`), and the language is detected from the OS locale.
- The window opens from the tray ("Settings…") or with the `open_settings` action.

## Tests
- View-model: a cell drag produces the right `Zone`; adding, editing and deleting rules;
  validation messages; load → no change → save leaves the file byte-for-byte the same.
- Every Fluent key exists in both languages (a test compares the key sets).

## Manual checklist
- [ ] You can create the home profile from scratch with only the GUI.
- [ ] Switching the language updates the UI immediately.
