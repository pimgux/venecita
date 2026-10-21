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
drag-hint = Drag across empty cells to pick a zone. Drag a rule to move it (also onto another display), or its edges to resize it.
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

# --- Actions (shortcuts) ---
dir-left = left
dir-right = right
dir-up = up
dir-down = down
edge-left = left edge
edge-right = right edge
edge-up = top edge
edge-down = bottom edge
target-next = next
target-prev = previous

action-move_zone = Move { $dir }
action-move_zone-desc = Moves the focused window one cell { $dir }, keeping its size.
action-grow = Grow ({ $edge })
action-grow-desc = Pushes the window's { $edge } out by one cell.
action-shrink = Shrink ({ $edge })
action-shrink-desc = Pulls the window's { $edge } in by one cell.
action-move_to_display = Move to the { $target } display
action-move_to_display-desc = Sends the focused window to the { $target } display, keeping its zone.
action-snap_to_grid = Snap to grid
action-snap_to_grid-desc = Fits the window to the nearest zone, without remembering it.
action-pin_here = Pin here
action-pin_here-desc = Fits the window and remembers that zone for its app.
action-stack_next = Next window in the zone
action-stack_next-desc = Shows the next window sharing the focused window's zone.
action-stack_prev = Previous window in the zone
action-stack_prev-desc = Shows the previous window sharing the focused window's zone.
action-dynamic_next = Next in the dynamic zone
action-dynamic_next-desc = Shows the next window of the dynamic zone.
action-dynamic_prev = Previous in the dynamic zone
action-dynamic_prev-desc = Shows the previous window of the dynamic zone.
action-focus_window = Focus a window
action-focus_window-desc = Brings a specific window to the front.
action-focus_mode_toggle = Focus mode
action-focus_mode_toggle-desc = Enlarges the focused window and dims everything else. Press again to leave.
action-relayout_all = Arrange windows
action-relayout_all-desc = Puts every window back in its place.
action-activate_profile = Profile “{ $name }”
action-activate_profile-desc = Switches to this profile (automatic choice turns off).
action-next_profile = Next profile
action-next_profile-desc = Moves to the next profile that fits the connected displays.
action-auto_profile = Automatic profile
action-auto_profile-desc = Goes back to choosing the profile by the connected displays.
action-toggle_pause = Pause / resume
action-toggle_pause-desc = Stops or resumes arranging windows.
action-open_settings = Open settings
action-open_settings-desc = Opens this window.

# --- Redesign ---
general-grid-desc = Rows × columns every display is divided into, unless it has its own.
general-behavior = Behavior
general-auto-arrange = Arrange automatically
general-auto-arrange-desc = Puts a window back in its zone as soon as you move it away and let go.
files-title = Files
open-config = Open configuration file
open-logs = Show logs
zone-title = Selected zone
zone-actions = Actions
add-app = Add app
rules-title = Rules of this layout
connected-hint = Click “Assign as…” to use a display in your layouts.
profile-actions = This profile
creating-rule = New rule: choose its apps, then Apply.
editing-rule = Editing the rule of { $apps }. Draw on free cells for a new one.
