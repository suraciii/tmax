// Run with playwright-cli run-code against an already-open local tmax workspace.
// Only uniquely named provider/model/alias fixtures are created. They are removed
// in finally. No credentials, upstream requests, or native files are involved.
async page => {
  const origin = await page.evaluate(() => location.origin);
  const id = 'web-smoke-' + Date.now();
  const model = id + '/test-model';
  const alias = id + '-alias';
  const checks = [];
  const errors = [];
  const onError = error => errors.push(error.message);
  page.on('pageerror', onError);

  function assert(name, condition) {
    checks.push({name, passed: !!condition});
    if (!condition) throw new Error(name);
  }

  async function api(operation, operands = [], args = {}) {
    return page.evaluate(async ({operation, operands, args}) => {
      const response = await fetch('/api/execute', {
        method: 'POST', headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({request: {
          command: operation.split(' '), operands, arguments: args,
          dry_run: false, if_revision: null,
        }, key: null}),
      });
      return response.json();
    }, {operation, operands, args});
  }

  async function configuration() {
    const response = await api('config show');
    assert('Configuration is readable', response.exit_code === 0);
    return response.body.data.configuration;
  }

  async function close() {
    if (await page.locator('#dialog').isVisible()) {
      await page.locator('#dialog-close').click();
    }
  }

  async function navigate(area) {
    await page.locator(`#navigation [data-area="${area}"]`).click();
    await page.waitForFunction(area => document.querySelector('#navigation [aria-current="page"]')?.dataset.area === area, area);
  }

  async function submit(operation, preview = false) {
    const response = page.waitForResponse(r => {
      if (!r.url().endsWith('/api/execute')) return false;
      return r.request().postDataJSON()?.request.command.join(' ') === operation;
    });
    await page.locator(`#action-form button[value="${preview ? 'preview' : 'save'}"]`).click();
    const result = await (await response).json();
    await page.waitForFunction(() => {
      const form = document.querySelector('#action-form');
      return !document.querySelector('#refresh').disabled && (!form || form.dataset.saved === 'true' || !form.querySelector('button[value="save"]').disabled);
    });
    return result;
  }

  try {
    await page.goto(origin);
    await page.waitForFunction(() => document.querySelector('#content')?.getAttribute('aria-busy') === 'false');
    const initial = await configuration();
    assert('Fixture ID is unused', !initial.providers[id]);
    await page.locator('#page-actions [data-action="provider-add"]').click();
    await page.locator('#f-integration').selectOption('deepseek');
    await page.locator('#f-id').fill(id);
    await page.locator('#f-name').fill('Browser smoke fixture');
    const preview = await submit('provider add', true);
    assert('Preview succeeds', preview.exit_code === 0);
    assert('Preview creates no provider', !(await configuration()).providers[id]);
    assert('Provider saves', (await submit('provider add')).exit_code === 0);
    await close();

    await navigate('providers');
    await page.locator(`[data-action="provider-auth"][data-id="${id}"]`).click();
    await page.locator('#f-key').fill('synthetic-preview-only');
    const credentialRequest = page.waitForRequest(r => r.url().endsWith('/api/execute') && r.postDataJSON()?.request.command.join(' ') === 'provider key set');
    assert('Credential preview succeeds', (await submit('provider key set', true)).exit_code === 0);
    const credentialPayload = (await credentialRequest).postDataJSON();
    assert('Preview sends no key or obsolete unlock field', credentialPayload.key === null && !Object.hasOwn(credentialPayload, 'unlock'));
    assert('Credential field clears after preview', await page.locator('#f-key').inputValue() === '');
    assert('Preview leaves authentication unset', !(await configuration()).providers[id].auth);
    await close();

    await navigate('proxy');
    await page.locator('[data-action="proxy-configure"]').first().click();
    assert('Proxy preview succeeds without an unlock password', (await submit('proxy configure', true)).exit_code === 0);
    await close();
    assert('Proxy preview preserves saved configuration', JSON.stringify((await configuration()).proxy) === JSON.stringify(initial.proxy));

    await page.locator('#advanced').click();
    await page.locator('#f-command').fill('skill install');
    await page.locator('[data-action="operation"][data-id="skill install"]').click();
    assert('New upstream skill command is available for preview', (await submit('skill install', true)).exit_code === 0);
    await close();

    await navigate('proxy');
    await page.locator('[data-action="operation"][data-id="network show"]').click();
    assert('Advanced read omits the configuration revision', (await submit('network show')).exit_code === 0);
    await close();

    await navigate('models');
    await page.locator('#page-actions [data-action="model-add"]').click();
    await page.locator('#f-provider').selectOption(id);
    await page.locator('#f-model').fill('test-model');
    assert('Model saves', (await submit('model add')).exit_code === 0);
    await close();
    const savedModel = (await configuration()).providers[id].models['test-model'];
    assert('Explicit empty string survives', savedModel.overrides.name === '');
    assert('Zero priority survives', savedModel.priority === 0);

    await page.locator(`[data-action="model-edit"][data-id="${model}"]`).click();
    await page.locator('#f-enabled').selectOption('false');
    assert('Model disables', (await submit('model update')).exit_code === 0);
    await close();
    assert('False survives', (await configuration()).providers[id].models['test-model'].enabled === false);
    await api('model update', [model], {set: ['enabled', 'true']});
    await page.locator('#refresh').click();
    await page.waitForFunction(() => !document.querySelector('#refresh').disabled);

    await page.locator('[data-action="model-tab"][data-id="alias"]').click();
    await page.locator('#page-actions [data-action="alias-add"]').click();
    await page.locator('#f-id').fill(alias);
    await page.locator(`[name="candidate"][value="${model}"]`).check();
    assert('Explicit candidate alias saves', (await submit('model alias add')).exit_code === 0);
    await close();
    assert('Alias retains exact candidate', (await configuration()).model_aliases.find(a => a.id === alias)?.candidates[0] === model);

    await navigate('providers');
    await page.locator('#search').fill(id);
    assert('Search isolates the fixture', await page.locator('.provider-card').count() === 1);
    await page.locator(`[data-action="provider-detail"][data-id="${id}"]`).click();
    await page.locator('[data-action="provider-edit"]').click();
    await api('provider update', [id], {set: ['overrides.name', 'Concurrent change']});
    await page.locator('#f-name').fill('Stale edit');
    assert('Stale form cannot overwrite newer configuration', (await submit('provider update')).body.code === 'inputs_changed');
    await close();
    assert('Concurrent change is retained', (await configuration()).providers[id].overrides.name === 'Concurrent change');

    await page.setViewportSize({width: 390, height: 844});
    for (const area of ['overview', 'providers', 'models', 'agents', 'proxy']) {
      await navigate(area);
      assert('No horizontal overflow: ' + area, await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    }
    assert('No browser script errors', errors.length === 0);
  } finally {
    await close();
    const config = await configuration();
    if (config.model_aliases.some(a => a.id === alias && a.protocol === 'chat-completions')) {
      assert('Remove alias fixture', (await api('model alias remove', [alias], {protocol: ['chat-completions']})).exit_code === 0);
    }
    if (config.providers[id]?.models['test-model']) {
      assert('Remove model fixture', (await api('model remove', [model])).exit_code === 0);
    }
    if (config.providers[id]) {
      assert('Remove provider fixture', (await api('provider remove', [id])).exit_code === 0);
    }
    page.off('pageerror', onError);
    await page.setViewportSize({width: 1440, height: 1080});
    await page.goto(origin);
  }
  return {checks, errors};
}
