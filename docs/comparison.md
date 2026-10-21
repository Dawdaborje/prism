# Where Prism sits among BI tools

Prism was designed after weighing two kinds of tool.

**Pipeline BI** connects data sources, lets people build queries as a pipeline of steps (join, filter,
calculate, summarize; or SQL, or a script), then draws charts over a query and arranges them on dashboards.
**Spreadsheet dashboards** ship a dashboard per app as a spreadsheet of formulas over the app's data, so a
dashboard is as flexible as a spreadsheet and as hard to keep correct.

| | Pipeline BI | Spreadsheet dashboards | Prism |
|---|---|---|---|
| Data | Any database, connected by an administrator | The app's own models | Every installed app's models, read as the viewer |
| Who may see a number | The source connection's rights, then the tool's own sharing | The app's rules, with a share link that runs as its owner | Only the record rules and hidden fields of the person looking. No second sharing system |
| Building | Pipelines, SQL, scripts: powerful, and a language to learn | Spreadsheet formulas | One short text per chart (group by, measures), checked in plain words |
| Reuse | Queries source other queries | Copy the spreadsheet | Metrics defined once, used by any chart |
| Ready on day one | A few sample dashboards | A dashboard per app | Packs per installed app |
| Alerts, public links | Yes | Share links | Not yet (see below) |

## What Prism keeps from each

- From pipeline BI: charts that drill down to the records behind a figure, number cards with period
  comparison, goals and a trend, dashboard filters that route to the charts that have the field, funnel and
  heatmap, goal lines, notes between charts.
- From spreadsheet dashboards: dashboards that exist before anyone builds one, one per installed app.

## What Prism chooses not to copy

- **Joins, SQL and scripts.** They are what make pipeline BI hard to trust: a clever query can read what its
  reader may not. Prism runs one model at a time through the kernel's report engine, so a figure never
  includes a record the viewer could not open in the app. A link field groups by the linked record's name,
  which answers most "by customer / by team" questions.
- **External databases.** The data is in Aether already.
- **Public links that run as the owner.** They make a number visible to people the app would not show it to.

## What was missing, and what is still

Alerts are now built (`prism_alert`). The obstacle was that a scheduler job runs as the kernel, which would show
an alert everything; jobs can now run as a named person (`run_as`), and each check runs as the alert's owner.
Starring dashboards, guide lines (average, median, trend) and goal lines are built too.

The chart builder, bubble and flow charts, the country map and workbook folders are built too:

- **A chart builder with field pickers.** A purpose-built `chart_builder` view: the fields of the chosen data
  as pickers, a list of the values that exist for conditions, and a live preview. It writes the same text a
  chart is stored as.
- **Bubble and flow charts.** Bubble places each group by two measures and sizes it by a third; a flow joins
  the values of two to four fields.
- **A map.** A bubble per country, placed from a built-in table of country centres, so no map data is shipped.
  It does not draw borders, regions or cities; a field that is not a country is listed beneath the map.
- **Folders.** A chart's `folder` groups the workbook list.

Still open: borders and regions on the map (they need real geometry), and calculated fields written once and
used across charts beyond what `metric(code)` gives.
