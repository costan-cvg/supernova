import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  retries: 0,
  use: {
    baseURL: "http://localhost:3000",
    headless: true,
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
  webServer: {
    command:
      'bash -c "export PATH=$HOME/.cargo/bin:$PATH && cargo run -p centurisk-server"',
    port: 3000,
    timeout: 120_000,
    reuseExistingServer: !process.env.CI,
    env: {
      CENTURISK_DB_PATH: "./data/e2e-test.db",
      CENTURISK_STATIC_DIR: "./crates/centurisk-web/static",
    },
  },
});
