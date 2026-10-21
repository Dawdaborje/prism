#!/bin/sh
# One-time setup of the organization prism_e2e.py runs against.
#
#   AETHER=/path/to/aether CONFIG=/path/to/aether.toml sh plugins/prism/tests/setup.sh
#
# The config's plugin_paths must list base, crm and prism, and the four Prism plugins must be built (`make`
# in each). The platform must be initialised already (`aether -c aether.toml --init`). Run it once per database.
set -e
A="${AETHER:-aether} -c ${CONFIG:-aether.toml}"
ORG="${ORG:-prism_test}"
PASS="${PASSWORD:-Test-pass-123}"

$A --load-plugins
$A --create-org "Prism Test" --org-db-name "$ORG" --username prism_owner --email prism_owner@test.local --password "$PASS"
# Installing the pack pulls prism and prism_dashboard; the CRM plugins are what it reads.
$A --install-plugin prism_pack prism_alert crm_deal crm_lead --org "$ORG"
for u in prism_analyst prism_analyst2 prism_viewer prism_crm; do
  $A --create-user --username "$u" --email "$u@test.local" --password "$PASS" --org-db-name "$ORG"
done
# Two analysts and a plain viewer, none of whom may read CRM data in the app itself...
$A --grant-role prism.prism_analyst --username prism_analyst --org "$ORG"
$A --grant-role prism.prism_analyst --username prism_analyst2 --org "$ORG"
$A --grant-role prism.prism_viewer --username prism_viewer --org "$ORG"
# ...and a viewer who can: deals and leads, and the masters (stage and source names) they link to.
$A --grant-role prism.prism_viewer --username prism_crm --org "$ORG"
for role in crm_deal.crm_user crm_lead.crm_user crm.crm_reader; do
  $A --grant-role "$role" --username prism_crm --org "$ORG"
done
# The first analyst reads CRM too, so their private chart over deals can run.
for role in crm_deal.crm_user crm_lead.crm_user crm.crm_reader; do
  $A --grant-role "$role" --username prism_analyst --org "$ORG"
done
echo "ready: organization $ORG, logins prism_owner prism_analyst prism_analyst2 prism_viewer prism_crm ($PASS)"
