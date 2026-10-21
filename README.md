# Prism

Business intelligence for Aether. Pick any data you may read, build charts and reusable metrics, and put
them on dashboards with shared filters. Every number follows the record rules and hidden fields of the
person looking at it, and every figure drills down to the records behind it.

| Plugin | What it does |
|---|---|
| `prism` | Workbooks, metrics and charts over any installed app's data (number cards with period comparison, goals and better/worse colouring, bar, stacked bar, row, line, area, pie, donut, funnel, map, bubble, flow, table, pivot, heatmap; goal lines). A builder with field pickers and a live preview; charts can be put in folders. Drill-down and Excel/PDF download. |
| `prism_dashboard` | Dashboards: a grid of charts and notes under a filter bar. One date filter steers every chart; pickers apply to the charts that have the field. |
| `prism_alert` | Alerts: tell me when a number card crosses a line, in the app and by email. Each check runs as its owner, so it never shows more than Prism shows them. |
| `prism_pack` | Ready-made dashboards for installed apps (CRM sales and leads, helpdesk tickets and SLA, hiring, leave, sales orders, purchasing, stock, general ledger, receivables and cash), yours to copy. |

```sh
A="aether -c aether.toml"
$A --load-plugins ../plugins/prism
$A --install-plugin prism_pack --org <org>     # pulls prism and prism_dashboard
$A --install-plugin prism_alert --org <org>    # optional: alerts
$A --grant-role prism.prism_analyst --username <user> --org <org>
```

Open **Prism** in the app launcher. Each plugin's design is in its `docs/design.md`;
`tests/prism_e2e.py` checks the whole workspace against a running kernel (see the header of that file).

## What the kernel gives Prism

Three small kernel additions make this possible, the first two documented under *Queries* and the last under *the scheduler* in the architecture docs:

- `report::sources`, and `"*"` in `optional_dependencies`: read the models of every installed app, as the
  person, only where a model has record rules.
- `sum_if` and `avg_if` report measures, so "value of won deals" is one query.
- `run_as` on a background job (capability `scheduler::run_as`): alerts are checked as the person who made them.
