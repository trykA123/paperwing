// Opens the working-tree compare of the first cloned repository from the repository table and leaves the folder compare on screen.
export async function openFolderCompare(page) {
  await page.waitForSelector('.fm-row[data-id]');
  await page.click('.fm-chip:has-text("Cloned")');
  await page.fill('.rf-search input', 'gateway');
  await page.waitForTimeout(300);
  await page.locator('.fm-row[data-id] .fm-name').first().click();
  await page.click('.side .sec .nav:has-text("Compare")');
  await page.click('.rf-actions-list .btn:has-text("Compare working tree")');
  await page.waitForSelector('.folder-compare');
}
