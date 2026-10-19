# Venecita Dock — settings window (English)
# Every message id here must also exist in es/settings.ftl (checked by a test).

window-title = Venecita Dock Settings

# Shared actions
save = Save
cancel = Cancel
close = Close
add = Add
remove = Remove
delete = Delete
rename = Rename
duplicate = Duplicate
edit = Edit

unsaved-changes = You have unsaved changes.
save-failed = Could not save: { $error }

# Tabs
tab-layout = Layout
tab-displays = Displays
tab-profiles = Profiles
tab-hotkeys = Shortcuts
tab-general = General

# --- Layout editor ---
layout-editor-title = Layout editor
select-profile = Profile
select-layout = Layout
dynamic-zone = Dynamic zone
mark-dynamic = Mark selection as the dynamic zone
clear-dynamic = Clear dynamic zone
zone-apps = Applications in this zone
running-apps = Running applications
type-app = Type an app id or name
drag-hint = Drag across the grid to pick a zone.
no-selection = Select a zone on a display to add or edit a rule.
rule-count = { $count } rule(s)
new-rule = New rule
apply-rule = Apply

# --- Displays ---
displays-title = Displays
connected-displays = Connected displays
defined-displays = Defined in your configuration
identify = Identify
identify-hint = Briefly shows a number on each screen.
assign-as = Assign as…
display-name = Name
display-resolution = Resolution
display-builtin = Built-in
display-external = External
display-grid = Grid

# --- Profiles ---
profiles-title = Profiles
new-profile = New profile
profile-name = Profile name
layouts = Layouts
new-layout = New layout
layout-name = Layout name
layout-displays = Displays used
no-profiles = No profiles yet. Create one to get started.

# --- Hotkeys ---
hotkeys-title = Keyboard shortcuts
col-action = Action
col-shortcut = Shortcut
press-keys = Press keys…
conflict-warning = This shortcut is already used by another action.
reset-hotkeys = Reset to defaults

# --- General ---
general-title = General
general-language = Language
lang-auto = Automatic (follow the system)
lang-en = English
lang-es = Español
general-grid = Default grid
grid-cols = Columns
grid-rows = Rows
general-gap = Gap between windows
general-launch-at-login = Launch at login
general-ignore = Ignored applications
ignore-hint = These applications are never moved.

focus-title = Focus mode
focus-size = Focused window size
focus-dim = Dim opacity

dim-title = Dim the other windows
dim-enabled = Darken every window except the focused one
dim-opacity = How dark

api-title = Local API
api-enabled = Enable the local API
api-bind = Listen address
api-allow-lan = Allow connections from the local network
api-lan-warning = ⚠ Exposing the API to the network lets other devices control your windows. Keep the token secret.
api-token = Access token
token-show = Show
token-hide = Hide
token-copy = Copy
token-rotate = Rotate
token-rotated = A new token was generated.

# --- Window chrome ---
saved = Saved.
discard = Discard
problems = { $count } problem(s) to fix
load-failed = Could not read the configuration file
open-file = Open the file in a text editor
no-layouts = This profile has no layouts yet. Add one in the Profiles tab.
displays-matched-as = Defined as
display-unmatched = not defined
capture-hint = Click a shortcut, then press the new keys (Esc cancels). If the system or Venecita itself catches the keys, type the shortcut instead.
token-missing = No token yet.
