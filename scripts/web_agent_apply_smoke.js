// Browser regression: optional OpenCode default and a completed application.
// The target declaration is temporary; apply responses are mocked, so no native
// files, credentials or inference requests are involved.
async page => {
  const target = 'web-apply-' + Date.now();
  const checks = [];
  const errors = [];
  const onError = error => errors.push(error.message);
  page.on('pageerror', onError);
  function assert(name, condition) {
    checks.push({name, passed: !!condition});
    if (!condition) throw new Error(name);
  }
  async function api(command, operands, args = {}) {
    return page.evaluate(async ({command, operands, args}) => {
      return (await fetch('/api/execute', {
        method: 'POST', headers: {'Content-Type':'application/json'},
        body: JSON.stringify({request:{command:command.split(' '), operands,
          arguments:args, dry_run:false, if_revision:null}, key:null}),
      })).json();
    }, {command, operands, args});
  }
  let applied = 0;
  let previews = 0;
  const handler = async route => {
    const request = route.request().postDataJSON()?.request;
    if (request?.command.join(' ') !== 'agent apply' || request.operands[0] !== target) {
      return route.continue();
    }
    if (request.dry_run) previews++; else applied++;
    await route.fulfill({json:{exit_code:0, body:{schema_version:1, operation:'agent apply',
      code:request.dry_run?'dry_run_ready':'awaiting_native_reload',
      status:request.dry_run?'dry_run':'success', data:{target,
        state:'applied_reload_pending', effective:'not_verified',
        warnings:request.dry_run?[]:[{code:'native_version_unverified'}]},
    }}});
  };
  let created = false;
  try {
    const added = await api('agent add', [target], {agent:['opencode'], path:[target]});
    assert('Temporary target registered', added.exit_code === 0);
    created = true;
    await page.route('**/api/execute', handler);
    await page.goto((await page.evaluate(() => location.origin)) + '/#agents');
    await page.reload();
    await page.locator(`[data-action="agent-detail"][data-id="${target}"]`).click();
    await page.locator(`[data-action="agent-apply"][data-id="${target}"]`).waitFor();
    assert('Native default can be retained', (await page.locator('#dialog-content').innerText()).includes('沿用 OpenCode 当前选择'));
    assert('Default change is optional', (await page.locator('[data-action="agent-default"]').innerText()).includes('可选'));
    await page.locator('[data-action="agent-apply"]').click();
    assert('No target or revision fields to re-enter', await page.locator('#action-form input').count() === 0);
    assert('No raw command in normal application flow', !(await page.locator('#dialog-content').innerText()).includes('agent apply'));
    await page.locator('button[name="intent"][value="preview"]').click();
    await page.locator('#form-result').getByText('预览完成，没有执行修改。').waitFor();
    assert('Preview never submits an application', previews === 1 && applied === 0);
    await page.locator('button[name="intent"][value="save"]').click();
    await page.locator('#form-result').getByText('配置已写入，请重启或重新加载对应编程工具。').waitFor();
    assert('Application submitted once', applied === 1);
    assert('Reload and version reminders are not errors', await page.locator('#form-result .error').count() === 0);
    assert('Version reminder is localized', (await page.locator('#form-result').innerText()).includes('此工具版本尚未验证'));
    assert('No page errors', errors.length === 0);
    return {checks, errors};
  } finally {
    await page.unroute('**/api/execute', handler);
    if (await page.locator('#dialog').isVisible()) await page.locator('#dialog-close').click();
    if (created) assert('Temporary target removed', (await api('agent remove', [target])).exit_code === 0);
    page.off('pageerror', onError);
    await page.reload();
  }
}
