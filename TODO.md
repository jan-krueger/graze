# graze TODO / Known Issues

All current TODOs have been implemented. Future ideas:

## Future phases (from original plan)
- **Phase 5**: Diff mode — `graze diff file1.parquet file2.parquet --key id,date`
- **Phase 6**: Parquet metadata inspection (encoding, compression, row groups)

## Regex support in search mode (currently plain substring) 
I think regex support is of the upmost importance

## Unified UI
Depending on the mode it says different things at the top. Like in the SQL mode it lists all keys and what they do. In statistics it just says "Esc" - and nothing else, etc. Also in statistics mode  the colum names have the same color as their type instead of the uniform color in the normal table view which i prefer. we should also find better symbols for indicating asc and desc sorting, and maybe use subindex numbers for numbering them

## Unified Search and Filter Experience
We also have a bunch of filter and search experiences that should probably be more unified and or evens simplified.

### General architecture
We should probably come up with a general architecture for all the various modi and functions we have, so that we have a consistent implementation and UX and UI in the end.

## Help page
We should have a page which documents all the various functions and key bindings>

## Future feature ideas
- Multi-column filter UI: pick column, pick operator, type value — no SQL knowledge needed
- Filter history (up arrow to recall previous filters)
- Search match counter (e.g., "3/17 matches")
