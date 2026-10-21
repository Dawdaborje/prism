//! The ready-made packs. A pack is data: a workbook of charts and a dashboard that uses them.
//!
//! Every chart names the models it reads as `plugin.model`, the way Prism's source picker does.
//! A pack is offered only when every plugin in `requires` is installed.

use aether_sdk::{json, Value};

const OPEN: &str = r#"{"closed_at": {"null": true}, "active": true}"#;
const WON: &str = r#"{"closed_at": {"null": false}, "lost_reason": {"null": true}}"#;

/// A chart definition: `code` is how the dashboard refers to it, the rest is what Prism stores.
fn chart(code: &str, name: &str, kind: &str, source: &str, rest: Value) -> Value {
    let mut chart = json!({ "code": code, "name": name, "kind": kind, "source": source });
    if let (Some(base), Some(extra)) = (chart.as_object_mut(), rest.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    chart
}

fn item(chart: &str, width: u8) -> Value {
    json!({ "chart": chart, "width": width.to_string() })
}

/// A block of text between charts: what a section shows, or how a figure is counted.
fn note(heading: &str, text: &str, width: u8) -> Value {
    json!({ "note": text, "title": heading, "width": width.to_string() })
}

const TICKET_OPEN: &str = r#"{"state": {"in": ["new", "open", "pending", "on_hold"]}}"#;
const HIRING_ACTIVE: &str = r#"{"status": "active"}"#;
const LEAVE_COUNTED: &str = r#"{"state": {"in": ["approved", "taken"]}}"#;
const ORDER_LIVE: &str = r#"{"state": {"nin": ["draft", "cancelled"]}}"#;
const INVOICED: &str = r#"{"state": "approved", "kind": "invoice"}"#;
const CREDITED: &str = r#"{"state": "approved", "kind": "credit_note"}"#;
const PO_LIVE: &str = r#"{"state": {"nin": ["draft", "cancelled"]}}"#;
const PO_OPEN: &str = r#"{"state": {"in": ["to_approve", "confirmed", "partially_received"]}}"#;
const BILLED: &str = r#"{"state": "approved", "kind": "bill"}"#;
const POSTED: &str = r#"{"state": "posted"}"#;
const DOC_OPEN: &str = r#"{"state": "open"}"#;
const PAID: &str = r#"{"state": "confirmed"}"#;

pub fn all() -> Vec<Value> {
    vec![
        crm_sales(),
        crm_leads(),
        helpdesk_tickets(),
        helpdesk_sla(),
        hr_hiring(),
        hr_leave(),
        erp_sales(),
        erp_purchasing(),
        erp_stock(),
        gl_activity(),
        gl_cash(),
    ]
}

pub fn find(code: &str) -> Option<Value> {
    all().into_iter().find(|pack| pack["code"] == code)
}

fn crm_sales() -> Value {
    let deal = "crm_deal.deal";
    json!({
        "code": "crm_sales",
        "name": "CRM sales",
        "description": "Pipeline value, wins, forecast and where deals are won and lost.",
        "requires": ["crm_deal"],
        "workbook": { "name": "CRM sales", "description": "Ready-made charts over deals. Make your own copy of the dashboard to change it." },
        "charts": [
            chart("open_value", "Open pipeline", "number", deal, json!({
                "measures": format!("open_value = sum_if(expected_value, {OPEN})"),
                "format": "currency",
                "description": "Expected value of deals still in play",
                "info": "Deals that are active and not yet won or lost, at their expected value."
            })),
            chart("open_deals", "Open deals", "number", deal, json!({
                "measures": format!("open_deals = count_if({OPEN})")
            })),
            chart("won", "Won", "number", deal, json!({
                "measures": format!("won_value = sum_if(expected_value, {WON})"),
                "format": "currency", "date_field": "closed_at", "period": "this_quarter", "compare": true,
                "description": "Value of deals won this quarter, against last quarter",
                "info": "A deal counts as won once it is closed without a lost reason."
            })),
            chart("win_rate", "Win rate", "number", deal, json!({
                "measures": format!("win_rate = ratio(_won, _closed)\n_won = count_if({WON})\n_closed = count_if({{\"closed_at\": {{\"null\": false}}}})"),
                "format": "percent", "date_field": "closed_at", "period": "this_quarter", "compare": true,
                "description": "Won deals over all deals closed this quarter"
            })),
            chart("average_deal", "Average won deal", "number", deal, json!({
                "measures": format!("average = avg_if(expected_value, {WON})"),
                "format": "currency", "date_field": "closed_at", "period": "this_year", "compare": true
            })),
            chart("by_stage", "Open pipeline by stage", "row", deal, json!({
                "dimensions": "stage",
                "measures": format!("value = sum_if(expected_value, {OPEN})"),
                "format": "compact", "sort": "-value", "row_limit": 12
            })),
            chart("forecast", "Forecast by expected close", "bar", deal, json!({
                "dimensions": "expected_close:month",
                "measures": format!("value = sum_if(expected_value, {OPEN})"),
                "format": "compact", "date_field": "expected_close", "period": "this_year",
                "description": "When open deals are expected to close"
            })),
            chart("won_by_month", "Won value by month", "area", deal, json!({
                "dimensions": "closed_at:month",
                "measures": format!("won_value = sum_if(expected_value, {WON})"),
                "format": "compact", "date_field": "closed_at", "period": "this_year"
            })),
            chart("lost_reasons", "Why deals are lost", "donut", deal, json!({
                "dimensions": "lost_reason",
                "measures": "lost = count",
                "filter": r#"{"lost_reason": {"null": false}}"#,
                "date_field": "closed_at"
            })),
            chart("by_source", "Deals by source", "donut", deal, json!({
                "dimensions": "source", "measures": "deals = count"
            })),
            chart("owners", "Owners", "table", deal, json!({
                "dimensions": "owner",
                "measures": format!("open_value = sum_if(expected_value, {OPEN})\nopen_deals = count_if({OPEN})\nwon_value = sum_if(expected_value, {WON})"),
                "format": "currency", "sort": "-open_value", "row_limit": 20
            })),
            chart("stage_by_team", "Open value: stage by team", "pivot", deal, json!({
                "dimensions": "stage, team",
                "measures": format!("value = sum_if(expected_value, {OPEN})"),
                "format": "compact"
            })),
        ],
        "dashboard": {
            "name": "Sales overview",
            "description": "How the pipeline looks today and how it is converting.",
            "filters": [
                { "name": "date", "label": "Closed or expected", "kind": "period", "field": "$date" },
                { "name": "team", "label": "Team", "kind": "select", "field": "team", "source": deal },
                { "name": "source", "label": "Source", "kind": "select", "field": "source", "source": deal }
            ],
            "items": [
                item("open_value", 3), item("open_deals", 3), item("won", 3), item("win_rate", 3),
                item("by_stage", 6), item("forecast", 6),
                item("won_by_month", 8), item("lost_reasons", 4),
                item("by_source", 4), item("owners", 8),
                item("stage_by_team", 12)
            ]
        }
    })
}

fn crm_leads() -> Value {
    let lead = "crm_lead.lead";
    json!({
        "code": "crm_leads",
        "name": "CRM leads",
        "description": "Lead volume, conversion and where leads come from.",
        "requires": ["crm_lead"],
        "workbook": { "name": "CRM leads", "description": "Ready-made charts over leads." },
        "charts": [
            chart("open_leads", "Open leads", "number", lead, json!({
                "measures": r#"leads = count_if({"active": true, "converted_deal": {"null": true}})"#
            })),
            chart("conversion", "Lead to deal", "number", lead, json!({
                "measures": r#"conversion = ratio(_converted, _all)
_converted = count_if({"converted_deal": {"null": false}})
_all = count"#,
                "format": "percent",
                "info": "Leads that became a deal, over all leads."
            })),
            chart("by_stage", "Lead funnel", "funnel", lead, json!({
                "dimensions": "stage", "measures": "leads = count", "row_limit": 8,
                "description": "Leads at each stage; the figure on the right is the share kept from the stage above"
            })),
            chart("by_source", "Leads by source", "donut", lead, json!({
                "dimensions": "source", "measures": "leads = count"
            })),
            chart("lost", "Why leads are lost", "donut", lead, json!({
                "dimensions": "lost_reason", "measures": "lost = count",
                "filter": r#"{"lost_reason": {"null": false}}"#
            })),
            chart("owners", "Leads by owner", "table", lead, json!({
                "dimensions": "owner",
                "measures": "leads = count\naverage_score = avg(score)",
                "sort": "-leads", "row_limit": 20
            })),
        ],
        "dashboard": {
            "name": "Lead funnel",
            "description": "Where leads come from and how many become deals.",
            "filters": [
                { "name": "team", "label": "Team", "kind": "select", "field": "team", "source": lead },
                { "name": "source", "label": "Source", "kind": "select", "field": "source", "source": lead }
            ],
            "items": [
                item("open_leads", 4), item("conversion", 4), item("lost", 4),
                item("by_stage", 6), item("by_source", 6),
                item("owners", 12)
            ]
        }
    })
}

fn helpdesk_tickets() -> Value {
    let ticket = "helpdesk_ticket.hd_ticket";
    json!({
        "code": "helpdesk_tickets",
        "name": "Helpdesk tickets",
        "description": "Backlog, speed of answers, who is waiting and where tickets come from.",
        "requires": ["helpdesk_ticket"],
        "workbook": { "name": "Helpdesk tickets", "description": "Ready-made charts over tickets. Make your own copy of the dashboard to change it." },
        "charts": [
            chart("open", "Open tickets", "number", ticket, json!({
                "measures": format!("open = count_if({TICKET_OPEN})"),
                "lower_is_better": true,
                "info": "Tickets that are new, open, pending or on hold."
            })),
            chart("waiting_on_us", "Waiting on us", "number", ticket, json!({
                "measures": format!("waiting = count_if({{\"waiting_for_us\": true, \"state\": {{\"in\": [\"new\", \"open\"]}}}})"),
                "lower_is_better": true,
                "description": "The customer wrote last and no agent has answered"
            })),
            chart("resolved", "Resolved", "number", ticket, json!({
                "measures": "resolved = count_if({\"resolved_at\": {\"null\": false}})",
                "date_field": "resolved_at", "period": "this_month", "compare": true,
                "description": "Tickets resolved this month, against last month"
            })),
            chart("first_response", "First response (minutes)", "number", ticket, json!({
                "measures": "minutes = avg(first_response_minutes)",
                "date_field": "opened_at", "period": "this_month", "compare": true, "lower_is_better": true,
                "info": "Average minutes from a ticket opening to the first agent reply, for tickets opened in the period."
            })),
            chart("resolution", "Time to resolve (minutes)", "number", ticket, json!({
                "measures": "minutes = avg(resolution_minutes)",
                "date_field": "resolved_at", "period": "this_month", "compare": true, "lower_is_better": true
            })),
            chart("rating", "Customer rating", "number", ticket, json!({
                "measures": "rating = avg(rating)",
                "date_field": "rated_at", "period": "this_quarter", "compare": true, "target": 4.5,
                "description": "Average rating out of 5 this quarter"
            })),
            chart("volume", "Tickets opened", "area", ticket, json!({
                "dimensions": "opened_at:week", "measures": "tickets = count",
                "date_field": "opened_at", "period": "last_90_days",
                "description": "New tickets a week over the last 90 days"
            })),
            chart("by_channel", "Where tickets come from", "donut", ticket, json!({
                "dimensions": "channel", "measures": "tickets = count", "date_field": "opened_at"
            })),
            chart("backlog", "Open tickets by team and priority", "heatmap", ticket, json!({
                "dimensions": "team, priority", "measures": format!("open = count_if({TICKET_OPEN})"),
                "description": "The darker the cell, the bigger the backlog"
            })),
            chart("agents", "Agents", "table", ticket, json!({
                "dimensions": "assignee",
                "measures": format!("open = count_if({TICKET_OPEN})\nresolved = count_if({{\"resolved_at\": {{\"null\": false}}}})\nminutes = avg(resolution_minutes)"),
                "sort": "-open", "row_limit": 20, "date_field": "opened_at"
            })),
        ],
        "dashboard": {
            "name": "Support overview",
            "description": "How big the backlog is and how quickly customers get answers.",
            "filters": [
                { "name": "date", "label": "Opened", "kind": "period", "field": "$date" },
                { "name": "team", "label": "Team", "kind": "select", "field": "team", "source": ticket },
                { "name": "channel", "label": "Channel", "kind": "select", "field": "channel", "source": ticket }
            ],
            "items": [
                item("open", 3), item("waiting_on_us", 3), item("resolved", 3), item("rating", 3),
                item("first_response", 6), item("resolution", 6),
                item("volume", 8), item("by_channel", 4),
                item("backlog", 12),
                item("agents", 12)
            ]
        }
    })
}

fn helpdesk_sla() -> Value {
    let clock = "helpdesk_sla.hd_sla_clock";
    json!({
        "code": "helpdesk_sla",
        "name": "Helpdesk SLA",
        "description": "How many promises to customers are kept, and where they slip.",
        "requires": ["helpdesk_sla"],
        "workbook": { "name": "Helpdesk SLA", "description": "Ready-made charts over SLA clocks." },
        "charts": [
            chart("compliance", "SLA met", "number", clock, json!({
                "measures": "met = ratio(_kept, _finished)\n_kept = count_if({\"state\": \"done\", \"breached\": false})\n_finished = count_if({\"state\": \"done\"})",
                "format": "percent", "date_field": "started_at", "period": "this_month", "compare": true, "target": 0.95,
                "info": "Clocks that finished inside their deadline, over all clocks that finished."
            })),
            chart("breached", "Breached", "number", clock, json!({
                "measures": "breached = count_if({\"breached\": true})",
                "date_field": "started_at", "period": "this_month", "compare": true, "lower_is_better": true
            })),
            chart("running", "Clocks running", "number", clock, json!({
                "measures": "running = count_if({\"state\": \"running\"})"
            })),
            chart("by_kind", "Kept and broken", "bar", clock, json!({
                "dimensions": "kind", "measures": "clocks = count\nbreached = count_if({\"breached\": true})",
                "date_field": "started_at"
            })),
            chart("by_team", "Breaches by team", "row", clock, json!({
                "dimensions": "team", "measures": "breached = count_if({\"breached\": true})",
                "sort": "-breached", "date_field": "started_at"
            })),
            chart("by_priority", "Breaches by priority", "donut", clock, json!({
                "dimensions": "priority", "measures": "breached = count",
                "filter": "{\"breached\": true}", "date_field": "started_at"
            })),
        ],
        "dashboard": {
            "name": "SLA performance",
            "description": "Promises kept to customers, by team and priority.",
            "filters": [
                { "name": "date", "label": "Started", "kind": "period", "field": "$date" },
                { "name": "team", "label": "Team", "kind": "select", "field": "team", "source": clock }
            ],
            "items": [
                item("compliance", 4), item("breached", 4), item("running", 4),
                note("How it is counted", "A clock runs while the customer waits on us and stops when we answer or resolve. It is breached when it passes its deadline first. Paused time does not count.", 12),
                item("by_kind", 4), item("by_team", 4), item("by_priority", 4)
            ]
        }
    })
}

fn hr_hiring() -> Value {
    let application = "hr_recruitment.rec_application";
    json!({
        "code": "hr_hiring",
        "name": "Hiring",
        "description": "Applicants at each stage, where they come from and who was hired.",
        "requires": ["hr_recruitment"],
        "workbook": { "name": "Hiring", "description": "Ready-made charts over applications." },
        "charts": [
            chart("active", "Active applications", "number", application, json!({
                "measures": format!("active = count_if({HIRING_ACTIVE})")
            })),
            chart("hired", "Hired", "number", application, json!({
                "measures": "hired = count_if({\"status\": \"hired\"})",
                "date_field": "hired_on", "period": "this_quarter", "compare": true
            })),
            chart("rejected", "Rejected", "number", application, json!({
                "measures": "rejected = count_if({\"status\": \"rejected\"})",
                "date_field": "stage_changed_on", "period": "this_quarter", "compare": true
            })),
            chart("funnel", "Hiring funnel", "funnel", application, json!({
                "dimensions": "stage", "measures": format!("applicants = count_if({HIRING_ACTIVE})"), "row_limit": 10,
                "description": "Active applicants at each stage"
            })),
            chart("by_source", "Where applicants come from", "donut", application, json!({
                "dimensions": "source", "measures": "applicants = count"
            })),
            chart("reasons", "Why applicants are rejected", "row", application, json!({
                "dimensions": "reject_reason", "measures": "rejected = count",
                "filter": "{\"status\": \"rejected\"}", "sort": "-rejected"
            })),
            chart("openings", "Openings", "table", application, json!({
                "dimensions": "opening",
                "measures": format!("active = count_if({HIRING_ACTIVE})\nhired = count_if({{\"status\": \"hired\"}})\nrejected = count_if({{\"status\": \"rejected\"}})"),
                "sort": "-active", "row_limit": 20
            })),
        ],
        "dashboard": {
            "name": "Hiring overview",
            "description": "The pipeline of applicants, opening by opening.",
            "filters": [
                { "name": "opening", "label": "Opening", "kind": "select", "field": "opening", "source": application },
                { "name": "source", "label": "Source", "kind": "select", "field": "source", "source": application }
            ],
            "items": [
                item("active", 4), item("hired", 4), item("rejected", 4),
                item("funnel", 6), item("by_source", 6),
                item("reasons", 6), item("openings", 6)
            ]
        }
    })
}

fn hr_leave() -> Value {
    let request = "hr_leave.leave_request";
    json!({
        "code": "hr_leave",
        "name": "Leave",
        "description": "Requests waiting for a decision, days taken and when people are away.",
        "requires": ["hr_leave"],
        "workbook": { "name": "Leave", "description": "Ready-made charts over leave requests." },
        "charts": [
            chart("pending", "Waiting for a decision", "number", request, json!({
                "measures": "pending = count_if({\"state\": {\"in\": [\"submitted\", \"first_approved\"]}})",
                "lower_is_better": true
            })),
            chart("days", "Days away this month", "number", request, json!({
                "measures": format!("days = sum_if(days, {LEAVE_COUNTED})"),
                "date_field": "start_date", "period": "this_month", "compare": true,
                "info": "Approved and taken leave that starts this month."
            })),
            chart("rejected", "Rejected", "number", request, json!({
                "measures": "rejected = count_if({\"state\": \"rejected\"})",
                "date_field": "start_date", "period": "this_year", "compare": true
            })),
            chart("by_type", "Days by type of leave", "donut", request, json!({
                "dimensions": "leave_type", "measures": format!("days = sum_if(days, {LEAVE_COUNTED})"),
                "date_field": "start_date", "period": "this_year"
            })),
            chart("by_month", "Days away by month", "bar", request, json!({
                "dimensions": "start_date:month", "measures": format!("days = sum_if(days, {LEAVE_COUNTED})"),
                "date_field": "start_date", "period": "this_year"
            })),
            chart("status_by_type", "Requests: type by decision", "heatmap", request, json!({
                "dimensions": "leave_type, state", "measures": "requests = count",
                "date_field": "start_date", "period": "this_year"
            })),
        ],
        "dashboard": {
            "name": "Leave overview",
            "description": "Who is away, for how long, and what is waiting for approval.",
            "filters": [
                { "name": "date", "label": "Starts", "kind": "period", "field": "$date" },
                { "name": "type", "label": "Type of leave", "kind": "select", "field": "leave_type", "source": request }
            ],
            "items": [
                item("pending", 4), item("days", 4), item("rejected", 4),
                item("by_month", 8), item("by_type", 4),
                note("Reading the grid", "Each cell counts the requests of one leave type in one state. A column of rejected or cancelled requests that grows faster than the rest is worth a conversation.", 12),
                item("status_by_type", 12)
            ]
        }
    })
}

fn erp_sales() -> Value {
    let (order, quote, invoice) = ("erp_selling.so", "erp_selling.quotation", "erp_selling.sales_invoice");
    json!({
        "code": "erp_sales",
        "name": "Sales orders and invoices",
        "description": "Orders, invoicing, quotations and who buys.",
        "requires": ["erp_selling"],
        "workbook": { "name": "Sales orders", "description": "Ready-made charts over quotations, orders and sales invoices." },
        "charts": [
            chart("ordered", "Ordered", "number", order, json!({
                "measures": format!("ordered = sum_if(total, {ORDER_LIVE})"),
                "format": "compact", "date_field": "order_date", "period": "this_month", "compare": true,
                "description": "Value of confirmed orders this month, against last month",
                "info": "Orders that are no longer drafts and not cancelled."
            })),
            chart("order_count", "Orders", "number", order, json!({
                "measures": format!("orders = count_if({ORDER_LIVE})"),
                "date_field": "order_date", "period": "this_month", "compare": true
            })),
            chart("invoiced", "Invoiced", "number", invoice, json!({
                "measures": format!("invoiced = sum_if(total, {INVOICED})"),
                "format": "compact", "date_field": "invoice_date", "period": "this_month", "compare": true,
                "info": "Approved invoices, before credit notes."
            })),
            chart("credited", "Credit notes", "number", invoice, json!({
                "measures": format!("credited = sum_if(total, {CREDITED})"),
                "format": "compact", "date_field": "invoice_date", "period": "this_month", "compare": true,
                "lower_is_better": true,
                "description": "Value credited back this month"
            })),
            chart("orders_by_month", "Orders by month", "bar", order, json!({
                "dimensions": "order_date:month",
                "measures": format!("ordered = sum_if(total, {ORDER_LIVE})"),
                "format": "compact", "date_field": "order_date", "period": "this_year", "guide": "trend"
            })),
            chart("invoiced_by_month", "Invoiced by month", "area", invoice, json!({
                "dimensions": "invoice_date:month",
                "measures": format!("invoiced = sum_if(total, {INVOICED})"),
                "format": "compact", "date_field": "invoice_date", "period": "this_year"
            })),
            chart("by_state", "Orders by state", "donut", order, json!({
                "dimensions": "state", "measures": "orders = count", "date_field": "order_date"
            })),
            chart("top_customers", "Top customers", "row", order, json!({
                "dimensions": "customer",
                "measures": format!("ordered = sum_if(total, {ORDER_LIVE})"),
                "format": "compact", "sort": "-ordered", "row_limit": 10, "date_field": "order_date"
            })),
            chart("customer_map", "Customers: orders against value", "bubble", order, json!({
                "dimensions": "customer",
                "measures": format!("orders = count_if({ORDER_LIVE})\nvalue = sum_if(total, {ORDER_LIVE})\naverage = avg_if(total, {ORDER_LIVE})"),
                "format": "compact", "row_limit": 25, "date_field": "order_date",
                "description": "Each bubble is a customer; the bigger, the larger their average order"
            })),
            chart("order_flow", "Where orders end up", "sankey", order, json!({
                "dimensions": "warehouse, state", "measures": "orders = count", "date_field": "order_date",
                "row_limit": 20,
                "description": "From the warehouse an order ships from to the state it is in"
            })),
            chart("quotes", "Quotations by outcome", "funnel", quote, json!({
                "dimensions": "state", "measures": "quotes = count", "date_field": "quote_date"
            })),
            chart("invoice_kinds", "Invoices and credit notes by state", "heatmap", invoice, json!({
                "dimensions": "kind, state", "measures": "documents = count", "date_field": "invoice_date"
            })),
        ],
        "dashboard": {
            "name": "Sales overview",
            "description": "What was ordered and invoiced, and by whom.",
            "filters": [
                { "name": "date", "label": "Order or invoice date", "kind": "period", "field": "$date" },
                { "name": "customer", "label": "Customer", "kind": "select", "field": "customer", "source": order }
            ],
            "items": [
                item("ordered", 3), item("order_count", 3), item("invoiced", 3), item("credited", 3),
                item("orders_by_month", 8), item("by_state", 4),
                item("invoiced_by_month", 6), item("quotes", 6),
                item("top_customers", 6), item("customer_map", 6),
                note("Reading the flow", "Each band is a number of orders. The left side is the warehouse they ship from, the right side the state they are in; click a band to see the orders.", 12),
                item("order_flow", 12),
                item("invoice_kinds", 12)
            ]
        }
    })
}

fn erp_purchasing() -> Value {
    let (po, bill, request) = ("erp_procurement.po", "erp_procurement.purchase_bill", "erp_procurement.material_request");
    json!({
        "code": "erp_purchasing",
        "name": "Purchasing",
        "description": "What is ordered, what is billed, and from whom.",
        "requires": ["erp_procurement"],
        "workbook": { "name": "Purchasing", "description": "Ready-made charts over purchase orders, bills and requests." },
        "charts": [
            chart("ordered", "Ordered", "number", po, json!({
                "measures": format!("ordered = sum_if(total, {PO_LIVE})"),
                "format": "compact", "date_field": "order_date", "period": "this_month", "compare": true,
                "lower_is_better": true,
                "description": "Value of purchase orders this month, against last month"
            })),
            chart("open_orders", "Open orders", "number", po, json!({
                "measures": format!("open_orders = count_if({PO_OPEN})"),
                "description": "Waiting for approval, for goods, or for the rest of the goods"
            })),
            chart("billed", "Billed", "number", bill, json!({
                "measures": format!("billed = sum_if(total, {BILLED})"),
                "format": "compact", "date_field": "bill_date", "period": "this_month", "compare": true,
                "lower_is_better": true
            })),
            chart("held", "Bills held back", "number", bill, json!({
                "measures": r#"held = count_if({"state": "approved", "release_to_pay": {"in": ["no", "exception"]}})"#,
                "lower_is_better": true,
                "info": "Approved bills that do not yet match what was ordered and received."
            })),
            chart("spend_by_month", "Spend by month", "bar", po, json!({
                "dimensions": "order_date:month",
                "measures": format!("ordered = sum_if(total, {PO_LIVE})"),
                "format": "compact", "date_field": "order_date", "period": "this_year", "guide": "average"
            })),
            chart("by_state", "Orders by state", "donut", po, json!({
                "dimensions": "state", "measures": "orders = count", "date_field": "order_date"
            })),
            chart("top_suppliers", "Top suppliers", "row", po, json!({
                "dimensions": "supplier",
                "measures": format!("ordered = sum_if(total, {PO_LIVE})"),
                "format": "compact", "sort": "-ordered", "row_limit": 10, "date_field": "order_date"
            })),
            chart("supplier_map", "Suppliers: orders against spend", "bubble", po, json!({
                "dimensions": "supplier",
                "measures": format!("orders = count_if({PO_LIVE})\nspend = sum_if(total, {PO_LIVE})"),
                "format": "compact", "row_limit": 25, "date_field": "order_date"
            })),
            chart("release", "Bills by release to pay", "donut", bill, json!({
                "dimensions": "release_to_pay", "measures": format!("bills = count_if({BILLED})"), "date_field": "bill_date"
            })),
            chart("requests", "Requests by stage", "funnel", request, json!({
                "dimensions": "state", "measures": "requests = count", "date_field": "request_date"
            })),
            chart("supplier_flow", "Orders from supplier to state", "sankey", po, json!({
                "dimensions": "supplier, state",
                "measures": format!("spend = sum_if(total, {PO_LIVE})"),
                "row_limit": 20, "format": "compact", "date_field": "order_date"
            })),
        ],
        "dashboard": {
            "name": "Purchasing overview",
            "description": "Spend, open orders and bills waiting for a decision.",
            "filters": [
                { "name": "date", "label": "Order or bill date", "kind": "period", "field": "$date" },
                { "name": "supplier", "label": "Supplier", "kind": "select", "field": "supplier", "source": po }
            ],
            "items": [
                item("ordered", 3), item("open_orders", 3), item("billed", 3), item("held", 3),
                item("spend_by_month", 8), item("by_state", 4),
                item("top_suppliers", 6), item("supplier_map", 6),
                item("release", 4), item("requests", 8),
                item("supplier_flow", 12)
            ]
        }
    })
}

fn erp_stock() -> Value {
    let (layer, movement, count) = ("erp_inventory.stock_layer", "erp_inventory.stock_move", "erp_inventory.stock_count_line");
    json!({
        "code": "erp_stock",
        "name": "Stock",
        "description": "What the stock is worth, where it is, and how it moves.",
        "requires": ["erp_inventory"],
        "workbook": { "name": "Stock", "description": "Ready-made charts over stock layers, movements and counts." },
        "charts": [
            chart("value", "Stock value", "number", layer, json!({
                "measures": "value = sum(remaining_value)",
                "format": "compact",
                "info": "What is left in the receipt layers, at the cost it came in at."
            })),
            chart("quantity", "Units on hand", "number", layer, json!({
                "measures": "units = sum(remaining_qty)", "format": "compact"
            })),
            chart("moves", "Movements", "number", movement, json!({
                "measures": format!("moves = count_if({POSTED})"),
                "date_field": "posting_date", "period": "this_month", "compare": true
            })),
            chart("count_gap", "Counting differences", "number", count, json!({
                "measures": "gap = sum(difference)", "lower_is_better": true,
                "description": "Counted minus expected, across all counts"
            })),
            chart("by_warehouse", "Value by warehouse", "donut", layer, json!({
                "dimensions": "warehouse", "measures": "value = sum(remaining_value)", "format": "compact"
            })),
            chart("by_item", "Items worth the most", "row", layer, json!({
                "dimensions": "item", "measures": "value = sum(remaining_value)",
                "format": "compact", "sort": "-value", "row_limit": 10
            })),
            chart("moves_by_month", "Movements by month and kind", "stacked_bar", movement, json!({
                "dimensions": "posting_date:month, kind",
                "measures": format!("moves = count_if({POSTED})"),
                "date_field": "posting_date", "period": "this_year"
            })),
            chart("by_method", "Value by valuation method", "pie", layer, json!({
                "dimensions": "valuation_method", "measures": "value = sum(remaining_value)", "format": "compact"
            })),
            chart("age", "Stock by age of the layer", "bar", layer, json!({
                "dimensions": "opened_at:quarter", "measures": "value = sum(remaining_value)",
                "format": "compact", "date_field": "opened_at",
                "info": "When the goods came in. A tall bar far to the left is stock that has sat a long time."
            })),
            chart("item_by_warehouse", "Value: item by warehouse", "heatmap", layer, json!({
                "dimensions": "item, warehouse", "measures": "value = sum(remaining_value)", "format": "compact"
            })),
        ],
        "dashboard": {
            "name": "Stock overview",
            "description": "Value on hand and how it moves.",
            "filters": [
                { "name": "date", "label": "Date", "kind": "period", "field": "$date" },
                { "name": "warehouse", "label": "Warehouse", "kind": "select", "field": "warehouse", "source": layer }
            ],
            "items": [
                item("value", 3), item("quantity", 3), item("moves", 3), item("count_gap", 3),
                item("moves_by_month", 8), item("by_warehouse", 4),
                item("by_item", 6), item("age", 6),
                item("by_method", 4), item("item_by_warehouse", 8)
            ]
        }
    })
}

fn gl_activity() -> Value {
    let (line, entry) = ("gl.gl_line", "gl.gl_entry");
    json!({
        "code": "gl_activity",
        "name": "General ledger",
        "description": "Postings by month, account and journal.",
        "requires": ["gl"],
        "workbook": { "name": "General ledger", "description": "Ready-made charts over ledger lines and entries." },
        "charts": [
            chart("debits", "Debits", "number", line, json!({
                "measures": "debit = sum(debit)", "format": "compact",
                "date_field": "date", "period": "this_month", "compare": true
            })),
            chart("credits", "Credits", "number", line, json!({
                "measures": "credit = sum(credit)", "format": "compact",
                "date_field": "date", "period": "this_month", "compare": true
            })),
            chart("entries", "Entries posted", "number", entry, json!({
                "measures": "entries = count", "date_field": "date", "period": "this_month", "compare": true
            })),
            chart("reversals", "Reversals", "number", entry, json!({
                "measures": r#"reversals = count_if({"reverses": {"null": false}})"#,
                "date_field": "date", "period": "this_month", "lower_is_better": true,
                "info": "Entries that undo an earlier entry."
            })),
            chart("by_month", "Debits and credits by month", "bar", line, json!({
                "dimensions": "date:month", "measures": "debit = sum(debit)\ncredit = sum(credit)",
                "format": "compact", "date_field": "date", "period": "this_year"
            })),
            chart("by_journal", "Entries by journal", "donut", entry, json!({
                "dimensions": "journal", "measures": "entries = count", "date_field": "date"
            })),
            chart("by_account", "Accounts", "table", line, json!({
                "dimensions": "account", "measures": "debit = sum(debit)\ncredit = sum(credit)\nlines = count",
                "format": "compact", "sort": "-debit", "row_limit": 25, "date_field": "date"
            })),
            chart("entries_by_month", "Entries by month", "line", entry, json!({
                "dimensions": "date:month", "measures": "entries = count", "date_field": "date", "period": "this_year"
            })),
            chart("sources", "Where entries come from", "pie", entry, json!({
                "dimensions": "source_kind", "measures": "entries = count", "date_field": "date"
            })),
            chart("account_month", "Debits: account by month", "heatmap", line, json!({
                "dimensions": "account, date:month", "measures": "debit = sum(debit)",
                "format": "compact", "date_field": "date", "period": "this_year"
            })),
        ],
        "dashboard": {
            "name": "Ledger overview",
            "description": "How much is being posted, and where.",
            "filters": [{ "name": "date", "label": "Posting date", "kind": "period", "field": "$date" }],
            "items": [
                item("debits", 3), item("credits", 3), item("entries", 3), item("reversals", 3),
                item("by_month", 8), item("by_journal", 4),
                item("entries_by_month", 6), item("sources", 6),
                item("by_account", 12),
                note("Reading the grid", "Each cell is what was debited to one account in one month. A row that suddenly lights up is an account whose activity changed.", 12),
                item("account_month", 12)
            ]
        }
    })
}

fn gl_cash() -> Value {
    let (document, payment) = ("gl_payment.pay_document", "gl_payment.payment");
    json!({
        "code": "gl_cash",
        "name": "Receivables, payables and cash",
        "description": "What is owed to you, what you owe, and what moved.",
        "requires": ["gl_payment"],
        "workbook": { "name": "Cash", "description": "Ready-made charts over open documents and payments." },
        "charts": [
            chart("receivable", "Owed to us", "number", document, json!({
                "measures": r#"owed = sum_if(total, {"state": "open", "side": "receivable"})"#,
                "format": "compact", "info": "Open invoices not yet settled by a payment."
            })),
            chart("payable", "We owe", "number", document, json!({
                "measures": r#"owed = sum_if(total, {"state": "open", "side": "payable"})"#,
                "format": "compact", "lower_is_better": true
            })),
            chart("received", "Received", "number", payment, json!({
                "measures": r#"received = sum_if(amount, {"state": "confirmed", "direction": "in"})"#,
                "format": "compact", "date_field": "date", "period": "this_month", "compare": true
            })),
            chart("paid_out", "Paid out", "number", payment, json!({
                "measures": r#"paid = sum_if(amount, {"state": "confirmed", "direction": "out"})"#,
                "format": "compact", "date_field": "date", "period": "this_month", "compare": true,
                "lower_is_better": true
            })),
            chart("due", "Open documents by month due", "stacked_bar", document, json!({
                "dimensions": "due_date:month, side",
                "measures": format!("open = sum_if(total, {DOC_OPEN})"),
                "format": "compact", "date_field": "due_date",
                "info": "Receivable and payable documents that are still open, by the month they fall due."
            })),
            chart("cash_by_month", "Money in and out by month", "bar", payment, json!({
                "dimensions": "date:month, direction",
                "measures": format!("amount = sum_if(amount, {PAID})"),
                "format": "compact", "date_field": "date", "period": "this_year"
            })),
            chart("by_method", "Payments by method", "donut", payment, json!({
                "dimensions": "method", "measures": format!("amount = sum_if(amount, {PAID})"),
                "format": "compact", "date_field": "date"
            })),
            chart("cash_flow", "Where money moves", "sankey", payment, json!({
                "dimensions": "direction, method",
                "measures": format!("amount = sum_if(amount, {PAID})"),
                "format": "compact", "date_field": "date"
            })),
        ],
        "dashboard": {
            "name": "Cash overview",
            "description": "Open balances and the money that moved.",
            "filters": [{ "name": "date", "label": "Date", "kind": "period", "field": "$date" }],
            "items": [
                item("receivable", 3), item("payable", 3), item("received", 3), item("paid_out", 3),
                item("due", 8), item("by_method", 4),
                item("cash_by_month", 6), item("cash_flow", 6)
            ]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dashboard_item_names_a_chart_of_its_pack() {
        for pack in all() {
            let codes: Vec<&str> = pack["charts"].as_array().unwrap().iter().map(|c| c["code"].as_str().unwrap()).collect();
            for item in pack["dashboard"]["items"].as_array().unwrap().iter().filter(|item| item.get("note").is_none()) {
                assert!(codes.contains(&item["chart"].as_str().unwrap()), "{} uses a chart it does not have", pack["code"]);
            }
            let mut sorted = codes.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), codes.len(), "{} repeats a chart code", pack["code"]);
        }
    }

    #[test]
    fn widths_fill_whole_rows() {
        for pack in all() {
            let mut row = 0;
            for item in pack["dashboard"]["items"].as_array().unwrap() {
                row += item["width"].as_str().unwrap().parse::<u32>().unwrap();
                assert!(row <= 12, "{} overflows a row", pack["code"]);
                if row == 12 {
                    row = 0;
                }
            }
            assert_eq!(row, 0, "{} ends with a short row", pack["code"]);
        }
    }

    #[test]
    fn embedded_filters_are_valid_json() {
        for pack in all() {
            for chart in pack["charts"].as_array().unwrap() {
                if let Some(filter) = chart["filter"].as_str() {
                    assert!(serde_json::from_str::<Value>(filter).unwrap().is_object());
                }
                let measures = chart["measures"].as_str().unwrap_or("");
                // Each JSON filter inside a measure must be written with matching braces.
                assert_eq!(measures.matches('{').count(), measures.matches('}').count(), "{}", chart["name"]);
            }
        }
    }

    #[test]
    fn packs_are_found_by_code() {
        assert!(find("crm_sales").is_some());
        assert!(find("nope").is_none());
    }
}
