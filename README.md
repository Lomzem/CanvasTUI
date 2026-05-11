# CanvasTUI

CanvasTUI is a personal terminal client for Canvas planner items. It is optimized for instant startup from cache and fast keyboard navigation.

## Setup

Set these environment variables in your shell:

```bash
export CANVAS_URL="https://canvas.example.edu"
export CANVAS_ACCESS_TOKEN="your-token"
```

The app uses the Canvas planner API with `Authorization: Bearer ...` authentication.

## Behavior

1. On startup, CanvasTUI loads cached planner data from `~/.cache/canvastui/snapshot.json` or `$XDG_CACHE_HOME/canvastui/snapshot.json`.
2. It immediately revalidates in the background.
3. It fetches all planner item types for the loaded date range.
4. It skips empty dates when navigating.
5. It automatically loads older or newer chunks as you move beyond the currently loaded range.

Default range behavior:

1. Initial refresh: `today .. today + 60 days`
2. Automatic history expansion: `30` days per chunk backward or forward

## Controls

- `j` / `k`: next / previous item
- `h` / `l`: previous / next populated day
- `g` / `G`: first / last loaded populated day
- `0`: jump back to the default landing day
- `r`: refresh loaded planner data
- `o`: open the selected item if Canvas provides an `html_url`
- `q`: quit

Submitted or completed items stay visible with a checkmark and green styling.
