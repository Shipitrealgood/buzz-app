import assert from "node:assert/strict";
import { test } from "node:test";
import { chromium, expect } from "@playwright/test";

test("the installed WebView rejects signed-out host commands and stays responsive", async (t) => {
  const port = process.env.BUZZ_TEST_CDP_PORT;
  assert.match(port ?? "", /^\d+$/);
  const endpoint = `http://127.0.0.1:${port}`;
  const appUrl = /^https?:\/\/tauri\.localhost\/(?:index\.html)?(?:[?#]|$)/;
  // Wait for the installed app's page before connecting to WebView2.
  await expect
    .poll(
      async () => {
        try {
          const response = await fetch(`${endpoint}/json/list`, {
            signal: AbortSignal.timeout(1000),
          });
          const targets = await response.json();
          return targets.some((p) => p.type === "page" && appUrl.test(p.url));
        } catch {
          return false;
        }
      },
      { timeout: 30_000 },
    )
    .toBe(true);
  const browser = await chromium.connectOverCDP(endpoint, {
    noDefaults: true,
    timeout: 5000,
  });
  t.after(() => browser.close());
  const page = browser
    .contexts()
    .flatMap((c) => c.pages())
    .find((p) => appUrl.test(p.url()));
  assert.ok(page, "packaged Buzz WebView missing");
  await page.waitForFunction(
    () => window.__TAURI_INTERNALS__?.metadata.currentWindow.label === "main",
    undefined,
    { timeout: 5000 },
  );
  const identity = () =>
    page.evaluate(() => window.__TAURI_INTERNALS__.invoke("identity_restore"));
  assert.equal(await identity(), null, "candidate runner must be signed out");

  const scope = {
    community: "https://example.invalid",
    owner: "11".repeat(32),
  };
  const host = { ...scope, host: "22".repeat(32), generation: "aa".repeat(32) };
  const agent = { ...scope, pubkey: "33".repeat(32) };
  const runtimeId = "test-runtime";
  const name = "Test agent";
  for (const [command, args] of [
    ["recover", scope],
    ["status", host],
    [
      "models",
      { input: { ...host, runtimeId, catalogRevision: "bb".repeat(32) } },
    ],
    ["provision", { input: { ...host, runtimeId, name } }],
    ["activate", { input: { ...agent, name, nipOa: ["o", "", "", ""] } }],
    ["profile", { input: { ...agent, name } }],
    ["remove", agent],
  ]) {
    const result = await page.evaluate(
      async ([command, args]) => {
        try {
          await window.__TAURI_INTERNALS__.invoke(
            `registered_host_${command}`,
            args,
          );
          return null;
        } catch (reason) {
          return reason;
        }
      },
      [command, args],
    );
    assert.equal(
      result,
      command === "recover"
        ? "Registered host recovery is unavailable"
        : "Set up your identity first",
      command,
    );
  }
  assert.equal(
    await identity(),
    null,
    "the same native app must still answer IPC",
  );
});
