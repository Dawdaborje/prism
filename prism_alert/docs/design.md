# Prism alerts (`prism_alert`)

## Purpose

Tell me when a number crosses a line. An alert watches one reading of a Prism **number** chart and tells its
owner, in the app and optionally by email, when it goes above or below a limit, or rises or falls by more than a
percentage against the previous period. It says so once when the line is crossed and once when things are back
to normal, not on every check.

## Dependencies

`prism` (the chart is run by `prism.run_chart`).

## Models

| Model | Role |
|---|---|
| `prism_alert` | Name, chart, reading, condition (`above`, `below`, `rises_by`, `falls_by`, `is_empty`), limit, frequency (hourly, daily, weekly), emails, message, and what the last check found (`state`, `last_value`, `last_checked`, `last_fired`, `last_error`). |

## Whose numbers (the point of the design)

A scheduler job normally runs as the kernel, which would show an alert everything. Each check runs **as the
person who made the alert**: the job is queued with `run_as` (capability `scheduler::run_as`, see
[the scheduler](../../../../aether/docs/architecture/scheduler.md#running-a-job-as-a-person)), so the chart is read
under their roles and record rules and an alert can never tell its owner more than Prism shows them.

The person is never read from the alert record. A record's `owner` field is data anyone allowed to write the
record could change, and a job that trusted it would let them run as someone else. Instead:

1. The owner's own save (`create_alert`, `set_alert`) queues the first check, as them, and writes a fresh
   `chain` token on the alert.
2. Each check is a job `run_alert { id, chain }` running as that account. It does nothing unless the alert's
   `chain` still matches, the alert is on, and the alert's `owner` is the account it runs as.
3. Before checking, it queues the next one, as the same account. A failed check therefore never ends the
   chain; a person who has lost access to the chart sees `state = error` with the reason.

Changing or turning off an alert gives it a new (or empty) chain, so every older job stops at step 2 and two
chains never watch one alert. Only the owner changes or checks an alert; an administrator can read and delete.

## Delivery

An in-app notification to the owner always, and an email to up to ten addresses if given. The figure in an
email is what the owner sees; address the alert only to people who may know it. A mail that cannot be sent does
not undo the check.

## Functions

`create_alert`, `set_alert`, `get_alert`, `search_alerts`, `delete_alert`, `check_alert_now` (the same check,
once, without touching the schedule) and the background `run_alert`.

## Boundaries

- Number charts only; the reading is the chart's first unless named.
- `rises_by` and `falls_by` need **Compare with previous period** (and a date field) on the chart; otherwise the
  alert says so in `last_error`.
- Checks are hourly, daily or weekly, and happen at about that interval from the last save, not at a fixed time.
- If the kernel is down, missed checks are not made up; the next one runs when it is back.
