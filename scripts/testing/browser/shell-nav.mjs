export const rail = (page, label) => page.click(`.activity-rail .rail-btn[aria-label="${label}"]`);

export async function setSidebar(page, visible) {
  const shown = await page.locator('.shell-side[aria-hidden="false"]').count();
  if (Boolean(shown) === visible) return;
  await page.click('button[aria-label="Toggle sidebar"]');
  await page.waitForTimeout(400);
}

export const clearNarrow = async (page, width) => { if (Number(width) < 700) await setSidebar(page, false); };

export async function openSet(page, name, width) {
  await setSidebar(page, true);
  await page.click(`.shell-side button:has-text("${name}")`);
  await clearNarrow(page, width);
}
