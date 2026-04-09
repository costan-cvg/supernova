#!/usr/bin/env bash
# Onboard sample pool data via the API.
# Usage: ./scripts/onboard-samples.sh [base_url]
#
# Requires: server running, curl, python3 (for JSON formatting)

set -euo pipefail

BASE_URL="${1:-http://localhost:3000}"
SAMPLES_DIR="$(dirname "$0")/../samples"

if ! curl -sf "$BASE_URL/health" > /dev/null 2>&1; then
    echo "Error: server not reachable at $BASE_URL"
    echo "Start the server first: cargo run -p centurisk-server"
    exit 1
fi

onboard_pool() {
    local pool_dir="$1"
    local pool_csv="$pool_dir/pool.csv"

    if [ ! -f "$pool_csv" ]; then
        echo "Skipping $pool_dir — no pool.csv"
        return
    fi

    # Read pool name from first data row
    local pool_name
    pool_name=$(tail -n +2 "$pool_csv" | head -1 | cut -d',' -f1)

    echo "Onboarding: $pool_name"

    # Build JSON request with members and their SOV CSVs
    local members_json="["
    local first=true

    # Get unique member names
    local member_names
    member_names=$(tail -n +2 "$pool_csv" | cut -d',' -f2 | sort -u)

    # Find SOV files
    local sov_files
    sov_files=$(find "$pool_dir" -name "*-sov.csv" | sort)

    local member_idx=0
    while IFS= read -r member_name; do
        [ -z "$member_name" ] && continue

        # Get the SOV file for this member (by index)
        local sov_file
        sov_file=$(echo "$sov_files" | sed -n "$((member_idx + 1))p")
        member_idx=$((member_idx + 1))

        local sov_csv=""
        if [ -n "$sov_file" ] && [ -f "$sov_file" ]; then
            sov_csv=$(cat "$sov_file")
        fi

        if [ "$first" = true ]; then
            first=false
        else
            members_json+=","
        fi

        # Escape the CSV content for JSON
        local escaped_csv
        escaped_csv=$(python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))" <<< "$sov_csv")

        members_json+="{\"member_name\":$(python3 -c "import json; print(json.dumps('$member_name'))"),\"sov_csv\":$escaped_csv}"
    done <<< "$member_names"

    members_json+="]"

    local request_json="{\"pool_name\":$(python3 -c "import json; print(json.dumps('$pool_name'))"),\"members\":$members_json}"

    # POST to onboard endpoint
    local response
    response=$(curl -sf -X POST "$BASE_URL/api/onboard" \
        -H "Content-Type: application/json" \
        -d "$request_json")

    # Display result
    echo "$response" | python3 -m json.tool 2>/dev/null || echo "$response"
    echo ""
}

echo "=== CentuRisk Pool Onboarding ==="
echo "Server: $BASE_URL"
echo ""

for pool_dir in "$SAMPLES_DIR"/*/; do
    onboard_pool "$pool_dir"
done

echo "=== Onboarding complete ==="
echo ""
echo "Users available (GET /api/users):"
curl -sf "$BASE_URL/api/users" | python3 -c "
import json, sys
users = json.load(sys.stdin)
for u in users:
    print(f\"  {u['display_name']:30} {u['category']:20} pool={str(u.get('pool_id','—'))[:8]}...\")
print(f'Total: {len(users)} users')
"
