// Run through playwright-cli against a local tmax Web workspace.
// Discovery responses are controlled to cover failure and request ordering.
// Only a unique target declaration is saved/removed; native files are never applied.
async page => {
  const origin=await page.evaluate(()=>location.origin);
  const checks=[], errors=[], id='discovery-smoke-'+Date.now();
  const path='C:/tmax-discovery-smoke/'+id;
  const assert=(name,ok)=>{checks.push({name,passed:Boolean(ok)});if(!ok)throw Error(name);};
  const onError=error=>errors.push(error.message);
  page.on('pageerror',onError);
  let mode='found', releaseSlow, slowDone;
  const api=async(operation,operands=[],args={})=>page.evaluate(async({operation,operands,args})=>{
    const response=await fetch('/api/execute',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({request:{command:operation.split(' '),operands,arguments:args,dry_run:false,if_revision:null},key:null})});
    return response.json();
  },{operation,operands,args});
  const handler=async route=>{
    const request=route.request().postDataJSON()?.request;
    if(request?.command.join(' ')!=='agent discover')return route.continue();
    const kind=request.arguments.agent[0], selected=mode;
    if(selected==='slow')await new Promise(resolve=>{releaseSlow=resolve;});
    const installation={agent:kind,path:kind==='codex'?path:path+'-'+kind,scope_state:selected==='empty'?'unavailable':'available',profiles:[],installation:{state:'detected',version:'test-version',compatibility:'unknown',qualified_version:'qualified-version'}};
    await route.fulfill({status:200,contentType:'application/json',body:JSON.stringify({exit_code:selected==='error'?4:0,body:{schema_version:1,operation:'agent discover',status:selected==='error'?'error':'success',code:selected==='error'?'probe_failed':'ok',data:{installations:[installation]}}})});
    if(selected==='slow')slowDone=true;
  };
  const open=async()=>{await page.locator('#page-actions [data-action="agent-add"]').click();await page.locator('#agent-rescan:not([disabled])').waitFor();};
  const close=async()=>{if(await page.locator('#dialog').isVisible())await page.locator('#dialog-close').click();};
  try {
    await page.goto(origin+'/#agents');await page.reload();
    await page.waitForFunction(()=>document.querySelector('#content')?.getAttribute('aria-busy')==='false');
    assert('Standalone discovery entry removed',await page.getByRole('button',{name:'发现本地安装',exact:true}).count()===0);
    await page.route('**/api/execute',handler);
    await open();
    assert('Discovery is integrated in add form',await page.locator('[data-discovered-path]').count()===1);
    assert('Version compatibility is explicit',(await page.locator('#agent-discovery').innerText()).includes('配置适配尚未验证'));
    await page.locator('[data-discovered-path]').click();
    assert('Selected path fills form',await page.locator('#f-path').inputValue()===path);
    await page.locator('#f-target').fill(id);
    await page.locator('button[value="preview"]').click();
    await page.locator('#form-result h3').filter({hasText:'变更预览'}).waitFor();
    assert('Preview does not register target',!(await api('config show')).body.data.configuration.agents[id]);
    const saveResponse=page.waitForResponse(r=>r.url().endsWith('/api/execute') && r.request().postDataJSON()?.request.command.join(' ')==='agent add');
    await page.locator('button[value="save"]').click();
    const saveResult=await (await saveResponse).json();
    assert('Target save succeeds',saveResult.exit_code===0);
    await page.getByRole('button',{name:'添加模型来源',exact:true}).waitFor({timeout:5000});
    const saved=(await api('config show')).body.data.configuration.agents[id];
    assert('Save opens model configuration',saved?.agent==='codex');
    assert('No models or native application inferred',Object.keys(saved.providers).length===0 && !saved.default);
    await close();await open();
    assert('Registered path opens existing target',await page.getByRole('button',{name:'打开配置',exact:true}).count()===1);
    assert('Registered candidate is not offered for duplicate addition',await page.locator('[data-discovered-path]').count()===0);
    await page.locator('#f-path').fill(path.toUpperCase().replaceAll('/','\\'));
    await page.locator('button[value="save"]').click();
    await page.locator('#form-result').filter({hasText:'此目录已添加'}).waitFor();
    assert('Windows path spelling cannot bypass duplicate check',true);
    await close();mode='empty';await open();
    assert('Empty result gives manual path guidance',(await page.locator('#agent-discovery').innerText()).includes('下方填写实际目录'));
    assert('Empty result keeps manual entry enabled',await page.locator('#f-path').isEnabled());
    await close();mode='error';await open();
    assert('Failure gives actionable fallback',(await page.locator('#agent-discovery').innerText()).includes('检测未完成'));
    mode='found';await page.locator('#agent-rescan').click();await page.getByRole('button',{name:'打开配置',exact:true}).waitFor();
    assert('Rescan recovers from failure',true);
    mode='slow';await page.locator('#agent-rescan').click();
    await page.waitForFunction(()=>document.querySelector('#agent-rescan').disabled);
    mode='found';await page.locator('#f-agent').selectOption('pi');
    await page.locator('[data-discovered-path]').waitFor();
    await page.locator('#f-path').fill('custom-manual-path');
    const staleResponse=page.waitForResponse(r=>r.url().endsWith('/api/execute') && r.request().postDataJSON()?.request.arguments?.agent?.[0]==='codex');
    releaseSlow();
    // Let the fulfilled stale request and its UI continuation drain.
    await staleResponse;
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    assert('Old scan cannot replace another tool results',(await page.locator('#agent-discovery').innerText()).includes(path+'-pi'));
    assert('Late result preserves manual input',await page.locator('#f-path').inputValue()==='custom-manual-path');
    await page.setViewportSize({width:390,height:844});
    assert('Mobile dialog fits viewport',await page.locator('#dialog').evaluate(el=>el.getBoundingClientRect().left>=0 && el.getBoundingClientRect().right<=innerWidth));
    assert('No script errors',errors.length===0);
    return {checks,errors};
  } finally {
    if(releaseSlow && !slowDone)releaseSlow();
    await page.unroute('**/api/execute',handler);
    await close();
    const config=(await api('config show')).body.data.configuration;
    if(config.agents[id])assert('Fixture removed',(await api('agent remove',[id])).exit_code===0);
    page.off('pageerror',onError);
    await page.setViewportSize({width:1440,height:1080});
    await page.reload();
  }
}
