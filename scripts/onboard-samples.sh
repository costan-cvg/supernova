#!/usr/bin/env bash
# Onboard sample pool data via the API.
# Usage: ./scripts/onboard-samples.sh [base_url]
#
# Requires: server running, python3

set -euo pipefail

BASE_URL="${1:-http://localhost:3000}"
SAMPLES_DIR="$(cd "$(dirname "$0")/../samples" && pwd)"

if ! curl -sf "$BASE_URL/health" > /dev/null 2>&1; then
    echo "Error: server not reachable at $BASE_URL"
    echo "Start the server first: cargo run -p centurisk-server"
    exit 1
fi

echo "=== CentuRisk Pool Onboarding ==="
echo "Server: $BASE_URL"
echo ""

for pool_dir in "$SAMPLES_DIR"/*/; do
    [ -f "$pool_dir/pool.csv" ] || continue

    # Use python to build the JSON request body and POST it
    python3 - "$pool_dir" "$BASE_URL" <<'PYEOF'
import csv, json, sys, os, glob, urllib.request

pool_dir = sys.argv[1]
base_url = sys.argv[2]

# Read pool.csv
with open(os.path.join(pool_dir, "pool.csv")) as f:
    reader = csv.DictReader(f)
    rows = list(reader)

pool_name = rows[0]["pool_name"].strip()
member_names = list(dict.fromkeys(r["member_name"].strip() for r in rows))

# Find SOV files
sov_files = sorted(glob.glob(os.path.join(pool_dir, "*-sov.csv")))

members = []
for i, name in enumerate(member_names):
    sov_csv = ""
    if i < len(sov_files):
        with open(sov_files[i]) as f:
            sov_csv = f.read()
    members.append({"member_name": name, "sov_csv": sov_csv})

payload = json.dumps({"pool_name": pool_name, "members": members}).encode()

req = urllib.request.Request(
    f"{base_url}/api/onboard",
    data=payload,
    headers={"Content-Type": "application/json"},
    method="POST",
)

try:
    with urllib.request.urlopen(req) as resp:
        result = json.loads(resp.read())
        print(f"  {result['pool_name']}: {result['total_assets']} assets, {len(result['members'])} members")
        for m in result["members"]:
            print(f"    - {m['member_name']}: {m['assets_imported']} assets")
            for e in m.get("errors", []):
                print(f"      ERROR: {e}")
except urllib.error.HTTPError as e:
    print(f"  ERROR: {e.code} {e.read().decode()}")
except Exception as e:
    print(f"  ERROR: {e}")
PYEOF

    sleep 0.5
done

echo ""
echo "=== Onboarding complete ==="
echo ""
echo "Users:"
sleep 0.5
python3 -c "
import json, urllib.request, sys, time

for attempt in range(3):
    try:
        with urllib.request.urlopen('$BASE_URL/api/users') as resp:
            users = json.loads(resp.read())
            for u in users:
                pool = (u.get('pool_id') or '\u2014')[:8]
                print(f'  {u[\"display_name\"]:30} {u[\"category\"]:20} pool={pool}')
            print(f'Total: {len(users)} users')
            break
    except Exception as e:
        if attempt < 2:
            time.sleep(1)
        else:
            print(f'  Could not fetch users: {e}')
"
