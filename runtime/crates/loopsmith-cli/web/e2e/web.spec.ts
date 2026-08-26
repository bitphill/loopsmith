import { test, expect } from "@playwright/test";

/**
 * These cover the seam: does the page mount, does it reach the API, and do the
 * few interactions that are genuinely browser-side behave. Validation rules,
 * argv construction, secret quoting, and everything else live in Rust tests.
 */

test("the shell mounts and reports the binary's own version", async ({ page }) => {
  await page.goto("/");
  // The mark is served straight from the binary, so a broken route shows up
  // here rather than as a silently missing image.
  const logo = await page.request.get("/logo.png");
  expect(logo.status()).toBe(200);
  expect(logo.headers()["content-type"]).toContain("image/png");
  await expect(page.getByRole("heading", { name: "loopsmith", level: 1 })).toBeVisible();
  // Served from the binary, so a version here proves the API round trip too.
  await expect(page.locator("header").getByText(/^\d+\.\d+\.\d+$/)).toBeVisible();
});

test("the first-run tour appears, explains the rule, and can be dismissed", async ({ page }) => {
  await page.goto("/");
  const tour = page.getByRole("dialog", { name: "How loopsmith works" });
  await expect(tour).toBeVisible();

  await tour.getByRole("button", { name: "Next" }).click();
  // The load-bearing idea. If this panel ever stops saying it, the tour has
  // lost the only thing it exists to teach.
  await expect(tour).toContainText("must not certify its own completion");

  await tour.getByRole("button", { name: "Skip" }).click();
  await expect(tour).toBeHidden();

  // Dismissal sticks across a reload.
  await page.reload();
  await expect(page.getByRole("dialog", { name: "How loopsmith works" })).toBeHidden();
});

/**
 * Fields are matched exactly. Each one's info control is deliberately labelled
 * "What is <field>?" for screen readers, which substring-matches the field's
 * own name — without `exact`, `getByLabel` resolves to the button.
 */
const field = (page: import("@playwright/test").Page, name: string) =>
  page.getByLabel(name, { exact: true }).first();

test.describe("with the tour dismissed", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem("loopsmith-tour", "done"));
    await page.goto("/");
  });

  test("the example library lists loops and loading one fills the form", async ({ page }) => {
    const rail = page.locator("aside").first();
    await expect(rail.locator("article")).not.toHaveCount(0);

    const first = rail.locator("article").first();
    const name = await first.getByRole("heading").innerText();
    await first.getByRole("button", { name: "Load" }).click();

    // The form is empty on first load, so this needs no confirmation.
    await expect(page.locator("#open-path")).toHaveCount(0);
    await expect(field(page, "Loop name")).toHaveValue(name);
    await expect(page.getByText(/Nothing is on disk yet/)).toBeVisible();
  });

  test("loading over a filled form asks before discarding it", async ({ page }) => {
    await field(page, "Loop name").fill("my-own-loop");

    const rail = page.locator("aside").first();
    await rail.locator("article").first().getByRole("button", { name: "Load" }).click();

    const dialog = page.getByRole("dialog", { name: /already filled some of this in/i });
    await expect(dialog).toBeVisible();

    // "Fill blanks only" must leave what the user typed alone. This is the
    // whole point of offering two buttons rather than one.
    await dialog.getByRole("button", { name: "Fill blanks only" }).click();
    await expect(field(page, "Loop name")).toHaveValue("my-own-loop");
  });

  test("the review rail refuses a goal that nothing checks", async ({ page }) => {
    await field(page, "Loop name").fill("unchecked");

    // Goals live on the Intent step now. Walking there is part of what is
    // being checked: the review rail has to keep watching across steps.
    await page.getByRole("tab", { name: "Intent" }).click();
    await page.getByRole("button", { name: "Add a goal" }).click();
    await field(page, "Name").fill("g1");
    await field(page, "Description").fill("a goal with a long enough description to be accepted");

    // A goal with no validation is the single most common way a loop fails,
    // and the config is refused rather than run.
    const right = page.locator("aside").last();
    await expect(right.getByText(/error/i).first()).toBeVisible({ timeout: 10_000 });
  });

  test("each step shows only its own actions", async ({ page }) => {
    // The whole point of the restructure: nine buttons at once was the wall.
    await expect(page.getByRole("button", { name: "Run once" })).toHaveCount(0);

    await page.getByRole("tab", { name: "Ship" }).click();
    await expect(page.getByRole("button", { name: "Run once" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Create loop" })).toBeVisible();
  });

  test("Previous and Next walk the steps and stop at both ends", async ({ page }) => {
    const prev = page.getByRole("button", { name: "← Previous" });
    const next = page.getByRole("button", { name: "Next →" });

    // Place is the first step, so there is nowhere back to go.
    await expect(prev).toBeDisabled();
    await next.click();
    await expect(page.getByRole("tab", { name: "Power" })).toHaveAttribute("aria-selected", "true");

    await expect(prev).toBeEnabled();
    await prev.click();
    await expect(page.getByRole("tab", { name: "Place" })).toHaveAttribute("aria-selected", "true");

    // And Ship is the last, so Next has nowhere to go either.
    await page.getByRole("tab", { name: "Ship" }).click();
    await expect(next).toBeDisabled();
  });

  test("the command palette reaches a step the current view does not show", async ({ page }) => {
    await page.keyboard.press("ControlOrMeta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await expect(palette).toBeVisible();

    // Subsequence matching, so a rough guess still lands.
    await palette.getByPlaceholder(/Jump to a section/).fill("stpgt");
    await palette.getByText("Stop gates").click();

    await expect(palette).toBeHidden();
    await expect(page.getByRole("tab", { name: "Proof" })).toHaveAttribute("aria-selected", "true");
  });

  test("both path fields offer the native folder chooser", async ({ page }) => {
    // Clicking would open a real OS dialog and hang the run, so this asserts
    // the control is present and reachable, not that the dialog appears.
    await expect(page.getByRole("button", { name: "Choose the folder for this loop" })).toBeVisible();

    await page.locator("aside").first().getByRole("button", { name: /Your loops/ }).click();
    await expect(page.getByRole("button", { name: "Browse for a loop folder" })).toBeVisible();
  });

  test("the run buttons stay locked until a loop exists on disk", async ({ page }) => {
    await page.getByRole("tab", { name: "Ship" }).click();
    await expect(page.getByRole("button", { name: "Run once" })).toBeDisabled();
    await expect(page.getByRole("button", { name: "Dry run" })).toBeDisabled();
    // Checking a draft never needs anything on disk.
    await expect(page.getByRole("button", { name: "Check config" })).toBeEnabled();
  });

  test("detection reports what is installed on this machine", async ({ page }) => {
    await expect(page.locator("header").getByText(/agent CLI/)).toBeVisible({ timeout: 15_000 });
  });

  test("the theme toggle wins over the operating system in both directions", async ({ page }) => {
    const root = page.locator("html");
    await page.getByRole("button", { name: "Dark theme" }).click();
    await expect(root).toHaveAttribute("data-theme", "dark");
    await page.getByRole("button", { name: "Light theme" }).click();
    await expect(root).toHaveAttribute("data-theme", "light");
    // "auto" removes the stamp so prefers-color-scheme decides again.
    await page.getByRole("button", { name: /Auto theme/ }).click();
    await expect(root).not.toHaveAttribute("data-theme", /.*/);
  });

  test("no console errors on a clean load", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
    await page.reload();
    await page.waitForTimeout(1500);
    expect(errors).toEqual([]);
  });
});
