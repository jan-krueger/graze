/// Keybindings for graze (Phase 6)
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
/// | $         | Go to last column                           |
/// | s         | Cycle sort on current column                |
/// | S         | Open stats overlay (SUMMARIZE)              |
/// | /         | Enter filter mode                           |
/// | f         | Enter filter mode pre-filled with column    |
/// | ?         | Enter text search mode                      |
/// | e         | Enter SQL scratchpad mode                   |
/// | Esc       | Clear search (1st), then reset filter (2nd) |
/// | n         | Jump to next search match                   |
/// | N         | Jump to previous search match               |
///
/// # Filter Mode
///
/// | Key       | Action                                     |
/// |-----------|--------------------------------------------|
/// | Enter     | Expand $refs and apply filter               |
/// | Esc       | Dismiss autocomplete, or cancel filter      |
/// | Tab       | Accept autocomplete suggestion              |
/// | Up/Down   | Navigate autocomplete suggestions           |
/// | Backspace | Delete last character                       |
/// | $         | Start column reference (triggers autocomplete) |
/// | Any char  | Append to filter input                      |
///
/// ## $ Column Shorthand
///
/// Type `$columnname` to reference a column. On Enter, it expands to
/// `"columnname"` (a double-quoted SQL identifier). For columns with
/// spaces, use `$"Column Name"` which expands to `"Column Name"`.
/// As you type after `$`, an autocomplete popup appears with matching
/// column names. Press Tab to accept the selected suggestion.
///
/// # Search Mode
///
/// | Key       | Action                                     |
/// |-----------|--------------------------------------------|
/// | Enter     | Apply search, return to Normal with highlights |
/// | Esc       | Cancel search, return to Normal             |
/// | Backspace | Delete last character                       |
/// | Any char  | Append to search input                     |
///
/// ## Text Search
///
/// Press `?` to enter search mode. Type a search term (plain substring,
/// case-insensitive). Press Enter to apply the search. Matching cells
/// are highlighted with yellow text on a black background. Use `n` to
/// jump to the next matching cell and `N` for the previous one. Press
/// `Esc` in Normal mode to clear the search. Pressing `?` again
/// pre-fills the search bar with the current search term.
///
/// `n`/`N` always navigate search matches regardless of filter state.
/// Use `j`/`k` or arrow keys to navigate rows when a filter is active.
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
/// # Sort Cycling
///
/// Pressing `s` on a column cycles through:
/// - No sort -> Ascending (^)
/// - Ascending -> Descending (v)
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
/// When a text search is active (via `?`), any visible cell whose text
/// contains the search term (case-insensitive) is highlighted with
/// yellow foreground on black background. This visual highlight is
/// applied on top of row/column selection styles.
#[allow(dead_code)]
pub const KEYBINDINGS_VERSION: u8 = 7;
