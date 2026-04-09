/**
 * Playwright global setup — onboard sample pool data via the API.
 * Runs once before all tests, after the webServer starts.
 */

const BASE_URL = "http://localhost:3000";
const SAMPLES_DIR = "./samples";

import { readFileSync, readdirSync, statSync } from "fs";
import { join } from "path";

export default async function globalSetup() {
    // Wait for server to be ready
    for (let i = 0; i < 30; i++) {
        try {
            const resp = await fetch(`${BASE_URL}/health`);
            if (resp.ok) break;
        } catch (_) {}
        await new Promise((r) => setTimeout(r, 1000));
    }

    // Check if data already exists
    const usersResp = await fetch(`${BASE_URL}/api/users`);
    const users = await usersResp.json();
    if (users.length > 1) {
        // Already onboarded (more than just the system admin)
        return;
    }

    // Onboard each sample pool directory
    const entries = readdirSync(SAMPLES_DIR)
        .filter((e) => statSync(join(SAMPLES_DIR, e)).isDirectory())
        .sort();

    for (const dir of entries) {
        const poolDir = join(SAMPLES_DIR, dir);
        const poolCsvPath = join(poolDir, "pool.csv");

        let poolCsv;
        try {
            poolCsv = readFileSync(poolCsvPath, "utf-8");
        } catch (_) {
            continue;
        }

        // Parse pool.csv to get pool name and member names
        const lines = poolCsv.trim().split("\n").slice(1); // skip header
        const poolName = lines[0].split(",")[0].trim();
        const memberNames = [...new Set(lines.map((l) => l.split(",")[1].trim()))];

        // Find SOV files
        const sovFiles = readdirSync(poolDir)
            .filter((f) => f.endsWith("-sov.csv"))
            .sort();

        const members = memberNames.map((name, idx) => {
            const sovFile = sovFiles[idx];
            const sovCsv = sovFile
                ? readFileSync(join(poolDir, sovFile), "utf-8")
                : "";
            return { member_name: name, sov_csv: sovCsv };
        });

        const resp = await fetch(`${BASE_URL}/api/onboard`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ pool_name: poolName, members }),
        });

        const result = await resp.json();
        console.log(
            `Onboarded: ${result.pool_name} — ${result.total_assets} assets, ${result.members.length} members`
        );
    }
}
