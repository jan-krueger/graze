/// Keybindings for graze (Phase 8)
///
/// # Normal Mode
///
/// | Key       | Action                                      |
/// |-----------|---------------------------------------------|
/// | q         | Quit                                        |
/// | Ctrl-c    | Quit                                        |
/// | j / Down  | Move down one row                           |
/// | k / Up    | Move up one row                             |
/// | Ctrl-d    | Move down half page                         |
/// | Ctrl-u    | Move up half page                           |
/// | g         | Go to first row                             |
/// | G         | Go to last row                              |
/// | h / Left  | Scroll columns left                         |
/// | l / Right | Scroll columns right                        |
/// | 0         | Go to first column                          |
/// | s         | Cycle sort on current column                |
/// | S         | Open stats overlay (SUMMARIZE)              |
/// | /         | Open search bar (plain text)                |
/// | $         | Open regex search bar                       |
/// | f         | Open filter bar pre-filled with column      |
/// | e         | Enter SQL scratchpad mode                   |
/// | Esc       | Clear filter and search                     |
/// | n         | Jump to next search match (full dataset)     |
/// | N         | Jump to previous search match (full dataset) |
/// | D         | Open diff mode (requires 2+ tabs)            |
/// | ]         | Switch to next tab                           |
/// | [         | Switch to previous tab                       |
///
/// # Search Mode (`/`)
///
/// Plain text substring search (case-insensitive). Yellow badge.
///
/// | Key       | Action                                     |
/// |-----------|--------------------------------------------|
/// | Enter     | Apply search                               |
/// | Esc       | Cancel                                     |
/// | Backspace | Delete last character                       |
/// | Any char  | Append to input                             |
///
/// # Regex Mode (`$`)
///
/// Regex search via DuckDB `regexp_matches()` (case-insensitive).
/// Magenta badge.
///
/// | Key       | Action                                     |
/// |-----------|--------------------------------------------|
/// | Enter     | Apply regex search                         |
/// | Esc       | Cancel                                     |
/// | Backspace | Delete last character                       |
/// | Any char  | Append to input                             |
///
/// # Filter Mode (`f`)
///
/// SQL WHERE-clause filter with `$column` autocomplete. Green badge.
///
/// | Key       | Action                                     |
/// |-----------|--------------------------------------------|
/// | Enter     | Apply SQL filter                           |
/// | Esc       | Dismiss autocomplete, or cancel             |
/// | Tab       | Accept autocomplete suggestion              |
/// | Up/Down   | Navigate autocomplete suggestions           |
/// | Backspace | Delete last character                       |
/// | $         | Start column reference (triggers autocomplete) |
/// | Any char  | Append to input                             |
///
/// ## $ Column Shorthand
///
/// Type `$columnname` to reference a column. On Enter, it expands to
/// `"columnname"` (a double-quoted SQL identifier). For columns with
/// spaces, use `$"Column Name"` which expands to `"Column Name"`.
/// As you type after `$`, an autocomplete popup appears with matching
/// column names. Press Tab to accept the selected suggestion.
///
/// # SQL Mode
///
/// | Key        | Action                                |
/// |------------|---------------------------------------|
/// | F5         | Execute SQL query                     |
/// | Ctrl-e     | Execute SQL query                     |
/// | Ctrl-Enter | Execute SQL query (terminal-dependent)|
/// | Enter      | Insert newline (max 5 lines)          |
/// | Esc        | Exit SQL mode, restore original view  |
/// | Backspace  | Delete character before cursor        |
/// | Left/Right | Move cursor within/between lines      |
/// | Up/Down    | Move cursor between lines             |
/// | Any char   | Insert at cursor position             |
///
/// # Stats Mode
///
/// | Key       | Action                          |
/// |-----------|---------------------------------|
/// | j / Down  | Scroll down one row             |
/// | k / Up    | Scroll up one row               |
/// | g         | Scroll to top                   |
/// | G         | Scroll to bottom                |
/// | Ctrl-d    | Scroll down half page           |
/// | Ctrl-u    | Scroll up half page             |
/// | Esc       | Close stats overlay             |
///
/// # Diff Setup Key Mode (`D` step 1)
///
/// Select key columns for row matching via JOIN.
///
/// | Key       | Action                          |
/// |-----------|---------------------------------|
/// | j / Down  | Move cursor down                |
/// | k / Up    | Move cursor up                  |
/// | Space     | Toggle column selection          |
/// | Enter     | Confirm (at least 1 required)   |
/// | Esc       | Cancel                          |
///
/// # Diff Setup Cols Mode (`D` step 2)
///
/// Select which columns to compare for differences.
///
/// | Key       | Action                          |
/// |-----------|---------------------------------|
/// | j / Down  | Move cursor down                |
/// | k / Up    | Move cursor up                  |
/// | Space     | Toggle column selection          |
/// | Enter     | Confirm (at least 1 required)   |
/// | Esc       | Cancel                          |
///
/// # Diff Mode
///
/// Unified diff view comparing two tabs.
///
/// | Key       | Action                          |
/// |-----------|---------------------------------|
/// | j / Down  | Scroll down one row             |
/// | k / Up    | Scroll up one row               |
/// | g         | Scroll to top                   |
/// | G         | Scroll to bottom                |
/// | Ctrl-d    | Scroll down half page           |
/// | Ctrl-u    | Scroll up half page             |
/// | h / Left  | Scroll columns left             |
/// | l / Right | Scroll columns right            |
/// | n         | Jump to next diff row           |
/// | N         | Jump to previous diff row       |
/// | Esc       | Close diff view                 |
///
/// # Sort Cycling
///
/// Pressing `s` on a column cycles through:
/// - No sort -> Ascending (▲)
/// - Ascending -> Descending (▼)
/// - Descending -> No sort
///
/// # Stats Overlay
///
/// Pressing `S` (Shift-s) shows summary statistics via DuckDB's
/// SUMMARIZE command. The overlay displays in the bottom half of the
/// screen with columns: column_name, column_type, min, max,
/// approx_unique, avg, std, q25, q50, q75, count, null_percentage.
///
/// # Filter Highlighting
///
/// When a filter is active, cells that match the filter condition are
/// highlighted in yellow. This works for simple column comparisons like:
/// - `city = 'Boston'`
/// - `age > 30`
/// - `"column_name" LIKE '%pattern%'`
///
/// Complex expressions (AND, OR, subqueries) show the filter in the
/// status bar but do not highlight individual cells.
///
/// # Search Highlighting
///
/// When a text search is active (via `/`), any visible cell whose text
/// contains the search term (case-insensitive) is highlighted with
/// yellow foreground on black background. Regex searches (via `$`) use
/// the compiled regex for highlighting. Use `n` to jump to the next
/// matching cell and `N` for the previous one. `n`/`N` search the full
/// dataset via DuckDB and scroll to matching rows anywhere in the data.
/// Press `Esc` in Normal mode to clear the search.
///
/// `n`/`N` always navigate search matches regardless of filter state.
/// Use `j`/`k` or arrow keys to navigate rows when a filter is active.
#[allow(dead_code)]
pub const KEYBINDINGS_VERSION: u8 = 12;
