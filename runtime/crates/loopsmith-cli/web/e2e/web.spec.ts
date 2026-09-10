import { test, expect } from "@playwright/test";

/**
 * These cover the seam: does the page mount, does it reach the API, and do the
 * few interactions that are genuinely browser-side behave. Validation rules,
 * argv construction, secret quoting, and everything else live in Rust tests.
 */

test("the shell mounts and reports the binary's own version", async ({ page }) => {
  // Past the way-in gate, so this is about the shell rather than the overlay.
  await page.addInitScript(() => localStorage.setItem("loopsmith-smith", "experienced"));
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

test("the way in offers both doors, and remembers which was taken", async ({ page }) => {
  await page.goto("/");
  const gate = page.getByRole("dialog", { name: "Choose how to start" });
  await expect(gate).toBeVisible();
  await expect(gate.getByRole("button", { name: /I am an experienced smith/ })).toBeVisible();

  await gate.getByRole("button", { name: /I am a new smith/ }).click();
  await expect(gate).toBeHidden();

  // The answer does not change between launches, so it is asked once.
  await page.reload();
  await expect(page.getByRole("dialog", { name: "Choose how to start" })).toBeHidden();
});

test("an experienced smith is not detained by the tour, but can still open it", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("loopsmith-smith", "experienced"));
  await page.goto("/");
  const tour = page.getByRole("dialog", { name: "How loopsmith works" });
  await expect(tour).toBeHidden();

  await page.getByRole("button", { name: "How this works" }).click();
  await expect(tour).toBeVisible();
});

test("a new smith gets the explanation, and it teaches the rule", async ({ page }) => {
  await page.goto("/");
  await page
    .getByRole("dialog", { name: "Choose how to start" })
    .getByRole("button", { name: /I am a new smith/ })
    .click();

  const tour = page.getByRole("dialog", { name: "How loopsmith works" });
  await expect(tour).toBeVisible();

  await tour.getByRole("button", { name: "Next" }).click();
  // The load-bearing idea. If this panel ever stops saying it, the tour has
  // lost the only thing it exists to teach.
  await expect(tour).toContainText("must not certify its own completion");

  await tour.getByRole("button", { name: "Skip" }).click();
  await expect(tour).toBeHidden();

  // The explanation hands straight over to a working loop to start from.
  await expect(page.getByRole("dialog", { name: "Load an existing loop" })).toBeVisible();
});

test("a reload mid-walk-through comes back to the walk-through", async ({ page }) => {
  await page.goto("/");
  await page
    .getByRole("dialog", { name: "Choose how to start" })
    .getByRole("button", { name: /I am a new smith/ })
    .click();
  await page.getByRole("dialog", { name: "How loopsmith works" })
    .getByRole("button", { name: "Skip" }).click();
  await page.getByRole("dialog", { name: "Load an existing loop" })
    .getByRole("button", { name: /Let's hammer/ }).click();

  await expect(page.getByRole("heading", { name: "What is this loop called?" })).toBeVisible();

  // Dropping someone into the form they were being walked through is the one
  // outcome a reload must not produce.
  await page.reload();
  await expect(page.locator("#guided-panel")).toBeVisible();
});

test("the examples picker starts from nothing unless a loop is chosen", async ({ page }) => {
  await page.goto("/");
  await page
    .getByRole("dialog", { name: "Choose how to start" })
    .getByRole("button", { name: /I am a new smith/ })
    .click();
  await page.getByRole("dialog", { name: "How loopsmith works" })
    .getByRole("button", { name: "Skip" }).click();

  const picker = page.getByRole("dialog", { name: "Load an existing loop" });
  // Starting empty is the deliberate opt-out, and it is what is selected.
  await expect(picker.getByRole("radio", { name: "I will make my own" }))
    .toHaveAttribute("aria-checked", "true");
  await expect(picker.getByRole("radio")).not.toHaveCount(1);

  await picker.getByRole("button", { name: /Let's hammer/ }).click();
  await expect(page.getByRole("heading", { name: "What is this loop called?" })).toBeVisible();
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
    await page.addInitScript(() => {
      localStorage.setItem("loopsmith-tour", "done");
      // Past the way-in gate: these cover the expert editor, which is what an
      // experienced smith is dropped into.
      localStorage.setItem("loopsmith-smith", "experienced");
    });
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

  test("a run survives the tab closing and streams back on reopen", async ({ page }) => {
    // The job is a subprocess of the server, not of this page, so it never
    // stopped — but the page used to forget which one it was watching, which
    // looked identical to it having stopped.
    const started = await page.request.post("/api/jobs", {
      data: { cwd: "/tmp", action: "doctor" },
    });
    expect(started.ok()).toBe(true);
    const { job } = await started.json();

    // A reload is the closest thing to reopening the tab.
    await page.reload();

    // The console reattaches on its own, and the replay means the output is
    // there from the beginning rather than from the moment we rejoined.
    await expect(page.getByText(/Still running|doctor/).first()).toBeVisible({ timeout: 15_000 });

    // And the server agrees the job is its own, independent of any client.
    const seen = await page.request.get(`/api/jobs/${job}`);
    expect(seen.ok()).toBe(true);
    const detail = await seen.json();
    expect(detail.summary.id).toBe(job);
    expect(detail.lines.length).toBeGreaterThan(0);
  });

  test("no console errors on a clean load", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
    await page.reload();
    await page.waitForTimeout(1500);
    expect(errors).toEqual([]);
  });
});

/**
 * The browser half of `loopsmith --guided`: the same questions, in the same
 * order, one card at a time. Entered from the palette here so the cases do not
 * depend on the once-only onboarding path.
 */
test.describe("the guided walk-through", () => {
  const primary = (page: import("@playwright/test").Page) =>
    page.locator("#guided-panel button.btn-primary");

  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.setItem("loopsmith-tour", "done");
      localStorage.setItem("loopsmith-smith", "experienced");
    });
    await page.goto("/");
    await page.keyboard.press("ControlOrMeta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await palette.getByPlaceholder(/Jump to a section/).fill("walk me through");
    await palette.getByText(/Walk me through it/).click();
    await expect(palette).toBeHidden();
  });

  test("asks one field at a time, in the terminal wizard's order", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "What is this loop called?" })).toBeVisible();

    // Exactly one card is ever mounted. A stepper that leaves the steps behind
    // it in the DOM duplicates every field, every element id, and every button
    // that was on them — and it is invisible until something clicks the wrong
    // one.
    await expect(page.locator("#guided-panel h2")).toHaveCount(1);

    await primary(page).click();
    await expect(page.getByRole("heading", { name: "What is it for, in a sentence?" })).toBeVisible();
    await expect(page.locator("#guided-panel h2")).toHaveCount(1);

    // ...and back is a return, not a fresh page.
    await page.getByRole("button", { name: "Previous step" }).click();
    await expect(page.getByRole("heading", { name: "What is this loop called?" })).toBeVisible();
  });

  test("what is typed in the wizard is the same draft the expert editor holds", async ({ page }) => {
    // Scoped to the card: the left rail's example filter is also a textbox.
    await page.locator("#guided-panel input").first().fill("guided-loop");
    await page.getByRole("button", { name: "Expert editor" }).click();

    // Same config, other view. There is deliberately no second draft.
    await expect(page.getByLabel("Loop name", { exact: true }).first()).toHaveValue("guided-loop");
  });

  test("a repeating section accumulates entries and says when it is done", async ({ page }) => {
    // name, description, version, providers, then Goals.
    for (let i = 0; i < 4; i++) await primary(page).click();

    await expect(page.getByRole("heading", { name: "What is this loop trying to achieve?" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Add another goal" })).toBeVisible();
    // The step is not left until it is declared finished, which is what the
    // terminal's Done menu entry means.
    await expect(primary(page)).toHaveText(/This part is done/);
  });

  test("the detector's own fields follow the detector that was chosen", async ({ page }) => {
    for (let i = 0; i < 5; i++) await primary(page).click();
    await expect(page.getByRole("heading", { name: "How is each goal checked?" })).toBeVisible();

    // script is the default, and it asks for a command.
    await expect(page.getByLabel("Command to run", { exact: true })).toBeVisible();

    await page.getByRole("radio", { name: "threshold" }).click();
    await expect(page.getByLabel("Command to run", { exact: true })).toHaveCount(0);
    await expect(page.getByLabel("Metric name", { exact: true })).toBeVisible();
    await expect(page.getByLabel("Threshold value", { exact: true })).toBeVisible();
  });

  test("an advanced section is asked for before it is walked through", async ({ page }) => {
    // Past the five core sections to the first opt-in gate.
    for (let i = 0; i < 13; i++) await primary(page).click();
    await expect(
      page.getByRole("heading", { name: "Define an execution graph of work nodes now?" }),
    ).toBeVisible();

    // Skipping jumps the whole section, exactly as the terminal's gate does.
    await page.getByRole("button", { name: "Skip" }).click();
    await expect(page.getByRole("heading", { name: /static information/ })).toBeVisible();

    await page.getByRole("button", { name: "Previous step" }).click();
    await page.getByRole("button", { name: "Add it" }).click();
    await expect(page.getByRole("heading", { name: "What are the units of work?" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Add another node" })).toBeVisible();
  });

  test("Create is refused while the real validator still reports errors", async ({ page }) => {
    // Walk to the end without answering anything, declining every opt-in
    // section on the way.
    const review = page.getByRole("heading", { name: "Everything checked" });
    for (let i = 0; i < 40 && !(await review.isVisible()); i++) {
      const skip = page.getByRole("button", { name: "Skip", exact: true });
      if (await skip.isVisible()) await skip.click();
      else await primary(page).click();
    }

    await expect(review).toBeVisible();
    // The same verdict the rail has been showing all along, and the same code
    // that decides whether a real run may start.
    await expect(primary(page)).toHaveText(/Create loop/);
    await expect(primary(page)).toBeDisabled();
  });
});
