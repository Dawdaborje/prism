# Prism (`prism`)

## Purpose

Business intelligence for every installed app. A person picks any data they may read, builds charts
and reusable metrics from it, and keeps them in workbooks. Dashboards (`prism_dashboard`) arrange the
charts; ready-made ones (`prism_pack`) ship for installed apps.

## Dependencies

None. `optional_dependencies = ["*"]` lets the plugin read the models of every installed app (see
**Access** below).

## Models

| Model | Role |
|---|---|
| `prism_workbook` | A named collection of charts. `shared` on: every Prism role reads it; off: its owner and administrators. |
| `prism_metric` | A named measure on a source, defined once (`code`) and used by any chart as `metric(code)`. |
| `prism_chart` | A chart: a source, what to group by, what to add up, an optional period and goal, and a type. `shared` and `owner` are copies of the workbook's, so the chart's rules need no second lookup. |

## A chart is text

```text
Group by      stage, expected_close:month
Measures      value    = sum_if(expected_value, {"closed_at": {"null": true}})
              deals    = count
              win_rate = ratio(_won, _closed)       # a name starting with _ is a helper, not shown
              growth   = change(value)
              profit   = metric(gross_profit)
Only where    {"active": true}
```

Measures: `count`, `sum`, `avg`, `min`, `max`, `count_if(filter)`, `sum_if(field, filter)`,
`avg_if(field, filter)`, `avg_days(from -> to|now)`. Formulas over other measures of the chart:
`ratio(a, b)`, `delta(a, b)`, `share(m)`, `prev(m)`, `change(m)`. A date takes a grain: `day`, `week`,
`month`, `quarter`, `year`. The text becomes one `report::query`; Prism adds nothing the report engine
cannot do, so every number is the kernel's.

Types: `number`, `bar`, `stacked_bar`, `row`, `line`, `area`, `pie`, `donut`, `funnel`, `map`, `bubble`,
`sankey`, `table`, `pivot`, `heatmap`. With two groups, bar, stacked bar, row, line and area split the first measure into one series per value of the
second.

## Numbers

A `number` chart states each visible measure as a card: its value, the previous period beside it
(`compare` on, with a `date_field` and a `period`), the change, a goal's progress (`target`), and a
trend over the date field when there is one. Periods that have a name before them (`this_month` →
`last_month`) compare against it; `last_7/30/90_days` against the stretch of the same length before.

## Funnel and heatmap

Neither adds a query; each is a different way to draw an answer the report engine already gives.

- `funnel`: one group (the stages) and one measure. Stages run from the largest down, a bar's width is its
  share of the largest, and the figure on the right is the share kept from the stage above. Stage order is by
  size, not by pipeline position, so it fits "who is still in play" and not a strict step-by-step funnel.
- `heatmap`: two groups (rows and columns) and one measure, drawn as a grid whose cells darken with size. It is
  a pivot without totals, so it drills down and downloads exactly like one.

## Map, bubble and flow

- `map`: one group whose values are countries (names such as `Germany`, or two or three letter codes) and one
  measure. A bubble sits on each country, sized by the figure. The browser holds a centre point for each country
  and a rough outline of the continents, so no map data is shipped; categories that are not countries are listed
  under the map instead of being dropped silently. Clicking a bubble drills into that country.
- `bubble`: one group, each value becoming one bubble. The first measure places it across, the second up, an
  optional third sizes it. Two measures are required.
- `sankey` ("flow"): two to four groups and one measure (use `count` or `sum`, which add up along a flow; an
  average does not). Each group is a column; a band joins a value to the value beside it, as thick as the
  figure. `row_limit` keeps that many of the largest bands and `truncated` says when some were left out.
  Clicking a node drills into it, clicking a band into both of its ends.

## Folders

A chart has an optional `folder` (`Sales`, `Sales / Pipeline`). It only groups: the workbook page offers *Group
by Folder* and sorts charts by folder then name. Dashboards are still how people find charts.

## The builder

New and edit chart pages use the `chart_builder` view (web: `widgets/board/builder/`). It reads the fields of
the chosen source with `get_source` and offers them as pickers: group-by fields (dates take a grain), measures
as *Sum of / Average of / …* over numeric fields, conditions with an operator and, for choice and link fields,
a list of the values that exist (`field_values`). It writes back exactly the text above, so a chart made in the
builder can be edited as text and the other way round. What a picker cannot show (a ratio, an `or` in a
filter) is kept as written. The preview calls `run_chart` with the unsaved `spec`, so what is drawn is what
the kernel answers for the person building it. The text form is no longer needed on those pages.

## Goals and "lower is better"

`target` is a goal. A number card shows progress to it and whether it is met (`status`: `on_track` or
`behind`); a bar, row, line or area chart draws it as a dashed line. `lower_is_better` turns both round for
costs and waiting times: a fall is green, a rise is red, and the goal is a limit that is met by staying under
it. Every reading carries its `tone` (`good`, `bad`, `flat`), so the screen never has to guess which way is good.

## Guide lines

`guide` draws a dashed line over a vertical bar, line or area chart: the `average` or `median` of its first
series, or its `trend` (the straight line that fits it best). Drawn in the browser from the figures shown, so it
describes what is on screen, filters included.

## Access (the point of the design)

Every query runs as the person looking at it. The kernel adds the source model's record rules and
hides its hidden fields to `report::query` and `db::find`, and a person only sees sources the rules let
them read anything of (`report::sources`). `"*"` attaches a model only when it has record rules, and
reads it *directly*: rules naming the owning plugin's roles are judged against the person's own roles,
and a trail through plugin functions earns no `via:` role. So a chart never shows what its viewer could
not open in the app itself, and Prism needs no sharing system of its own beyond who sees a workbook.

## Dashboard filters

A chart takes a list of controls (`field`, plus `eq`, `in`, `like`, `period` or `from`/`to`). A control
applies only when the chart's source has the field; `$date` means the chart's own `date_field`. A date
control replaces the chart's own period and still lets `compare` work (a named period compares against
the one before it; a custom range against the equal stretch before it). Controls that do not fit are
reported on the card (`meta.skipped`), never an error.

## Drill-down

`drill_chart` returns the records behind a bar, slice, table row or pivot cell: the chart's filters and
controls plus the clicked values (`segment`). A date group becomes the date range it stands for.

## Functions

`search_sources`, `get_source`, `field_values`, `run_chart`, `drill_chart`, `chart_drill`,
`export_chart`, `chart_board`, and create/set/get/search/delete for `workbook`, `metric`, `chart`
(plus `duplicate_chart`, `list_workbook_charts`).

## Roles

| Role | Use |
|---|---|
| `prism_viewer` | Opens shared workbooks and charts |
| `prism_analyst` | Builds workbooks, metrics and charts |
| `prism_admin` | Sees and changes everything |

## Boundaries

- Reads only. Prism never writes to another app's data.
- Joins across models and row-level scripts are not offered; the report engine groups one model. A link
  field groups by the linked record's name, which covers most "by customer / by team" questions.
- Money is the field's own currency. Prism does not convert between currencies.
