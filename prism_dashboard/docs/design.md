# Prism dashboards (`prism_dashboard`)

## Purpose

A dashboard is a grid of Prism charts under one bar of filters. One filter steers every chart it fits.

## Dependencies

`prism` (charts, drill-down and filter values are Prism's; this plugin calls them as the viewer).

## Models

| Model | Role |
|---|---|
| `prism_dashboard` | Name, description, `shared`, `refresh_secs`, `origin` (the ready-made dashboard it was copied from). |
| `prism_dashboard_item` | A chart on a dashboard: width out of 12 (3, 4, 6, 8, 12), a title that replaces the chart's own, and a position. Without a chart it is a **note**: a heading and a block of text. |
| `prism_dashboard_star` | A dashboard a person starred. Theirs alone: they read and change only their own stars. |
| `prism_dashboard_filter` | A control: `period` (date), `select` (pick values of a field, offered from `source`), or `text`. |

`shared` and `owner` are copied from the dashboard onto items and filters, so one rule covers all three.

## The board

`dashboard_board` answers what the `board` view draws: the filter bar (with each picker's values, most
common first) and a card per item. Filter values come back from the bar by filter name; a name the bar
has not sent uses the filter's default, one it sent empty stays empty. A card that fails shows its own
error; the rest still draw. `dashboard_drill` opens the records behind a bar under the same filters.

A filter's field is matched by name against each chart's source. `$date` is each chart's own date field,
so a single date filter works across charts that call their dates different things. A chart a filter does
not fit says so on its card.

## Notes

A note says what a section shows or how a figure is counted. It is a `text` card on the board, needs no access
to any data (so every viewer reads it), and is copied with the dashboard. Notes are added with `add_note` or
**Add note** on the editor.

## Starring

`star_dashboard` and `unstar_dashboard` mark a dashboard for the person calling. The list shows `starred` on each row
and puts starred dashboards first.

## Copying

`duplicate_dashboard` makes a private copy (items and filters included) with `origin` kept. Ready-made
dashboards belong to nobody, so people change a copy rather than the original.

## Functions

`dashboard_board`, `dashboard_drill`, create/set/get/search/delete/duplicate for dashboards, and for
items (`list_items`, `add_item`, `add_note`, `set_item`, `remove_item`, `item_up`, `item_down`) and filters
(`list_filters`, `add_filter`, `set_filter`, `remove_filter`).

## Roles

Prism's: viewers read shared dashboards; analysts build their own; administrators change any.
