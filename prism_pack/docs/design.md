# Prism packs (`prism_pack`)

## Purpose

Ready-made dashboards for the apps an organization has installed, so Prism is useful on the first day.

## Dependencies

`prism`, `prism_dashboard`. `optional_dependencies` lists the apps a pack reads (`crm_deal`, `crm_lead`, `helpdesk_ticket`, `helpdesk_sla`, `hr_recruitment`, `hr_leave`, `erp_selling`, `erp_procurement`, `erp_inventory`, `gl`, `gl_payment`).

## Packs

| Pack | Needs | Contents |
|---|---|---|
| `crm_sales` | `crm_deal` | Open pipeline, open deals, won, win rate, average won deal, pipeline by stage, forecast by close month, won by month, why deals are lost, deals by source, owners, stage by team. Dashboard "Sales overview". |
| `crm_leads` | `crm_lead` | Open leads, lead-to-deal, a funnel by stage, leads by source, why leads are lost, leads by owner. Dashboard "Lead funnel". |
| `helpdesk_tickets` | `helpdesk_ticket` | Open and waiting-on-us tickets, resolved, first response and resolution time (lower is better), customer rating against a goal, tickets opened a week, channels, a team × priority backlog heatmap, agents. Dashboard "Support overview". |
| `helpdesk_sla` | `helpdesk_sla` | SLA met against a 95% goal, breached, clocks running, kept and broken by kind, breaches by team and priority. Dashboard "SLA performance". |
| `hr_hiring` | `hr_recruitment` | Active, hired and rejected applications, a hiring funnel, sources, reject reasons, openings. Dashboard "Hiring overview". |
| `hr_leave` | `hr_leave` | Requests waiting, days away this month, rejected, days by type and month, a type × decision heatmap. Dashboard "Leave overview". |
| `erp_sales` | `erp_selling` | Ordered, orders, invoiced and credit notes this month, orders and invoicing by month, orders by state, top customers, a bubble of customers (orders against value, sized by average order), a flow from warehouse to order state, a quotation funnel, an invoice kind × state heatmap. Dashboard "Sales overview". |
| `erp_purchasing` | `erp_procurement` | Ordered, open orders, billed and bills held back, spend by month, orders by state, top suppliers, a bubble of suppliers, bills by release to pay, a request funnel, a flow from supplier to order state. Dashboard "Purchasing overview". |
| `erp_stock` | `erp_inventory` | Stock value, units on hand, movements, counting differences, value by warehouse and item, movements by month and kind, value by valuation method and by age of the layer, an item × warehouse heatmap. Dashboard "Stock overview". |
| `gl_activity` | `gl` | Debits, credits, entries and reversals this month, debits and credits by month, entries by journal and source, an account table, an account × month heatmap. Dashboard "Ledger overview". |
| `gl_cash` | `gl_payment` | Owed to us, we owe, received and paid out this month, open documents by month due, money in and out by month, payments by method, a flow from direction to method. Dashboard "Cash overview". |

Money in these packs is shown compact and without a currency symbol, because the documents carry their own currency and Prism does not convert.

A pack is data in `src/packs.rs`: a workbook, its charts and a dashboard. Adding one is adding an entry. A dashboard item
is a chart (`item(code, width)`) or a note (`note(heading, text, width)`).

## Behaviour

- `seed` (on install and upgrade) and the **Ready-made** page install every pack whose app is installed.
  Installing is idempotent: charts are matched by name in the pack's workbook and updated; the dashboard
  (found by name and `origin = pack:<code>`) gets its items and filters again.
- Packs belong to nobody and are shared, so every Prism role can open them and only administrators change
  them. A refresh overwrites changes to a pack; copy the dashboard (**Make my copy**) to change it freely.
- An app installed after Prism Packs is picked up by **Install or refresh** on the Ready-made page, or
  `aether --command prism_pack.seed --org <org>`.

## Boundaries

A pack shows what its viewer may read in the app. A viewer without access to the app's data sees empty
cards, not an error.
