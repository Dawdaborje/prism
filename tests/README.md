# Prism end-to-end checks

`prism_e2e.py` drives `prism`, `prism_dashboard` and `prism_pack` over HTTP as five people against real CRM
data, and checks both the numbers and who may see them. Plain Python 3, no packages.

## One-time setup

```sh
# in the project's config: plugin_paths = [base, crm, prism]; then
aether -c aether.toml --init
(cd plugins/prism/prism && make) && (cd plugins/prism/prism_dashboard && make) && (cd plugins/prism/prism_pack && make)
AETHER=aether CONFIG=aether.toml sh plugins/prism/tests/setup.sh
```

## Running

```sh
aether -c aether.toml --serve
python3 plugins/prism/tests/prism_e2e.py --base http://localhost:7890 --org prism_test
```

Every run tags its own deals, so it can run again on the same organization. It exits 1 if a check fails.

| Section | What it checks |
|---|---|
| 1 | sources follow the person's own access: a viewer without CRM roles is not offered deals |
| 2 | workbooks, metrics, charts; bad measures and unknown fields are refused in plain words |
| 3 | number cards: `sum_if`, metrics, ratios with hidden helper measures |
| 4 | bar, split series, table (exact decimals, totals), pivot (row/column totals) |
| 5 | periods, previous-period comparison, a dashboard date control replacing a period, skipped controls |
| 6 | drill-down, including a month group and paging |
| 7 | access: private workbooks, no CRM role, analyst vs viewer, other people's charts |
| 8 | Excel and PDF downloads |
| 9 | dashboards: filters steer charts, pickers offer values, a broken chart breaks only its card, copies |
| 10 | the CRM packs install, run every chart, and install again without duplicates |
| 11 | funnel, heatmap, goals where lower is better, notes on dashboards (and in copies) |
| 12 | guide lines, starred dashboards, alerts: checked as their owner, quiet when the line holds, no figures for someone without access, only the owner changes one |
