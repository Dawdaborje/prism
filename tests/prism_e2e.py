# End-to-end checks of Prism (prism, prism_dashboard, prism_pack) against a running kernel.
#
# Needs: the server running, an organization with prism_pack, crm_deal and crm_lead installed, and five logins in
# it. tests/setup.sh creates all of that. See README.md next to this file.
#
# Usage: python3 prism_e2e.py [--base http://localhost:7890] [--org prism_test] [-v]
# Every run tags its deals with a fresh code, so it can run again on the same organization.
import argparse, base64, datetime, json, secrets, sys, urllib.error, urllib.request

ap = argparse.ArgumentParser()
ap.add_argument("--base", default="http://localhost:7890")
ap.add_argument("--org", default="prism_test")
ap.add_argument("--password", default="Test-pass-123")
ap.add_argument("-v", action="store_true")
args = ap.parse_args()
RUN = secrets.token_hex(3)


class Failed(Exception):
    def __init__(self, status, data):
        super().__init__(f"{status}: {json.dumps(data)[:500]}")
        self.status, self.data = status, data


class Session:
    """A logged-in person: cookies kept by hand, the CSRF token is double-submitted."""

    def __init__(self, username):
        self.cookies = {"aether_csrf": secrets.token_hex(16)}
        self.name = username
        self.post("/api/auth/login", {"username": username, "password": args.password})
        self.post("/api/auth/org", {"org": args.org})

    def post(self, path, body):
        request = urllib.request.Request(args.base + path, data=json.dumps(body).encode(), method="POST")
        request.add_header("content-type", "application/json")
        request.add_header("origin", args.base)
        request.add_header("x-csrf-token", self.cookies["aether_csrf"])
        request.add_header("cookie", "; ".join(f"{k}={v}" for k, v in self.cookies.items()))
        try:
            response = urllib.request.urlopen(request)
            status, raw, headers = response.status, response.read(), response.headers
        except urllib.error.HTTPError as error:
            status, raw, headers = error.code, error.read(), error.headers
        for cookie in headers.get_all("set-cookie") or []:
            name, _, rest = cookie.partition("=")
            self.cookies[name.strip()] = rest.split(";")[0]
        try:
            data = json.loads(raw) if raw else None
        except ValueError:
            data = {"_raw": raw.decode(errors="replace")}
        if status >= 400:
            raise Failed(status, data)
        return data

    def call(self, plugin, function, body=None):
        answer = self.post(f"/api/plugins/{plugin}/{function}", body or {})
        if args.v:
            print(f"    {self.name} {plugin}.{function} -> {json.dumps(answer)[:300]}")
        return answer


results = []


def check(name, condition, detail=""):
    results.append((name, bool(condition)))
    print(("  ok    " if condition else "  FAIL  ") + name + ("" if condition else f"  [{detail}]"))


def refused(name, action, words=()):
    try:
        out = action()
    except Failed as failure:
        message = json.dumps(failure.data)
        check(name, not words or any(w in message for w in words), message[:300])
        return
    check(name, False, f"was allowed: {json.dumps(out)[:200]}")


def rows(answer):
    return answer if isinstance(answer, list) else answer.get("rows", [])


def values(card, i=0):
    return card["series"][i]["values"]


owner, analyst, viewer, crmv, analyst2 = (
    Session(u) for u in ("prism_owner", "prism_analyst", "prism_viewer", "prism_crm", "prism_analyst2")
)
today = datetime.date.today()
this_month = today.strftime("%Y-%m")

# ---------------------------------------------------------------------------------------------------------------
print("setup: a pipeline and deals of this run")
pipeline = rows(owner.call("crm", "search_pipelines", {}))[0]
stages = owner.call("crm", "list_stages", {"id": pipeline["id"]})
reasons = rows(owner.call("crm", "search_lost_reasons", {}))
company = pipeline["company"]
a, b = f"{RUN}-ann", f"{RUN}-bob"


def make_deal(title, who, value, outcome=None):
    deal = owner.call("crm_deal", "create_deal", {
        "company": company, "title": f"{RUN} {title}", "owner": who, "expected_value": str(value), "pipeline": pipeline["id"]})
    if outcome == "won":
        owner.call("crm_deal", "win_deal", {"id": deal["id"]})
    elif outcome == "lost":
        owner.call("crm_deal", "lose_deal", {"id": deal["id"], "lost_reason": reasons[0]["id"]})
    return deal


make_deal("open 1", a, 1000)
make_deal("open 2", a, 2000)
make_deal("open 3", b, 300)
make_deal("won 1", a, 3000, "won")
make_deal("won 2", b, 500, "won")
make_deal("lost 1", b, 700, "lost")
OURS = '{"title": {"like": "%s"}}' % RUN
OPEN = '{"closed_at": {"null": true}}'
WON = '{"closed_at": {"null": false}, "lost_reason": {"null": true}}'

# ---------------------------------------------------------------------------------------------------------------
print("1 sources follow the person's own access")
names = lambda s: {r["id"] for r in s.call("prism", "search_sources", {})}
check("an analyst with CRM access can pick deals", "crm_deal.deal" in names(analyst) or "crm_deal.deal" in names(crmv))
check("a viewer without CRM access is not offered deals", "crm_deal.deal" not in names(viewer))
check("Prism's own models are not offered as data", not any(n.startswith("prism") for n in names(crmv)))
fields = crmv.call("prism", "get_source", {"source": "crm_deal.deal"})["fields"]
check("a source lists its fields with kinds", {"stage", "expected_value", "closed_at"} <= {f["name"] for f in fields})
refused("a person without access cannot read a source's fields", lambda: viewer.call("prism", "get_source", {"source": "crm_deal.deal"}), ["not a source you can report on"])

# ---------------------------------------------------------------------------------------------------------------
print("2 building a workbook, a metric and charts")
wb = analyst.call("prism", "create_workbook", {"name": f"E2E {RUN}", "description": "test", "shared": True})
metric = analyst.call("prism", "create_metric", {"code": f"won_value_{RUN}", "name": "Won value", "source": "crm_deal.deal",
                                                  "measure": 'sum_if(expected_value, %s)' % WON, "format": "currency"})
refused("a metric must be a single measure", lambda: analyst.call("prism", "create_metric", {
    "code": f"bad_{RUN}", "name": "x", "source": "crm_deal.deal", "measure": "ratio(a, b)"}), ["measure"])
refused("a viewer cannot build", lambda: viewer.call("prism", "create_workbook", {"name": "nope"}), ["role"])


def chart(name, kind, **extra):
    body = {"workbook": wb["id"], "name": name, "kind": kind, "source": "crm_deal.deal", "filter": OURS}
    body.update(extra)
    return analyst.call("prism", "create_chart", body)


open_value = chart("Open value", "number", measures="open_value = sum_if(expected_value, %s)" % OPEN, format="currency")
won_metric = chart("Won via metric", "number", measures=f"v = metric(won_value_{RUN})")
by_owner = chart("Deals by owner", "bar", dimensions="owner", measures="deals = count", sort="owner")
split = chart("Value by owner and kind", "stacked_bar", dimensions="owner, stage", measures="n = count")
table = chart("Table", "table", dimensions="owner", measures="deals = count\nvalue = sum(expected_value)", sort="owner")
pivot = chart("Pivot", "pivot", dimensions="owner, stage", measures="n = count")
rate = chart("Win rate", "number", measures="rate = ratio(_won, _closed)\n_won = count_if(%s)\n_closed = count_if({\"closed_at\": {\"null\": false}})" % WON,
             format="percent")
refused("a chart with a measure that does not parse is refused", lambda: chart("bad", "bar", measures="x = median(amount)"), ["not a measure"])
refused("an unknown group-by field says what exists", lambda: analyst.call("prism", "run_chart", {"spec": {
    "source": "crm_deal.deal", "kind": "bar", "dimensions": "nope", "measures": "n = count"}}), ["you can use"])

# ---------------------------------------------------------------------------------------------------------------
print("3 numbers")
card = crmv.call("prism", "run_chart", {"chart": open_value["id"]})
check("sum_if adds only the matching deals", card["readings"][0]["value"] == 3300, card["readings"])
check("the card says what it read", card["source"] == "crm_deal.deal" and card["format"] == "currency")
card = crmv.call("prism", "run_chart", {"chart": won_metric["id"]})
check("a metric stands for its measure", card["readings"][0]["value"] == 3500, card["readings"])
card = crmv.call("prism", "run_chart", {"chart": rate["id"]})
check("helper measures are hidden and the ratio is right", [r["name"] for r in card["readings"]] == ["rate"] and abs(card["readings"][0]["value"] - 2 / 3) < 1e-3, card["readings"])

# ---------------------------------------------------------------------------------------------------------------
print("4 charts")
card = crmv.call("prism", "run_chart", {"chart": by_owner["id"]})
check("a bar chart groups by the field", dict(zip(card["categories"], values(card))) == {a: 3.0, b: 3.0}, card["categories"])
check("segments carry the raw value for drill-down", card["keys"] == [a, b] and card["dimensions"][0] == "owner")
card = crmv.call("prism", "run_chart", {"chart": split["id"]})
check("two groups split the measure into series", len(card["series"]) >= 2 and sum(sum(s["values"]) for s in card["series"]) == 6, card["series"])
card = crmv.call("prism", "run_chart", {"chart": table["id"]})
check("a table has its columns and rows", [c["key"] for c in card["columns"]] == ["owner", "deals", "value"] and len(card["rows"]) == 2, card["columns"])
ann = next(r for r in card["rows"] if r["keys"]["owner"] == a)
check("a table adds up decimals exactly", ann["cells"]["value"] == 6000.0, ann)
check("a table has totals", card["totals"]["deals"] == 6 and card["totals"]["value"] == 7500.0, card["totals"])
card = crmv.call("prism", "run_chart", {"chart": pivot["id"]})
check("a pivot has row and column totals", sum(card["row_totals"]) == 6 and sum(card["column_totals"]) == 6 and card["grand"] == 6, card)
check("a pivot's matrix agrees with its totals", sum(sum(r) for r in card["matrix"]) == 6)

# ---------------------------------------------------------------------------------------------------------------
print("5 periods, comparison and dashboard controls")
won_now = chart("Won this month", "number", measures="n = count_if(%s)" % WON, date_field="closed_at", period="this_month", compare=True)
card = crmv.call("prism", "run_chart", {"chart": won_now["id"]})
check("a period narrows by the date field", card["readings"][0]["value"] == 2, card["readings"])
check("compare reads the previous period", card["compared"] and card["readings"][0]["previous"] == 0 and card["readings"][0]["change"] is None, card["readings"])
check("a date field gives the card a trend", len(card["trend"]) >= 1 and card["trend"][-1]["value"] == 2, card["trend"])
card = crmv.call("prism", "run_chart", {"chart": won_now["id"], "controls": [{"field": "$date", "period": "last_year"}]})
check("a dashboard date control replaces the chart's own period", card["readings"][0]["value"] == 0 and "$date" in card["meta"]["applied"], card["meta"])
card = crmv.call("prism", "run_chart", {"chart": won_now["id"], "controls": [{"field": "$date", "from": str(today), "to": str(today)}]})
check("a custom range covers the whole last day", card["readings"][0]["value"] == 2, card["readings"])
check("a custom range still compares with the stretch before it", card["compared"] and card["readings"][0]["previous"] == 0, card["readings"])
card = crmv.call("prism", "run_chart", {"chart": by_owner["id"], "controls": [{"field": "owner", "in": [a]}]})
check("a field control filters the chart", card["categories"] == [a] and values(card) == [3.0], card["categories"])
card = crmv.call("prism", "run_chart", {"chart": by_owner["id"], "controls": [{"field": "no_such_field", "eq": "x"}]})
check("a control that does not fit is reported, not applied", len(card["meta"]["skipped"]) == 1 and sum(values(card)) == 6, card["meta"])
card = crmv.call("prism", "run_chart", {"chart": by_owner["id"], "controls": [{"field": "owner", "eq": ""}]})
check("an empty control asks for nothing", sum(values(card)) == 6)

# ---------------------------------------------------------------------------------------------------------------
print("6 drill-down")
drill = crmv.call("prism", "drill_chart", {"chart": by_owner["id"], "segment": {"owner": a}})
check("a bar opens the records behind it", drill["total"] == 3 and all(r["cells"].get("owner") == a for r in drill["rows"]), drill["total"])
by_month = chart("Won by month", "bar", dimensions="closed_at:month", measures="n = count_if(%s)" % WON)
card = crmv.call("prism", "run_chart", {"chart": by_month["id"]})
check("a date grain is labelled YYYY-MM", this_month in card["categories"], card["categories"])
drill = crmv.call("prism", "drill_chart", {"chart": by_month["id"], "segment": {"closed_at": this_month}})
check("a date group opens every record of that month (the lost deal closed then too)", drill["total"] == 3, drill["total"])
drill = crmv.call("prism", "drill_chart", {"chart": by_owner["id"], "segment": {"owner": a}, "limit": 2})
check("drill-down pages", len(drill["rows"]) == 2 and drill["total"] == 3)
refused("drill-down will not group on an unknown field", lambda: crmv.call("prism", "drill_chart", {"chart": by_owner["id"], "segment": {"nope": 1}}), ["not a field"])

# ---------------------------------------------------------------------------------------------------------------
print("7 access")
refused("a viewer without CRM access cannot run a chart on deals", lambda: viewer.call("prism", "run_chart", {"chart": open_value["id"]}),
        ["not a source you can report on"])
refused("nor drill into it", lambda: viewer.call("prism", "drill_chart", {"chart": by_owner["id"], "segment": {}}), ["not a source you can report on"])
check("a viewer can still open the chart record", viewer.call("prism", "get_chart", {"id": open_value["id"]})["name"] == "Open value")
private = analyst.call("prism", "create_workbook", {"name": f"Private {RUN}", "shared": False})
secret = analyst.call("prism", "create_chart", {"workbook": private["id"], "name": "Secret", "kind": "number", "source": "crm_deal.deal", "measures": "n = count"})
refused("a private chart is invisible to others", lambda: crmv.call("prism", "get_chart", {"id": secret["id"]}))
refused("and cannot be run by them", lambda: crmv.call("prism", "run_chart", {"chart": secret["id"]}))
check("others' searches do not list it", secret["id"] not in {c["id"] for c in rows(crmv.call("prism", "search_charts", {"q": "Secret"}))})
check("an administrator sees it", owner.call("prism", "get_chart", {"id": secret["id"]})["name"] == "Secret")
check("its owner runs it", analyst.call("prism", "run_chart", {"chart": secret["id"]})["readings"][0]["value"] >= 6)
refused("another analyst cannot change a chart that is not theirs", lambda: analyst2.call("prism", "set_chart", {"id": won_now["id"], "name": "hijack"}))
refused("a viewer cannot save a chart", lambda: viewer.call("prism", "create_chart", {"workbook": wb["id"], "name": "x", "source": "crm_deal.deal", "measures": "n = count"}), ["role"])
refused("an unsaved spec is for analysts", lambda: viewer.call("prism", "run_chart", {"spec": {"source": "crm_deal.deal", "kind": "number", "measures": "n = count"}}), ["role"])

# ---------------------------------------------------------------------------------------------------------------
print("8 charts as files")
export = crmv.call("prism", "export_chart", {"chart": table["id"], "format": "xlsx"})
check("a chart downloads as Excel", export["filename"].endswith(".xlsx") and base64.b64decode(export["data"])[:2] == b"PK", export.get("filename"))
export = crmv.call("prism", "export_chart", {"chart": by_owner["id"], "format": "pdf"})
check("and as PDF", base64.b64decode(export["data"])[:4] == b"%PDF")

# ---------------------------------------------------------------------------------------------------------------
print("9 dashboards")
dash = analyst.call("prism_dashboard", "create_dashboard", {"name": f"Board {RUN}", "shared": True})
owner_filter = analyst.call("prism_dashboard", "add_filter", {"id": dash["id"], "name": "who", "label": "Owner", "kind": "select", "field": "owner", "source": "crm_deal.deal"})
date_filter = analyst.call("prism_dashboard", "add_filter", {"id": dash["id"], "name": "date", "label": "Date", "kind": "period", "field": "$date"})
refused("a filter name is unique in its dashboard", lambda: analyst.call("prism_dashboard", "add_filter", {
    "id": dash["id"], "name": "who", "label": "Again", "kind": "text", "field": "title"}), ["already"])
refused("a value picker needs its source", lambda: analyst.call("prism_dashboard", "add_filter", {
    "id": dash["id"], "name": "x", "label": "X", "kind": "select", "field": "owner"}), ["needs the data"])
for item in (open_value, by_owner, won_now):
    analyst.call("prism_dashboard", "add_item", {"id": dash["id"], "chart": item["id"], "width": "6"})
board = crmv.call("prism_dashboard", "dashboard_board", {"id": dash["id"].split(":")[-1]})
check("the board has a card per item", len(board["items"]) == 3 and all("card" in i for i in board["items"]), board["items"])
bar = next(i for i in board["items"] if i["card"]["kind"] == "bar")
check("an item keeps its width", bar["width"] == 6)
bar_filter = next(f for f in board["filters"] if f["name"] == "who")
check("a value picker offers the field's values with counts", {a, b} <= {o["value"] for o in bar_filter["options"]}, bar_filter["options"])
board = crmv.call("prism_dashboard", "dashboard_board", {"id": dash["id"], "filters": {"who": [a], "date": ""}})
bar = next(i for i in board["items"] if i["card"]["kind"] == "bar")
check("a filter steers the charts it fits", bar["card"]["categories"] == [a], bar["card"]["categories"])
numbers = {i["card"]["title"]: i["card"] for i in board["items"] if i["card"]["kind"] == "number"}
check("and the numbers", numbers["Open value"]["readings"][0]["value"] == 3000, numbers["Open value"]["readings"])
board = crmv.call("prism_dashboard", "dashboard_board", {"id": dash["id"], "filters": {"date": "last_year"}})
won_card = next(i for i in board["items"] if i["card"]["title"] == "Won this month")["card"]
check("one date filter steers a chart whatever its date field", won_card["readings"][0]["value"] == 0)
open_card = next(i for i in board["items"] if i["card"]["title"] == "Open value")["card"]
check("a chart without a date field is left alone, and says so", open_card["readings"][0]["value"] == 3300 and open_card["meta"]["skipped"], open_card["meta"])
drill = crmv.call("prism_dashboard", "dashboard_drill", {"id": dash["id"], "chart": by_owner["id"], "segment": {"owner": b}, "filters": {}})
check("a dashboard bar drills down", drill["total"] == 3)
drill = crmv.call("prism_dashboard", "dashboard_drill", {"id": dash["id"], "chart": by_owner["id"], "segment": {"owner": b}, "filters": {"who": [a]}})
check("under the dashboard's filters", drill["total"] == 0, drill["total"])
refused("a viewer without CRM access gets errors on cards, not the data", lambda: None) if False else None
board = viewer.call("prism_dashboard", "dashboard_board", {"id": dash["id"]})
check("a viewer without CRM access sees the board but only errors", all("error" in i for i in board["items"]) and len(board["items"]) == 3, board["items"])
items = analyst.call("prism_dashboard", "list_items", {"id": dash["id"]})
check("items list their chart names", [i["chart_name"] for i in items] == ["Open value", "Deals by owner", "Won this month"], [i["chart_name"] for i in items])
analyst.call("prism_dashboard", "item_down", {"id": items[0]["id"]})
check("an item moves down", [i["chart_name"] for i in analyst.call("prism_dashboard", "list_items", {"id": dash["id"]})][:2] == ["Deals by owner", "Open value"])
analyst.call("prism_dashboard", "item_up", {"id": items[0]["id"]})
check("and back up", [i["chart_name"] for i in analyst.call("prism_dashboard", "list_items", {"id": dash["id"]})][0] == "Open value")
copy = analyst2.call("prism_dashboard", "duplicate_dashboard", {"id": dash["id"]})
check("a copy is private with its items and filters", copy["shared"] is False and len(analyst2.call("prism_dashboard", "list_items", {"id": copy["id"]})) == 3
      and len(analyst2.call("prism_dashboard", "list_filters", {"id": copy["id"]})) == 2, copy)
refused("the original stays its owner's", lambda: analyst2.call("prism_dashboard", "set_dashboard", {"id": dash["id"], "name": "mine now"}))
analyst.call("prism", "delete_chart", {"id": by_owner["id"]})
board = crmv.call("prism_dashboard", "dashboard_board", {"id": dash["id"]})
check("a deleted chart breaks only its own card", sum("error" in i for i in board["items"]) == 1 and sum("card" in i for i in board["items"]) == 2, board["items"])

# ---------------------------------------------------------------------------------------------------------------
print("10 ready-made dashboards")
packs = {p["id"]: p for p in owner.call("prism_pack", "list_packs")}
check("the packs are listed with whether their app is installed", packs["crm_sales"]["available"] and packs["crm_leads"]["available"], packs)
first = owner.call("prism_pack", "install_pack", {"code": "crm_sales"})
check("a pack installs", first["charts"] >= 10 and first["dashboard"], first)
again = owner.call("prism_pack", "install_pack", {"code": "crm_sales"})
check("installing again changes nothing", again["workbook"] == first["workbook"] and again["dashboard"] == first["dashboard"], again)
sales = [d for d in rows(owner.call("prism_dashboard", "search_dashboards", {"q": "Sales overview"})) if d.get("origin") == "pack:crm_sales" and d["name"] == "Sales overview"]
check("one dashboard per pack, however often it is installed", len(sales) == 1, sales)
board = crmv.call("prism_dashboard", "dashboard_board", {"id": sales[0]["id"]})
broken = [(i.get("title") or i.get("card", {}).get("title"), i["error"]) for i in board["items"] if "error" in i]
check("every chart of the sales pack runs", not broken and len(board["items"]) == 11, broken)
kinds = sorted({i["card"]["kind"] for i in board["items"] if "card" in i})
check("the pack covers number, row, bar, area, donut, table and pivot", {"number", "row", "bar", "area", "donut", "table", "pivot"} <= set(kinds), kinds)
check("the pack's filters are on the board", {f["name"] for f in board["filters"]} == {"date", "team", "source"}, board["filters"])
leads = owner.call("prism_pack", "install_pack", {"code": "crm_leads"})
lead_board = crmv.call("prism_dashboard", "dashboard_board", {"id": leads["dashboard"]})
broken = [(i.get("title") or i.get("card", {}).get("title"), i["error"]) for i in lead_board["items"] if "error" in i]
check("every chart of the lead pack runs", not broken and len(lead_board["items"]) == 6, broken)
fork = analyst.call("prism_dashboard", "duplicate_dashboard", {"id": sales[0]["id"]})
check("a pack dashboard can be forked and keeps its origin", fork["origin"] == "pack:crm_sales" and fork["shared"] is False, fork)
refused("a pack dashboard cannot be changed by an analyst", lambda: analyst.call("prism_dashboard", "set_dashboard", {"id": sales[0]["id"], "name": "x"}))
seed = owner.call("prism_pack", "seed_packs", {})
check("the install seed refreshes every available pack", len(seed["installed"]) == 2 and not seed["failed"], seed)

# ---------------------------------------------------------------------------------------------------------------
print("11 funnel, heatmap, goals and notes")
funnel = chart("Funnel", "funnel", dimensions="owner", measures="deals = count")
card = analyst.call("prism", "run_chart", {"chart": funnel["id"]})
check("a funnel is a chart of stages from the largest down", card["kind"] == "funnel" and card["series"][0]["values"] == [3, 3], card)
heat = chart("Heat", "heatmap", dimensions="owner, stage", measures="n = count")
card = analyst.call("prism", "run_chart", {"chart": heat["id"]})
check("a heatmap is a grid of counts without totals to read", card["kind"] == "heatmap" and sum(sum(row) for row in card["matrix"]) == 6, card)
check("and a cell drills down like a pivot cell", analyst.call("prism", "drill_chart", {"chart": heat["id"], "segment": {"owner": a}})["total"] == 3)
refused("a heatmap needs a row field", lambda: analyst.call("prism", "run_chart", {"spec": {
    "source": "crm_deal.deal", "kind": "heatmap", "measures": "n = count"}}), ["row field"])
cost = chart("Open deals under a limit", "number", measures="open = count_if(%s)" % OPEN, target=5, lower_is_better=True)
card = analyst.call("prism", "run_chart", {"chart": cost["id"]})
check("a goal where lower is better is met by staying under it", card["status"] == "on_track" and card["lower_is_better"], card)
tight = chart("Open deals over a limit", "number", measures="open = count_if(%s)" % OPEN, target=2, lower_is_better=True)
check("and missed by going over", analyst.call("prism", "run_chart", {"chart": tight["id"]})["status"] == "behind")
goal = chart("Deals by owner with a goal", "bar", dimensions="owner", measures="deals = count", target=4)
check("a bar chart carries its goal for the line", analyst.call("prism", "run_chart", {"chart": goal["id"]})["target"] == 4)
note = analyst.call("prism_dashboard", "add_note", {"id": dash["id"], "title": "Reading this", "text": "Open means not closed yet."})
check("a note goes on a dashboard", note["text"] == "Open means not closed yet." and note["width"] == "12", note)
board = crmv.call("prism_dashboard", "dashboard_board", {"id": dash["id"]})
texts = [i["card"] for i in board["items"] if "card" in i and i["card"]["kind"] == "text"]
check("the board shows it as a text card", len(texts) == 1 and texts[0]["text"] == "Open means not closed yet." and texts[0]["title"] == "Reading this", texts)
board = viewer.call("prism_dashboard", "dashboard_board", {"id": dash["id"]})
check("a note needs no access to anyone's data", any("card" in i and i["card"]["kind"] == "text" for i in board["items"]))
listed = analyst.call("prism_dashboard", "list_items", {"id": dash["id"]})
check("the editor lists it as a note", any(i["chart_kind"] == "note" and i["chart_name"] == "Open means not closed yet." for i in listed), listed)
refused("a note cannot be empty", lambda: analyst.call("prism_dashboard", "add_note", {"id": dash["id"], "text": " "}), ["required"])
copy = analyst2.call("prism_dashboard", "duplicate_dashboard", {"id": dash["id"]})
check("a copy keeps its notes", any(i["chart_kind"] == "note" for i in analyst2.call("prism_dashboard", "list_items", {"id": copy["id"]})))
sheet = analyst.call("prism", "export_chart", {"chart": heat["id"], "format": "xlsx"})
check("a heatmap downloads like a pivot", sheet, sheet)

# ---------------------------------------------------------------------------------------------------------------
print("12 guide lines, starred dashboards and alerts")
import time
guided = chart("Guided", "bar", dimensions="owner", measures="deals = count", guide="average")
check("a chart carries its guide line", analyst.call("prism", "run_chart", {"chart": guided["id"]})["guide"] == "average")
refused("an unknown guide line is refused", lambda: chart("Bad guide", "bar", dimensions="owner", measures="n = count", guide="mode"), ["guide"])
board_row = lambda who: next(d for d in rows(who.call("prism_dashboard", "search_dashboards", {"q": f"Board {RUN}"})) if d["id"] == dash["id"])
check("a dashboard starts unstarred", board_row(crmv)["starred"] is False)
crmv.call("prism_dashboard", "star_dashboard", {"id": dash["id"]})
crmv.call("prism_dashboard", "star_dashboard", {"id": dash["id"]})
check("starring marks it for that person only, once", board_row(crmv)["starred"] is True and board_row(analyst)["starred"] is False)
crmv.call("prism_dashboard", "unstar_dashboard", {"id": dash["id"]})
check("and unstarring takes it off", board_row(crmv)["starred"] is False)


def wait_for(alert_id, who, states, seconds=40):
    """Alerts are checked by a background job; wait for it to record an answer."""
    end = time.time() + seconds
    while time.time() < end:
        found = who.call("prism_alert", "get_alert", {"id": alert_id})
        if found.get("state") in states:
            return found
        time.sleep(1)
    return who.call("prism_alert", "get_alert", {"id": alert_id})


watched = analyst.call("prism_alert", "create_alert", {"name": f"Open value {RUN}", "chart": open_value["id"], "condition": "above", "threshold": 1000, "frequency": "hourly"})
check("an alert is made by its owner", watched["owner"] and watched["chain"], watched)
done = wait_for(watched["id"], analyst, {"firing", "ok", "error"})
check("the background check runs as the owner and sees their deals", done["state"] == "firing" and done["last_value"] == 3300, done)
quiet = analyst.call("prism_alert", "create_alert", {"name": f"Never {RUN}", "chart": open_value["id"], "condition": "above", "threshold": 99999, "frequency": "hourly"})
done = wait_for(quiet["id"], analyst, {"firing", "ok", "error"})
check("an alert whose line is not crossed stays quiet", done["state"] == "ok", done)
blind = analyst2.call("prism_alert", "create_alert", {"name": f"Blind {RUN}", "chart": open_value["id"], "condition": "above", "threshold": 0, "frequency": "hourly"})
done = wait_for(blind["id"], analyst2, {"firing", "ok", "error"})
check("a person without access to the data cannot get its figures through an alert", done["state"] != "firing" and done.get("last_value") in (None, 0), done)
check("it reports why instead", done["state"] == "error" and done.get("last_error"), done)
refused("an alert on a chart that is not a number chart is refused", lambda: analyst.call("prism_alert", "create_alert", {
    "name": "x", "chart": table["id"], "condition": "above", "threshold": 1}), ["number chart"])
refused("a limit is required", lambda: analyst.call("prism_alert", "create_alert", {"name": "x", "chart": open_value["id"], "condition": "above"}), ["limit"])
refused("an address must look like one", lambda: analyst.call("prism_alert", "create_alert", {
    "name": "x", "chart": open_value["id"], "condition": "above", "threshold": 1, "emails": "nope"}), ["email"])
refused("a viewer cannot make alerts", lambda: viewer.call("prism_alert", "create_alert", {"name": "x", "chart": open_value["id"], "threshold": 1}), ["role"])
refused("only the owner changes an alert", lambda: analyst2.call("prism_alert", "set_alert", {"id": watched["id"], "name": "mine now"}))
refused("or checks it", lambda: analyst2.call("prism_alert", "check_alert_now", {"id": watched["id"]}))
analyst.call("prism_alert", "set_alert", {"id": watched["id"], "active": False})
off = analyst.call("prism_alert", "get_alert", {"id": watched["id"]})
check("turning an alert off ends its chain of checks", off["active"] is False and off["chain"] == "", off)
now_check = analyst.call("prism_alert", "check_alert_now", {"id": quiet["id"]})
check("Check now answers at once", now_check["state"] == "ok", now_check)
for a_id in (watched["id"], quiet["id"]):
    analyst.call("prism_alert", "delete_alert", {"id": a_id})
analyst2.call("prism_alert", "delete_alert", {"id": blind["id"]})

# ---------------------------------------------------------------------------------------------------------------
failed = [name for name, ok in results if not ok]
print(f"\n{len(results) - len(failed)}/{len(results)} checks passed")
for name in failed:
    print("  FAILED:", name)
sys.exit(1 if failed else 0)
