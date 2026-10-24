'use strict';

// Presentation only: the operation catalog and Rust operation handlers own validation.
// No credentials or configuration snapshots are written to browser storage.
const $ = id => document.getElementById(id);
const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const icons = {
  overview:'<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
  providers:'<rect x="4" y="4" width="16" height="6" rx="2"/><rect x="4" y="14" width="16" height="6" rx="2"/><path d="M8 7h.01M8 17h.01M12 7h5M12 17h5"/>',
  models:'<path d="m12 3 9 5-9 5-9-5 9-5Zm-9 9 9 5 9-5M3 16l9 5 9-5"/>',
  agents:'<rect x="3" y="4" width="18" height="13" rx="2"/><path d="m7 8 3 3-3 3M13 14h4M8 21h8M12 17v4"/>',
  proxy:'<path d="M12 3v4m0 10v4M3 12h4m10 0h4"/><rect x="7" y="7" width="10" height="10" rx="3"/><path d="m10 12 1.5 1.5L14 11"/>',
  settings:'<path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/>',
  plus:'<path d="M12 5v14M5 12h14"/>',
  arrow:'<path d="M5 12h14m-5-5 5 5-5 5"/>',
  refresh:'<path d="M20 7v5h-5M4 17v-5h5M6 6a8 8 0 0 1 13 3M5 15a8 8 0 0 0 13 3"/>',
  search:'<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/>',
  info:'<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/>',
  key:'<circle cx="8" cy="8" r="5"/><path d="m12 12 9 9m-5-5 3-3m-6 0 3-3"/>',
  copy:'<rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V3H3v13h5"/>',
  check:'<path d="m5 12 4 4L19 6"/>',
  history:'<path d="M3 11a9 9 0 1 1 2 7M3 4v7h7M12 7v5l3 2"/>',
};
const icon = name => `<svg viewBox="0 0 24 24" aria-hidden="true">${icons[name] || icons.info}</svg>`;
const areas = {
  overview:['总览','YOUR MODEL WORKSPACE','你的模型、服务商和编程工具，在这里连接。'],
  providers:['服务商','YOUR MODEL PROVIDERS','集中管理你的模型服务，每个账户独立配置。'],
  models:['模型','MODELS & ROUTING','整理可用模型，用明确的候选模型定义自己的路由。'],
  agents:['编程工具','CONNECTED TOOLS','为每个编程工具选择模型，按需应用到本地配置。'],
  proxy:['网关','LOCAL MODEL GATEWAY','一个本地入口，连接已有的模型与应用。'],
};
const agentTypes = [['codex','Codex','C',''],['pi','pi','π','amber'],['omp','omp','o','purple'],['opencode','OpenCode','O','blue'],['hermes','Hermes','H','amber']];
const integrationNames = {'deepseek':'DeepSeek','codex-subscription':'Codex 订阅','opencode-go':'OpenCode Go','kimi-coding-plan':'Kimi Coding Plan','custom':'自定义服务'};
const protocolNames = {'chat-completions':'Chat Completions','responses':'Responses','messages':'Messages'};
const stateLabel = value => ({saved:'已保存',native_reload_and_inference_not_verified:'尚未验证工具加载与实际使用',missing:'暂无信息',unknown:'未知',unavailable:'不可用',absent:'未配置',available:'可用',fresh:'有效',stale:'已过期',expired:'已过期',unverified:'未验证',not_verified:'未验证',configured:'已配置',ready:'已就绪',loaded:'已加载',not_applied:'尚未应用',applied:'已应用',incomplete:'待完善',complete:'已完整配置',unchanged:'无变化',mismatch:'与当前配置不一致',matches:'与配置一致',drifted:'存在外部修改',awaiting_native_reload:'等待工具重新加载'}[value] || '未知');
const state = {area:'overview', catalog:[], config:null, providers:[], models:[], agents:[], runtime:null, integrations:[], errors:[], revision:null, loaded:false, generation:0, modelKind:'concrete', query:'', busy:false, dialogGeneration:0};
let toastTimer;
let revealGeneration=0;
const badge = (text, tone='') => `<span class="badge ${tone}">${escapeHtml(text)}</span>`;
const button = (label, action, id='', variant='', glyph='') => `<button type="button" class="btn ${variant}" data-action="${escapeHtml(action)}" data-id="${escapeHtml(id)}">${glyph ? icon(glyph) : ''}${escapeHtml(label)}</button>`;
const fact = (label, value) => `<div class="fact"><span>${escapeHtml(label)}</span><span>${escapeHtml(value ?? '未知')}</span></div>`;
const notice = (message, tone='') => `<div class="notice ${tone}">${icon('info')}<div>${message}</div></div>`;
const detail = (value, label='查看技术详情') => `<details><summary>${escapeHtml(label)}</summary><pre>${escapeHtml(JSON.stringify(value, null, 2))}</pre></details>`;
const panel = (title, subtitle, body, action='') => `<section class="panel"><div class="panel-header"><div><h2>${title}</h2>${subtitle ? `<p>${subtitle}</p>` : ''}</div>${action}</div><div class="panel-body">${body}</div></section>`;
const empty = (title, message, action='', glyph='models', compact=false) => `<div class="empty ${compact?'compact':''}"><div class="empty-icon">${icon(glyph)}</div><h3>${title}</h3><p>${message}</p>${action}</div>`;
const providerName = id => state.config?.providers?.[id]?.overrides?.name || id;
const serviceName = p => integrationNames[p?.integration] || p?.integration || p?.metadata || '未指定服务';
const dateValue = value => typeof value === 'string' ? value : value?.$__toml_private_datetime || null;
const modelConfig = id => { const slash=id.indexOf('/'); return state.config?.providers?.[id.slice(0,slash)]?.models?.[id.slice(slash+1)]; };
const isSuccess = response => response?.exit_code === 0 && ['success','dry_run'].includes(response.body?.status);

async function http(path, payload) {
  let response;
  try {
    response = await fetch(path, {method:payload === undefined?'GET':'POST', credentials:'omit', cache:'no-store', headers:payload === undefined?{}:{'Content-Type':'application/json'}, body:payload === undefined?undefined:JSON.stringify(payload)});
  } catch (_) { throw new Error('无法连接本地服务。请检查 tmax 是否仍在运行；如果刚提交了修改，请刷新确认结果后再操作。'); }
  const text = await response.text();
  let value;
  try { value = JSON.parse(text); }
  catch (_) { throw new Error(text === 'Proxy not configured' ? '请先配置网关，再查看共享密钥。' : `本地请求未完成（${response.status}）。${text.slice(0,200)}`); }
  if (!response.ok) throw new Error(value.error?.message || `本地请求被拒绝（${response.status}）`);
  return value;
}
async function execute(operation, operands=[], args={}, options={}) {
  const value = await http('/api/execute', {request:{command:operation.split(' '), operands, arguments:args, dry_run:!!options.preview, if_revision:options.revision ?? null}, key:options.preview?null:options.key || null});
  if (!Number.isInteger(value.exit_code) || value.body?.schema_version !== 1 || value.body?.operation !== operation || typeof value.body?.status !== 'string' || !Object.hasOwn(value.body, 'data')) throw new Error('本地服务返回了无法识别的结果，请刷新重试。');
  return value;
}
async function read(operation, operands=[], args={}) {
  const response = await execute(operation, operands, args);
  if (!isSuccess(response)) throw new Error(response.body?.blockers?.[0]?.message || response.body?.code || '读取失败');
  return response.body.data;
}
function toast(message) {
  clearTimeout(toastTimer); $('toast').textContent=message; $('toast').hidden=false;
  toastTimer=setTimeout(() => {$('toast').hidden=true;}, 4500);
}
async function refresh() {
  const generation=++state.generation;
  $('refresh').disabled=true; $('content').setAttribute('aria-busy','true');
  const operations=['status','config show','provider list','model list','agent list','integration list'];
  const responses=await Promise.allSettled(operations.map(op => read(op)));
  if (generation !== state.generation) return;
  state.errors=responses.flatMap((r,i) => r.status==='rejected'?[`${operations[i]}：${r.reason.message}`]:[]);
  const values=responses.map(r => r.status==='fulfilled'?r.value:null);
  state.runtime=values[0]?.resident || null; state.config=values[1]?.configuration || null; state.revision=values[1]?.configuration_revision || null;
  state.providers=values[2]?.providers ?? null; state.models=values[3]?.models ?? null; state.agents=values[4]?.agents ?? null; state.integrations=values[5]?.integrations || [];
  state.loaded=true;
  $('connection').innerHTML=`<span class="dot ${state.runtime?'':'off'}"></span>${state.runtime?'本地已连接':'连接不可用'}`;
  $('load-error').hidden=!state.errors.length;
  $('load-error').innerHTML=state.errors.length?notice('部分信息读取失败，请刷新重试。'+detail(state.errors),'error'):'';
  $('updated').textContent='最近读取 '+new Date().toLocaleTimeString('zh-CN',{hour:'2-digit',minute:'2-digit'})+' · 不自动请求上游';
  $('refresh').disabled=false; $('content').setAttribute('aria-busy','false'); render();
}
function navigate(area) {
  if (!areas[area]) area='overview';
  state.area=area; state.query=''; clearReveal();
  if ($('dialog').open) $('dialog').close();
  const [title,eyebrow,description]=areas[area];
  $('breadcrumb').textContent=title; $('page-title').textContent=title; $('eyebrow').textContent=eyebrow; $('page-description').textContent=description;
  document.querySelectorAll('#navigation a').forEach(a => {if(a.dataset.area===area)a.setAttribute('aria-current','page');else a.removeAttribute('aria-current');});
  render();
}
function render() {
  if (!state.loaded) return;
  const actions={overview:button('添加服务商','provider-add','','primary','plus'),providers:button('添加服务商','provider-add','','primary','plus'),models:button(state.modelKind==='alias'?'新建别名':'添加模型',state.modelKind==='alias'?'alias-add':'model-add','','primary','plus'),agents:button('添加工具配置','agent-add','','primary','plus'),proxy:button('网关设置','proxy-configure','','','settings')};
  $('page-actions').innerHTML=actions[state.area];
  if (!state.config) { $('content').innerHTML=empty('暂时无法读取配置','请检查本地配置或使用高级操作查看校验结果。',button('校验配置','operation','config validate'),'info'); return; }
  ({overview:renderOverview,providers:renderProviders,models:renderModels,agents:renderAgents,proxy:renderProxy}[state.area])();
}
function metric(label, value, sub, glyph, status=false) {
  return `<div class="metric"><div class="metric-top">${label}${icon(glyph)}</div><div class="metric-value ${status?'status-value':''}">${escapeHtml(value)}</div><div class="metric-bottom">${escapeHtml(sub)}</div></div>`;
}
function authBadge(provider) {
  if (!provider.enabled) return badge('已停用');
  if (provider.authentication==='not_configured') return badge('待授权','amber');
  return badge('已配置授权','green');
}
function renderOverview() {
  const providers=state.providers, models=state.models, agents=state.agents;
  const running=state.runtime?.resident?.inference?.state==='running';
  const ready=state.runtime?.proxy?.ready===true;
  const needsAuth=(providers || []).filter(p=>p.enabled && p.authentication==='not_configured');
  const configuredProviders=Object.entries(state.config.providers);
  const upcoming=configuredProviders.filter(([,p])=>dateValue(p.subscription?.renewal_date)).sort((a,b)=>dateValue(a[1].subscription.renewal_date).localeCompare(dateValue(b[1].subscription.renewal_date)));
  const hero=`<section class="hero"><div><p class="eyebrow">ONE PLACE. YOUR CHOICE.</p><h2>${providers?.length?'让每个工具，用上合适的模型。':'从一个服务商，开始你的工作台。'}</h2><p>${providers?.length?'集中整理模型服务，为不同工具选择所需模型。所有更改由你决定。':'添加已有的模型服务，再为编程工具选择模型。配置一次，在需要的地方使用。'}</p>${button(providers?.length?'管理编程工具':'添加第一个服务商',providers?.length?'navigate':'provider-add',providers?.length?'agents':'','primary','arrow')}</div><div class="hero-art" aria-hidden="true"><div class="art-stack"><div class="art-chip">模型服务</div><div class="art-chip">订阅计划</div></div><div class="art-hub">t↗</div><div class="art-stack"><div class="art-chip">Codex</div><div class="art-chip">OpenCode</div></div></div></section>`;
  const providerBody=providers===null?empty('服务商暂不可用','请刷新后重试。'):providers.length?providers.slice(0,5).map(p=>`<div class="agent-row"><div class="avatar blue">${escapeHtml(serviceName(state.config.providers[p.id]).slice(0,1))}</div><div class="grow"><h3>${escapeHtml(providerName(p.id))}</h3><p>${escapeHtml(serviceName(state.config.providers[p.id]))} · ${Object.keys(state.config.providers[p.id]?.models || {}).length} 个模型</p></div>${authBadge(p)}${button('管理','provider-detail',p.id,'small')}</div>`).join(''):empty('还没有添加服务商','接入你的 API 服务或受支持的订阅，模型会在这里集中管理。',button('添加服务商','provider-add','','soft','plus'),'providers',true);
  const agentsBody=agentTypes.map(([id,name,mark,tone])=>{const targets=(agents || []).filter(t=>t.agent===id);return `<div class="agent-row"><div class="avatar ${tone}">${mark}</div><div class="grow"><h3>${name}</h3><p>${targets.length?`${targets.length} 个已保存配置 · 原生状态需检查`:'添加配置后选择模型'}</p></div>${button(targets.length?'管理':'设置',targets.length?'navigate':'agent-add',targets.length?'agents':id,'small')}</div>`;}).join('');
  const renewals=upcoming.length?upcoming.map(([id,p])=>{const date=dateValue(p.subscription.renewal_date);const today=new Date().toLocaleDateString('en-CA');return `<div class="list-row split"><div><h3>${escapeHtml(providerName(id))}</h3><small>${escapeHtml(p.subscription.plan || '手动记录的日期')} · 计费结果未知</small></div>${badge(date+(date<today?' · 日期已过':''),date<today?'amber':'')}</div>`;}).join(''):'<p class="muted">暂无已知续费日期。可在服务商详情中记录计划和日期。</p>';
  const issues=needsAuth.length?notice(`${needsAuth.length} 个服务商尚未配置授权。可以继续管理模型，使用前请完成授权。`,'warning'):'';
  $('content').innerHTML=hero+`<div class="metrics">${metric('服务商',providers?.length ?? '—','独立管理每一份模型访问','providers')}${metric('模型',models?.filter(m=>m.kind==='concrete').length ?? '—',`${models?.filter(m=>m.kind==='alias').length ?? '—'} 个自定义别名`,'models')}${metric('工具配置',agents?.length ?? '—','保存与原生应用分别展示','agents')}${metric('网关',!state.runtime?'未知':running?(ready?'运行中':'运行中 · 待就绪'):'未启动',running?'仅转发已支持的原生协议':'需要时再启动本地网关','proxy',true)}</div>`+issues+`<div class="grid-two">${panel('我的服务商','同一服务的不同账户，各自独立',providerBody,button('查看全部','navigate','providers','text small','arrow'))}${panel('编程工具','为你熟悉的工具选择模型',agentsBody)}</div><div class="steps"><button class="step" data-action="provider-add"><span class="step-number">01</span><span><strong>接入模型服务</strong><small>选择服务，独立管理授权</small></span></button><button class="step" data-action="navigate" data-id="models"><span class="step-number">02</span><span><strong>整理模型与别名</strong><small>选择模型，明确路由候选</small></span></button><button class="step" data-action="navigate" data-id="agents"><span class="step-number">03</span><span><strong>连接你的编程工具</strong><small>选择模型，再按需应用</small></span></button></div><div class="section-label"><h2>订阅与续费</h2><p>来自已保存的信息</p></div>${panel('已知续费日期','不推断扣款、剩余额度或续费结果',renewals)}`;
}
function searchBox(placeholder) { return `<div class="search">${icon('search')}<input id="search" type="search" aria-label="${placeholder}" placeholder="${placeholder}" value="${escapeHtml(state.query)}"></div>`; }
function renderProviders() {
  $('content').innerHTML=`<div class="toolbar">${searchBox('搜索服务商名称或 ID')}<span class="muted">${state.providers?.length ?? '—'} 个服务商</span></div><div id="provider-list"></div>`;
  renderProviderList();
  $('search').addEventListener('input',e=>{state.query=e.target.value;renderProviderList();});
}
function renderProviderList() {
  if (!state.providers) { $('provider-list').innerHTML=empty('服务商列表读取失败','点击右上角刷新重试。'); return; }
  const filtered=state.providers.filter(p=>`${p.id} ${providerName(p.id)} ${serviceName(state.config.providers[p.id])}`.toLowerCase().includes(state.query.toLowerCase()));
  if (!filtered.length) { $('provider-list').innerHTML=`<div class="panel">${empty(state.query?'没有匹配的服务商':'接入你的第一个模型服务',state.query?'试试其他名称或 ID。':'选择已有服务或自定义端点。授权、模型和订阅信息会集中在这里。',state.query?'':button('添加服务商','provider-add','','primary','plus'),'providers')}</div>`; return; }
  const groups=Map.groupBy?Map.groupBy(filtered,p=>serviceName(state.config.providers[p.id])):filtered.reduce((m,p)=>{const k=serviceName(state.config.providers[p.id]);m.set(k,[...(m.get(k)||[]),p]);return m;},new Map());
  $('provider-list').innerHTML=[...groups].map(([name,providers])=>`<div class="section-label"><h2>${escapeHtml(name)}</h2><p>${providers.length} 个独立实例</p></div><div class="cards">${providers.map(p=>{const config=state.config.providers[p.id];const observation=p.observations?.observations?.find(o=>o.scope==='quota');return `<article class="provider-card"><div class="card-top"><div class="avatar blue">${escapeHtml(name.slice(0,1))}</div><div class="grow"><h3>${escapeHtml(providerName(p.id))}</h3><p class="mono">${escapeHtml(p.id)}</p></div>${authBadge(p)}</div><div class="card-facts">${fact('模型',Object.keys(config.models || {}).length+' 个')}${fact('订阅计划',config.subscription?.plan || '未记录')}${fact('额度信息',observation?.last_attempt_outcome==='failed'?'最近读取失败':stateLabel(observation?.freshness?.state))}${fact('使用此服务的工具',p.affected_targets+' 个配置')}</div><div class="card-actions">${button('管理服务商','provider-detail',p.id,'small')}${button(p.authentication==='not_configured'?'配置授权':'更新授权','provider-auth',p.id,'soft small','key')}</div></article>`;}).join('')}</div>`).join('');
}
function renderModels() {
  $('content').innerHTML=`<div class="toolbar"><div class="tabs" role="tablist" aria-label="模型类型"><button class="tab" role="tab" aria-selected="${state.modelKind==='concrete'}" data-action="model-tab" data-id="concrete">具体模型</button><button class="tab" role="tab" aria-selected="${state.modelKind==='alias'}" data-action="model-tab" data-id="alias">模型别名</button></div>${searchBox('搜索模型或服务商')}</div>${state.modelKind==='alias'?notice('为明确选择的候选模型取一个固定名称。别名仅用于网关，不自动改变工具的直接连接。'):''}<div id="model-list"></div>`;
  renderModelList(); $('search').addEventListener('input',e=>{state.query=e.target.value;renderModelList();});
}
function renderModelList() {
  if (!state.models) {$('model-list').innerHTML=empty('模型列表读取失败','刷新后重试。');return;}
  const rows=state.models.filter(m=>m.kind===state.modelKind && m.id.toLowerCase().includes(state.query.toLowerCase()));
  if (!rows.length) {$('model-list').innerHTML=`<div class="panel">${empty(state.query?'没有匹配的模型':state.modelKind==='alias'?'为你的使用场景创建别名':'还没有登记模型',state.query?'试试其他关键词。':state.modelKind==='alias'?'例如为日常编码选择一组具体候选模型。':'添加服务商后，登记要使用的上游模型。',state.query?'':button(state.modelKind==='alias'?'新建别名':'添加模型',state.modelKind==='alias'?'alias-add':'model-add','','primary','plus'))}</div>`;return;}
  $('model-list').innerHTML=`<div class="panel table-wrap"><table class="data-table"><thead><tr><th>模型 / 名称</th><th>${state.modelKind==='alias'?'候选模型':'服务商'}</th><th>原生协议</th><th>状态</th><th>操作</th></tr></thead><tbody>${rows.map(m=>{const config=m.kind==='concrete'?modelConfig(m.id):state.config.model_aliases.find(a=>a.id===m.id&&a.protocol===m.protocol);return `<tr><td><strong>${escapeHtml(config?.overrides?.name || m.id)}</strong><small class="mono">${escapeHtml(m.id)}</small></td><td>${escapeHtml(m.kind==='alias'?`${config?.candidates?.length || 0} 个明确候选`:providerName(m.id.split('/')[0]))}</td><td><div class="protocols">${(m.published_protocols || []).map(p=>badge(protocolNames[p] || p)).join('') || badge('未发布')}</div></td><td>${!m.enabled?badge('已停用'):m.availability==='authentication_missing'?badge('待授权','amber'):badge(m.kind==='alias'?'已配置':'待验证')}</td><td>${button('管理',m.kind==='alias'?'alias-edit':'model-edit',m.kind==='alias'?JSON.stringify([m.id,m.protocol]):m.id,'small')}</td></tr>`;}).join('')}</tbody></table></div>`;
}
function renderAgents() {
  $('content').innerHTML=notice('保存工具配置后，可先预览影响，再主动应用。保存不会自动改写工具文件。')+`<div class="cards">${agentTypes.map(([id,name,mark,tone])=>{const targets=(state.agents || []).filter(a=>a.agent===id);return `<section class="agent-card"><div class="card-top"><div class="avatar ${tone}">${mark}</div><div class="grow"><h3>${name}</h3><p>${targets.length} 个已保存配置</p></div>${badge(targets.length?'已登记':'未配置',targets.length?'green':'')}</div>${targets.length?targets.map(t=>`<div class="list-row"><strong>${escapeHtml(t.target)}</strong><small class="label-note mono">${escapeHtml(t.path)}</small><div class="detail-facts">${fact('默认模型',state.config.agents[t.target]?.default?.id || '尚未选择')}${fact('原生应用状态','需要检查')}</div>${button('模型与应用','agent-detail',t.target,'small soft','arrow')}</div>`).join(''):'<p class="muted">添加此工具的本地配置，为它选择模型。</p>'}<div class="actions">${button('添加配置','agent-add',id,'small','plus')}${button('发现本地安装','agent-discover',id,'text small')}</div></section>`;}).join('')}</div>`;
}
function clearReveal() { revealGeneration++;const output=$('revealed'); if(output){output.textContent='';output.hidden=true;} }
function renderProxy() {
  const runtime=state.runtime; const proxy=state.config.proxy; const inference=runtime?.resident?.inference;
  const running=inference?.state==='running'; const address=inference?.url || (proxy?.listen?'http://'+proxy.listen:null);
  const configured=!!proxy;
  $('content').innerHTML=`<div class="grid-two">${panel('网关','供支持相同协议的应用连接',`<div class="service-status"><div class="status-symbol">${icon('proxy')}</div><div><h3>${!runtime?'状态未知':running?'网关运行中':configured?'已配置 · 未启动':'尚未配置'}</h3><p>${running?(runtime.proxy?.ready?'服务报告已就绪':'服务尚未就绪，请检查配置与授权'):'需要通过网关使用模型时，再启动服务。'}</p></div></div>${address?`<div class="big-address"><code>${escapeHtml(address)}</code>${button('复制','copy',address,'small','copy')}</div>`:notice('先设置本地监听地址，保存后会生成共享密钥。')}${`<div class="actions">${button(configured?(running?'停止网关':'启动网关'):'配置网关',configured?'operation':'proxy-configure',configured?(running?'proxy stop':'proxy start'):'','primary',running?'proxy':'arrow')}${configured?button('修改设置','proxy-configure','','','settings'):''}</div>`}`)}${panel('连接与授权','共享密钥仅在主动查看时显示',`<div class="detail-facts">${fact('已发布模型',runtime?.publication?.model_count ?? '未知')}${fact('进行中的请求',runtime?.active_requests ?? '未知')}${fact('登录时启动',runtime?.autostart?.login_enabled?'已注册':'未注册')}${fact('共享密钥状态',stateLabel(runtime?.shared_key?.loaded_state))}</div>${configured?`<div class="actions">${button('查看共享密钥','reveal','','','key')}${button('隐藏','hide-key','','small')}</div><div id="revealed" class="key-output mono" hidden></div>`:notice('配置网关后，可以在这里查看用于连接的共享密钥。')}`)}</div><div class="section-label"><h2>请求记录</h2>${button('读取记录','history','','small','history')}</div><div id="history-panel" class="panel">${empty('按需查看本地请求','仅记录经过此网关的请求，不包含直接连接流量，也不代表账户全部用量。',button('读取请求记录','history','','soft'),'history',true)}</div><div class="section-label"><h2>其他设置</h2></div><div class="actions">${button('登录时启动设置','autostart','','','settings')}${button('查看出站网络','operation','network show','','settings')}${button('网关高级操作','advanced','proxy','','settings')}</div>`;
}

function openDialog(title, body, eyebrow='WORKSPACE') {
  state.dialogGeneration++;
  $('dialog-title').textContent=title; $('dialog-eyebrow').textContent=eyebrow; $('dialog-content').innerHTML=body;
  if (!$('dialog').open) $('dialog').showModal();
  $('dialog-content').querySelector('input:not([type=hidden]),select,button')?.focus();
  return state.dialogGeneration;
}
function field(name, label, value='', options={}) {
  const attributes=`id="f-${escapeHtml(name)}" name="${escapeHtml(name)}" ${options.required?'required':''} ${options.readonly?'readonly':''} autocomplete="off"`;
  let control;
  if (options.options) control=`<select ${attributes}>${options.options.map(o=>{const [v,t]=Array.isArray(o)?o:[o,o];return `<option value="${escapeHtml(v)}" ${String(value)===String(v)?'selected':''}>${escapeHtml(t)}</option>`;}).join('')}</select>`;
  else if(options.multiline) control=`<textarea ${attributes} placeholder="${escapeHtml(options.placeholder || '')}">${escapeHtml(value)}</textarea>`;
  else control=`<input ${attributes} type="${options.type || 'text'}" value="${escapeHtml(value)}" placeholder="${escapeHtml(options.placeholder || '')}">`;
  return `<label class="field"><span>${escapeHtml(label)}${options.required?' <span class="required">*</span>':''}</span>${control}${options.help?`<small>${escapeHtml(options.help)}</small>`:''}</label>`;
}
const value = name => $('f-'+name)?.value ?? '';
function resultMessage(response) {
  const body=response.body;
  const known={unsupported_protected_input:'当前 Windows 版本无法接收受保护输入。配置尚未完成，需要修复运行时后再试。',inputs_changed:'配置已被其他操作更新。请关闭此表单，刷新页面后重新编辑。',target_incomplete:'工具配置尚未完整。请添加来源和模型，并选择默认模型。',credential_unavailable:'授权凭据不可用，请检查服务商授权配置。',saved_not_active:'配置已保存，但未生效。请检查原因后重新加载配置。',awaiting_native_reload:'配置已写入，请重启或重新加载对应编程工具。',unresolved_reference:'还有其他配置引用此项。请先调整相关模型、别名或工具配置。',active_requests:'仍有请求进行中。请等待请求结束后再停止。',restart_required:'配置需要重启网关才能生效。',login_source_required:'请选择负责登录的工具配置。',interactive_required:'此操作需要交互登录，请通过本地命令行完成登录后刷新状态。'};
  return known[body.code] || body.blockers?.[0]?.message || (isSuccess(response)?(body.status==='dry_run'?'预览完成，没有执行修改。':'操作已完成。'):'操作未完成，请查看原因。');
}
function presentResult(response, container) {
  const body=response.body; const success=isSuccess(response); const warnings=[...(body.warnings || []),...(body.data?.warnings || [])];
  const next=body.next_actions || body.data?.next_actions || [];
  const hasWarning=warnings.length>0 || ['failed','blocked','unavailable'].includes(body.data?.activation?.activation || body.data?.activation?.state);
  const warningText=w=>({external_availability_unverified:'预览不会读取密钥或验证上游授权。',listener_restart_required:'配置已保存，需要主动重启网关后生效。',saved_not_active:'配置已保存，运行时尚未生效；请检查并重新加载，不要重复创建。'}[w.code] || (typeof w==='string'?w:w.message || w.code));
  const changes=body.changes || body.data?.changes || [];
  const preview=body.status==='dry_run'&&changes.length?`<p>涉及 ${changes.length} 项变更：</p><ul>${changes.slice(0,8).map(c=>`<li>${escapeHtml(({add:'新增',remove:'移除',replace:'更新'}[c.kind] || '更改')+' · '+(c.path || c.resource?.id || '当前配置'))}</li>`).join('')}</ul>${changes.length>8?'<p>其余变更见完整详情。</p>':''}`:'';
  container.innerHTML=`<div class="result-box ${!success||hasWarning?'error':''}" role="status"><h3>${body.status==='dry_run'?'变更预览':success?(hasWarning?'已完成，请留意提示':'操作完成'):(body.status==='incomplete'?'部分完成，需要后续操作':'操作未完成')}</h3><p>${escapeHtml(resultMessage(response))}</p>${body.data?.save==='saved'?'<p>设置已保存。原生工具文件是否应用，请以应用结果为准。</p>':''}${preview}${warnings.length?`<ul>${warnings.map(w=>`<li>${escapeHtml(warningText(w))}</li>`).join('')}</ul>`:''}${next.length?`<ul>${next.map(a=>`<li>${escapeHtml(typeof a==='string'?a:a.message || a.operation || a.command || JSON.stringify(a))}</li>`).join('')}</ul>`:''}${detail(response)}</div>`;
}
function formDialog(title, description, fields, build, options={}) {
  const revision=state.revision;
  const generation=openDialog(title,`<p class="form-intro">${description}</p><form id="action-form">${fields}<div class="form-actions">${button('取消','close')}${options.preview===false?'':'<button class="btn" type="submit" name="intent" value="preview" formnovalidate>预览影响</button>'}<button class="btn primary" type="submit" name="intent" value="save">${options.submit || '保存'}</button></div><div id="form-result" aria-live="polite"></div></form>`,options.eyebrow || 'CONFIGURATION');
  $('action-form').addEventListener('submit',async event=>{
    event.preventDefault(); if(state.busy)return;
    const preview=event.submitter?.value==='preview'; let request;
    if(preview && [...$('action-form').elements].some(input=>input.type!=='password' && input.reportValidity && !input.reportValidity()))return;
    try {request=build();}catch(error){$('form-result').innerHTML=notice(escapeHtml(error.message),'error');return;}
    const optionsForRequest={...request.options,preview,revision:Object.hasOwn(request.options || {},'revision')?request.options.revision:revision};
    // Remove protected values before awaiting any request; previews never send them.
    document.querySelectorAll('#dialog input[type=password]').forEach(input=>{input.value='';});
    if(preview){optionsForRequest.key=null;}
    state.busy=true; $('action-form').querySelectorAll('button').forEach(b=>b.disabled=true);
    try {
      const response=await execute(request.operation,request.operands || [],request.args || {},optionsForRequest);
      if(generation!==state.dialogGeneration)return;
      presentResult(response,$('form-result'));
      if(!preview) await refresh();
      const needsAttention=response.body.warnings?.length || response.body.data?.warnings?.length || ['failed','blocked','unavailable'].includes(response.body.data?.activation?.activation || response.body.data?.activation?.state);
      if(isSuccess(response)&&!preview&&!options.stayOpen&&!needsAttention){$('dialog').close();toast(options.success || '设置已保存');}
      else if(isSuccess(response)&&!preview){$('action-form').dataset.saved='true';}
    } catch(error) {if(generation===state.dialogGeneration)$('form-result').innerHTML=notice(escapeHtml(error.message),'error');}
    finally {request=null;optionsForRequest.key=null;state.busy=false;if(generation===state.dialogGeneration)$('action-form')?.querySelectorAll('button').forEach(b=>{b.disabled=b.value==='save' && $('action-form').dataset.saved==='true';});}
  });
  return generation;
}

function suggestedId(base) {
  const ids=state.config?.providers || {}; if(!Object.hasOwn(ids,base))return base;
  let n=2;while(Object.hasOwn(ids,`${base}-${n}`))n++;return `${base}-${n}`;
}
async function addProvider() {
  const generation=formDialog('添加服务商','选择服务元数据，或明确使用自定义配置。先保存服务商，再单独配置授权。',
    field('setup','配置方式','custom',{options:[['catalog','从 models.dev 目录选择'],['custom','手动选择服务 / 自定义端点']]})+
    '<div id="catalog-fields" hidden><div id="catalog-selector"></div></div>'+
    field('integration','服务类型',state.integrations.find(i=>i.id==='deepseek')?'deepseek':'custom',{options:state.integrations.map(i=>[i.id,integrationNames[i.id] || i.id]),required:true})+
    '<div id="endpoint-fields"></div><div class="form-grid">'+field('id','本地 ID',suggestedId('deepseek'),{required:true,help:'用于模型引用，可使用自己的名称。'})+field('name','显示名称','',{placeholder:'例如：我的 DeepSeek'})+'</div>',()=>{
      const args={integration:[value('integration')]};
      if(value('setup')==='catalog'){if(!value('metadata'))throw Error('请先选择目录中的服务商。');args.metadata=[value('metadata')];}
      if(value('name'))args.set=['overrides.name',value('name')];
      if($('f-region'))args.region=[value('region')];
      const endpoints=[];document.querySelectorAll('[data-endpoint]').forEach(input=>{if(input.value.trim())endpoints.push(input.dataset.endpoint,input.value.trim());});
      if(endpoints.length)args.endpoint=endpoints;
      return {operation:'provider add',operands:[value('id').trim()],args};
    },{success:'服务商已添加。接下来可以配置授权和模型。'});
  let idEdited=false; $('f-id').addEventListener('input',()=>{idEdited=true;});
  const updateIntegration=()=>{
    const integration=state.integrations.find(i=>i.id===value('integration'));
    if(!idEdited)$('f-id').value=suggestedId(value('setup')==='catalog'&&value('metadata')?value('metadata'):value('integration'));
    const support=integration?.endpoint_support;
    if(support?.kind==='custom')$('endpoint-fields').innerHTML=notice('为需要使用的协议填写对应基础地址，至少填写一个。')+(integration.protocols || []).map(p=>field('endpoint-'+p,protocolNames[p]+' 基础地址','',{type:'url',placeholder:'https://api.example.com/v1'})).join('');
    else $('endpoint-fields').innerHTML=(support?.regions?.length>1?field('region','服务区域','',{required:true,options:[['','请选择区域'],...support.regions.map(r=>[r.id,r.id])]}):'')+notice('基础地址与原生协议使用此服务的内置配置。'+(support?.regions?.[0]?.endpoints?'<br>'+Object.entries(support.regions[0].endpoints).map(([p,url])=>`${escapeHtml(protocolNames[p])} · <span class="mono">${escapeHtml(url)}</span>`).join('<br>'):''));
    document.querySelectorAll('[id^="f-endpoint-"]').forEach(input=>input.dataset.endpoint=input.id.slice('f-endpoint-'.length));
  };
  $('f-integration').addEventListener('change',updateIntegration);updateIntegration();
  let catalogLoaded=false;
  $('f-setup').addEventListener('change',async()=>{
    const selected=value('setup')==='catalog';$('catalog-fields').hidden=!selected;
    if(!selected || catalogLoaded)return;
    $('catalog-selector').innerHTML='<p class="form-intro">正在读取本地目录…</p>';
    try {const data=await read('catalog providers');if(generation!==state.dialogGeneration)return;const providers=data.providers || [];
      $('catalog-selector').innerHTML=providers.length?field('metadata','目录服务商','',{options:[['','请选择'],...providers.map(p=>[p.id,p.name || p.id])],help:'只读取本地目录，不自动下载或更新。'}):notice('本地目录暂无条目。可使用手动配置，或在高级操作中刷新并接受目录。','warning');
      $('f-metadata')?.addEventListener('change',()=>{
        const selected=providers.find(p=>p.id===value('metadata'));
        const integration=selected?.integration_choices?.[0]?.integration;
        if(integration && state.integrations.some(i=>i.id===integration))$('f-integration').value=integration;
        updateIntegration();
      });catalogLoaded=true;
    }catch(error){if(generation===state.dialogGeneration)$('catalog-selector').innerHTML=notice(escapeHtml(error.message)+'。可以改用手动配置。','warning');}
  });
}

function editProvider(id) {
  const p=state.config.providers[id];if(!p)return;
  formDialog('编辑服务商',`编辑 ${escapeHtml(id)} 的显示和订阅信息。留空会恢复名称继承或清除对应的手动订阅信息。`,
    field('name','显示名称',p.overrides?.name || '',{help:'不改变本地 ID 或模型引用。'})+
    '<div class="form-grid">'+field('plan','订阅计划',p.subscription?.plan || '')+field('renewal','续费日期',dateValue(p.subscription?.renewal_date) || '',{type:'date'})+'</div>'+field('enabled','服务状态',String(p.enabled),{options:[['true','启用'],['false','停用']]}),()=>{
      const args={set:['enabled',value('enabled')],clear:[]};
      for(const [fieldName,v] of [['overrides.name',value('name')],['subscription.plan',value('plan')],['subscription.renewal_date',value('renewal')]]){
        if(v)args.set.push(fieldName,v);else args.clear.push(fieldName);
      }
      return {operation:'provider update',operands:[id],args};
    });
}
async function providerDetail(id) {
  const generation=openDialog(providerName(id),'<div class="loading">正在读取服务商详情…</div>','PROVIDER');
  try {
    const [data,inputs]=await Promise.all([read('provider inspect',[id]),read('provider input list',[id]).catch(()=>null)]);
    if(generation!==state.dialogGeneration)return;
    const p=data.provider;
    $('dialog-content').innerHTML=`<div class="actions">${button('编辑信息','provider-edit',id,'primary','settings')}${button('配置授权','provider-auth',id,'','key')}${button('添加模型','model-add',id,'','plus')}</div><div class="detail-facts">${fact('本地 ID',id)}${fact('服务类型',serviceName(p))}${fact('授权',p.auth?'已配置 · 上游未验证':'尚未配置')}${fact('账户身份',data.identity || '未知')}${fact('订阅计划',p.subscription?.plan || '未记录')}${fact('续费日期',dateValue(data.renewal) || '未知')}</div><h3 class="subheading">额度与信息来源</h3>${notice('额度未知不代表额度为零。仅显示已获取的信息，不自动查询服务商。')}${(data.observations?.observations || []).filter(o=>o.scope==='quota').map(o=>`<div class="detail-facts">${fact('来源',o.source)}${fact('新鲜度',stateLabel(o.freshness?.state))}${fact('观测状态',stateLabel(o.state))}</div>`).join('') || '<p class="muted">暂无已知额度观测。</p>'}${inputs?.inputs?.length?detail(inputs,'查看额度输入及新鲜度'):''}<div class="actions">${button('查询上游额度','provider-quota',id,'small')}${button('登记额度输入','provider-input',id,'small')}</div>${detail(data,'查看继承、覆盖值、端点与完整详情')}<h3 class="subheading">更多操作</h3><div class="actions">${button('高级编辑','provider-advanced',id,'small')}${button('移除服务商','provider-remove',id,'small danger')}</div>`;
  }catch(error){if(generation===state.dialogGeneration)$('dialog-content').innerHTML=notice(escapeHtml(error.message),'error');}
}
function providerAuth(id) {
  const p=state.config.providers[id];const integration=state.integrations.find(i=>i.id===p?.integration);
  const keySupported=integration?.authentication_methods.includes('api-key');
  if(!keySupported){openOperation('provider login',[id]);return;}
  formDialog('配置 '+providerName(id)+' 的授权','密钥提交后保存在本机，由系统用户权限保护。输入框会在提交或关闭后清空。',
    field('key','服务商 API Key','',{type:'password',required:true,placeholder:'输入服务商提供的密钥'})+notice('保存授权不会自动查询上游或启动网关。'),()=>({operation:'provider key set',operands:[id],options:{key:value('key')}}),{success:'授权已保存，仍需按实际使用结果验证。'});
}
function modelForm(id='', provider='') {
  const editing=!!id;const config=editing?modelConfig(id):null;
  if(!Object.keys(state.config.providers).length){openDialog('先添加服务商',empty('模型需要归属一个服务商','添加服务商后，即可登记具体模型。',button('添加服务商','provider-add','','primary')));return;}
  formDialog(editing?'编辑模型':'添加模型',editing?'优先级影响候选模型排序；数值越大，越优先。':'登记服务商提供的准确模型 ID，不会发送推理请求。',
    (editing?field('qualified','模型引用',id,{readonly:true}):field('provider','所属服务商',provider || Object.keys(state.config.providers)[0],{required:true,options:Object.keys(state.config.providers).map(p=>[p,providerName(p)])})+field('model','上游模型 ID','',{required:true,placeholder:'例如：deepseek-chat'}))+
    '<div class="form-grid">'+field('name','显示名称',config?.overrides?.name ?? '',{help:'留空表示显式空名称；恢复继承请使用下方操作。'})+field('priority','基础优先级',config?.priority ?? 0,{type:'number',required:true})+'</div>'+field('enabled','模型状态',String(config?.enabled ?? true),{options:[['true','启用'],['false','停用']]})+(editing?`<div class="actions">${button('恢复名称继承','model-inherit',id,'small')}${button('高级设置 / 优先级规则','model-advanced',id,'small')}${button('移除模型','model-remove',id,'small danger')}</div>`:''),()=>({operation:editing?'model update':'model add',operands:[editing?id:value('provider')+'/'+value('model').trim()],args:{set:['overrides.name',value('name'),'priority',value('priority'),'enabled',value('enabled')]}}));
}
function aliasForm(encoded='') {
  const [id,protocol]=encoded?JSON.parse(encoded):[];const current=id?state.config.model_aliases.find(a=>a.id===id&&a.protocol===protocol):null;
  const models=(state.models || []).filter(m=>m.kind==='concrete');
  formDialog(id?'编辑模型别名':'新建模型别名','固定一个请求名称，并明确选择同一原生协议的候选模型。不会自动加入其他模型。',
    '<div class="form-grid">'+field('id','别名',id || '',{required:true,readonly:!!id,placeholder:'例如：coding'})+field('protocol','原生协议',protocol || 'chat-completions',{required:true,options:Object.entries(protocolNames)})+'</div><h3 class="subheading">候选模型</h3><div id="candidates" class="checklist"></div>'+(id?`<div class="actions">${button('查看选择依据','alias-explain',encoded,'small')}${button('移除别名','alias-remove',encoded,'small danger')}</div>`:''),()=>{
      const candidates=[...document.querySelectorAll('[name=candidate]:checked')].map(i=>i.value);if(!candidates.length)throw Error('请至少选择一个候选模型。');
      const args={protocol:[protocol || value('protocol')],candidate:candidates};if(id&&protocol!==value('protocol'))args['new-protocol']=[value('protocol')];
      return {operation:id?'model alias update':'model alias add',operands:[value('id').trim()],args};
    });
  const update=()=>{$('candidates').innerHTML=models.length?models.map(m=>{const supported=m.published_protocols?.includes(value('protocol'));return `<label><input type="checkbox" name="candidate" value="${escapeHtml(m.id)}" ${current?.candidates.includes(m.id)?'checked':''} ${!supported&&!current?.candidates.includes(m.id)?'disabled':''}><span>${escapeHtml(m.id)}${!supported?'<small class="label-note">此模型未发布所选协议，保存时仍需通过完整校验</small>':''}</span></label>`;}).join(''):notice('还没有具体模型，请先添加模型。','warning');};
  $('f-protocol').addEventListener('change',update);update();
}
function agentForm(agent='') {
  formDialog('添加工具配置','登记一个本地安装或配置目录。此操作只保存管理信息，不修改工具文件。',
    field('agent','编程工具',agent || 'codex',{required:true,options:agentTypes.map(a=>[a[0],a[1]])})+field('target','配置名称','',{required:true,placeholder:'例如：codex-main'})+field('path','原生配置路径','',{required:true,placeholder:'填写该工具的配置目录或适配器支持的文件路径',help:'可先在工具卡片中“发现本地安装”，查看实际路径。'}),()=>({operation:'agent add',operands:[value('target').trim()],args:{agent:[value('agent')],path:[value('path').trim()]}}));
}
async function agentDetail(target) {
  const generation=openDialog(target,'<div class="loading">正在检查工具配置…</div>','CODING TOOL');
  try {const data=await read('agent inspect',[target]);if(generation!==state.dialogGeneration)return;
    const config=state.config.agents[target];
    $('dialog-content').innerHTML=notice('模型选择只保存设置。点击“应用到工具”才会写入原生配置。')+`<div class="detail-facts">${fact('已保存状态',stateLabel(data.desired_state))}${fact('应用状态',stateLabel(data.applied_state))}${fact('实际生效状态',stateLabel(data.effective_state))}${fact('默认模型',config.default?.id || '未选择')}</div><div class="actions">${button('添加模型来源','agent-source',target,'','plus')}${button('选择默认模型','agent-default',target,'soft')}</div><h3 class="subheading">模型来源</h3>${Object.entries(config.providers).map(([name,p])=>`<div class="list-row"><strong>${escapeHtml(name)}</strong><small class="label-note">${escapeHtml(p.proxy?'tmax 本地网关':providerName(p.provider))} · ${p.models.length} 个模型</small><div class="actions">${button('添加可选模型','agent-model',JSON.stringify([target,name]),'small')}</div></div>`).join('') || '<p class="muted">尚未配置来源。</p>'}<h3 class="subheading">应用与恢复</h3><div class="actions">${button('应用到工具','agent-apply',target,'primary')}${button('恢复原生配置','agent-restore',target)}${button('更多配置','agent-advanced',target,'small')}</div>${detail(data,'查看检查结果、阻塞原因与受影响文件')}`;
  }catch(error){if(generation===state.dialogGeneration)$('dialog-content').innerHTML=notice(escapeHtml(error.message),'error');}
}
function agentSource(target) {
  formDialog('添加模型来源','为工具登记一个直接连接的服务商，或明确使用 tmax 网关。',field('name','工具内的来源名称','',{required:true,placeholder:'例如：deepseek'})+field('source','连接来源','',{required:true,options:[['','请选择'],...Object.keys(state.config.providers).map(p=>[p,providerName(p)]),['@proxy','tmax 本地网关']]}),()=>({operation:'agent provider add',operands:[target,value('name').trim()],args:value('source')==='@proxy'?{proxy:['true']}:{provider:[value('source')]}}));
}
function agentModel(encoded) {
  const [target,name]=JSON.parse(encoded);const source=state.config.agents[target].providers[name];
  const candidates=(state.models || []).filter(m=>source.proxy || (m.kind==='concrete'&&m.id.startsWith(source.provider+'/'))).flatMap(m=>(m.published_protocols || []).map(p=>[JSON.stringify([source.proxy?m.id:m.id.slice(source.provider.length+1),p]),`${m.id} · ${protocolNames[p] || p}`]));
  formDialog('添加可选模型','只添加到此来源的模型列表；默认模型需要另行选择。',field('model','模型与原生协议','',{required:true,options:[['','请选择'],...candidates]}),()=>{const [id,protocol]=JSON.parse(value('model'));return {operation:'agent model add',operands:[target,name,id],args:{protocol:[protocol]}};});
}
function agentDefault(target) {
  const config=state.config.agents[target];
  const choices=Object.entries(config.providers).flatMap(([name,p])=>p.models.map(m=>[JSON.stringify([name,m.id,m.protocol]),`${name} / ${m.id} · ${protocolNames[m.protocol] || m.protocol}`]));
  formDialog('选择默认模型','从已登记的模型中选择。保存后，主动应用到工具才会更改原生配置。',field('selection','默认模型','',{required:true,options:[['','请选择'],...choices]}),()=>{const [name,id,protocol]=JSON.parse(value('selection'));return {operation:'agent default set',operands:[target,name,id],args:{protocol:[protocol]}};});
}
function configureProxy() {
  formDialog('网关设置','设置本地监听地址。首次配置会生成共享密钥；保存不会自动启动网关。',field('listen','监听地址',state.config.proxy?.listen || '127.0.0.1:8317',{required:true,help:'仅用于本机连接，例如 127.0.0.1:8317。'}),()=>({operation:'proxy configure',args:{listen:[value('listen').trim()]}}),{stayOpen:true});
}

const operandLabels={'Provider ID':'服务商 ID','Qualified offering':'具体模型引用','Alias ID':'别名','Target ID':'工具配置名称','Native provider entry':'工具内来源名称','Model ID':'模型 ID','Catalog provider ID':'目录服务商 ID','Request handle':'请求记录标识','Recovery handle':'恢复记录标识','Native role':'原生模型角色'};
const operationLabels={'proxy start':'启动网关','proxy stop':'停止网关','proxy restart':'重启网关','proxy autostart enable':'启用登录时启动','proxy autostart disable':'关闭登录时启动','agent apply':'应用到工具','agent restore':'恢复原生配置','provider remove':'移除服务商','model remove':'移除模型','model alias remove':'移除模型别名','provider refresh':'查询上游信息','provider login':'登录服务商','model explain':'查看模型选择依据','agent discover':'发现本地安装','skill install':'安装 tmax 使用技能'};
const operationEffects={
  'skill install':'将 tmax 使用说明安装到本机技能目录，供编程工具读取。可先预览目标路径；不会修改模型配置。',
  'proxy start':'启动本地网关。请确认监听地址与服务商授权已配置。',
  'proxy stop':'停止网关会停止接受新请求。仍有进行中的请求时，正常停止会被拒绝。',
  'proxy restart':'重新加载配置并重启网关监听。进行中的请求受服务端保护。',
  'agent apply':'将已保存的模型选择应用到此工具的原生配置。可先预览涉及的文件与影响。',
  'agent restore':'恢复由 tmax 管理的原生更改，保留其他修改；存在冲突时会阻止覆盖。',
  'provider remove':'移除此服务商的已保存设置。若仍被引用，操作会被拒绝；不会注销上游账户。',
  'model remove':'移除模型登记；被工具或别名引用时，需先解除相关引用。',
  'model alias remove':'移除此协议下的别名，不改变其他模型或工具的直接连接。',
  'provider refresh':'这是一次主动的上游查询，可能访问服务商。不会自动登记新模型或执行推理。',
  'model probe':'这会向上游发送真实推理请求，可能消耗额度或产生费用。仅在明确需要时执行。',
  'web stop':'停止管理页面后，浏览器将无法继续连接。需要通过命令行重新打开。',
};
function argumentControl(name, meta) {
  const node=document.createElement('div');node.className='argument-group';node.dataset.argument=name;node.dataset.switch=String(!!meta.switch);
  const title=document.createElement('strong');title.textContent='--'+name;node.append(title);
  const rows=document.createElement('div');node.append(rows);
  function addRow(initial=[]) {
    const row=document.createElement('div');row.className='argument-row';row.dataset.row='true';
    for(let part=0;part<(meta.arity || 1);part++) {
      const choices=part===0?(meta.fields?.length?meta.fields:meta.choices || []):[];
      const label=document.createElement('label');label.className='field';
      const span=document.createElement('span');span.textContent=meta.arity===2?(part===0?(name==='endpoint'?'原生协议':'字段'):'值（可以为空字符串）'):'值';label.append(span);
      const input=document.createElement(choices.length?'select':'input');input.dataset.part=String(part);input.autocomplete='off';
      if(choices.length)for(const choice of ['',...choices]){const option=document.createElement('option');option.value=choice;option.textContent=choice || '不设置';input.append(option);}
      if(meta.switch)input.type='checkbox';
      if(initial[part]!==undefined){if(meta.switch)input.checked=initial[part]==='true';else input.value=initial[part];}
      label.append(input);row.append(label);
    }
    if(meta.repeatable){const remove=document.createElement('button');remove.type='button';remove.className='btn small';remove.textContent='移除';remove.addEventListener('click',()=>row.remove());row.append(remove);}
    rows.append(row);
  }
  addRow();
  if(meta.repeatable){const add=document.createElement('button');add.type='button';add.className='btn small';add.textContent='添加一项';add.addEventListener('click',()=>addRow());node.append(add);}
  return node;
}
function collectArguments() {
  const args={};
  for(const group of document.querySelectorAll('#operation-arguments [data-argument]')) {
    const values=[];
    for(const row of group.querySelectorAll('[data-row]')) {
      const inputs=[...row.querySelectorAll('[data-part]')];
      if(group.dataset.switch==='true'){if(inputs[0].checked)values.push('true');continue;}
      if(inputs.every(input=>input.value===''))continue;
      // Empty second operands are valid string values, not an omitted --set pair.
      if(inputs.length===2 && !inputs[0].value)throw Error('请选择字段或协议，再填写对应值。');
      values.push(...inputs.map(input=>input.value));
    }
    if(values.length)args[group.dataset.argument]=values;
  }
  return args;
}
function openOperation(operation, operands=[], args={}) {
  const entry=state.catalog.find(c=>c.operation===operation);
  if(!entry){toast('此版本未提供该操作，请查看高级操作列表。');return;}
  const fields=Array.from({length:entry.operands.max},(_,i)=>field('operand-'+i,operandLabels[entry.operands.labels[i]] || entry.operands.labels[i] || `参数 ${i+1}`,operands[i] || '',{required:i<entry.operands.min})).join('');
  const protectedFields=operation==='provider key set'?field('key','服务商 API Key','',{type:'password',help:'提交或关闭后清空，不保存在浏览器。'}):'';
  const revisionField=entry.revision_guard?field('revision','配置版本保护',state.revision || '',{help:'默认为本次读取的配置版本，阻止覆盖其他操作的新修改。'}):'';
  formDialog(operationLabels[operation] || '高级操作',operationEffects[operation] || (entry.mutation?'执行此明确操作；可先预览影响。完整参数与命令行一致。':'读取本地信息，不自动请求上游服务。'),
    `<p class="form-intro mono">${escapeHtml(operation)}</p>${fields}<div id="operation-arguments"></div>${protectedFields}<details><summary>版本保护</summary>${revisionField || '此操作不修改配置文件，无需版本保护。'}</details>`,()=>{
      const operands=Array.from({length:entry.operands.max},(_,i)=>value('operand-'+i));while(operands.length && !operands[operands.length-1])operands.pop();
      return {operation,operands,args:collectArguments(),options:{key:value('key'),revision:entry.revision_guard?value('revision') || null:null}};
    },{preview:entry.mutation,stayOpen:true,submit:entry.mutation?'执行操作':'读取信息',eyebrow:'ADVANCED OPERATION'});
  for(const arg of entry.arguments) {
    if(/-(fd|stdin)$/.test(arg)||arg==='non-interactive')continue;
    const meta=entry.argument_metadata?.[arg] || {arity:1};const group=argumentControl(arg,meta);$('operation-arguments').append(group);
    if(args[arg]) {
      const arity=meta.arity || 1;
      for(let n=0;n<args[arg].length;n+=arity){if(n && meta.repeatable)group.querySelector(':scope > button').click();const row=[...group.querySelectorAll('[data-row]')].at(-1);row.querySelectorAll('[data-part]').forEach((input,i)=>{if(meta.switch)input.checked=args[arg][n+i]==='true';else input.value=args[arg][n+i] ?? '';});}
    }
  }
}
function advanced(group='') {
  const entries=state.catalog.filter(c=>c.operation!=='proxy key show'&&(!group||c.command[0]===group));
  openDialog('高级操作',`<p class="form-intro">完整管理能力。选择操作后填写参数；常用配置也可以在对应页面直接完成。</p>${field('command','查找操作','',{placeholder:'搜索命令，例如 model rule'})}<div id="command-list"></div>`,'ALL OPERATIONS');
  const show=()=>{const matches=entries.filter(c=>c.operation.includes(value('command').toLowerCase()));$('command-list').innerHTML=matches.map(c=>`<div class="list-row split"><div><h3 class="mono">${escapeHtml(c.operation)}</h3><small>${c.mutation?'执行明确变更 · 支持预览':'本地只读'}</small></div>${button('打开','operation',c.operation,'small')}</div>`).join('') || '<p class="muted">没有匹配的操作。</p>';};
  $('f-command').addEventListener('input',show);show();
}
async function history() {
  const generation=state.generation;const container=$('history-panel');if(!container)return;
  container.innerHTML='<div class="loading">正在读取本地请求记录…</div>';
  try {const data=await read('history list');if(state.area!=='proxy'||generation!==state.generation||container!==$('history-panel'))return;
    container.innerHTML=data.records?.length?`<div class="panel-body"><h3 class="subheading">${data.records.length} 条保留记录</h3>${notice('仅包含当前进程保留的网关请求，不代表上游账单或全部账户用量。')}${data.records.map(record=>detail(record,'查看请求 '+(record.handle || record.id || record.started_at || '详情'))).join('')}${detail(data.coverage,'查看记录覆盖范围与保留限制')}</div>`:empty('还没有网关请求','当前进程没有保留的请求记录。直接连接流量不会出现在这里。','','history',true);
  }catch(error){if(container===$('history-panel'))container.innerHTML=notice(escapeHtml(error.message),'error');}
}
async function revealKey() {
  clearReveal();
  const generation=revealGeneration;
  try {const response=await http('/api/proxy-key/reveal',{});if(state.area!=='proxy'||generation!==revealGeneration)return;
    if(!response.key)throw Error(response.error?.message || '共享密钥暂不可用，请检查网关配置与密钥状态。');
    $('revealed').textContent=response.key;$('revealed').hidden=false;
  }catch(error){if(generation===revealGeneration)toast(error.message);}
}
async function dispatch(action,id) {
  if(state.busy){toast('正在处理当前操作，请稍候。');return;}
  switch(action) {
    case 'retry': await initialize();break;
    case 'navigate': location.hash=id;break;
    case 'close': $('dialog').close();break;
    case 'provider-add': await addProvider();break;
    case 'provider-detail': await providerDetail(id);break;
    case 'provider-edit': editProvider(id);break;
    case 'provider-auth': providerAuth(id);break;
    case 'provider-remove': openOperation('provider remove',[id]);break;
    case 'provider-advanced': openOperation('provider update',[id]);break;
    case 'provider-quota': openOperation('provider refresh',[id],{scope:['quota']});break;
    case 'provider-input': openOperation('provider input add',[id]);break;
    case 'model-add': modelForm('',id);break;
    case 'model-edit': modelForm(id);break;
    case 'model-advanced': openOperation('model update',[id]);break;
    case 'model-inherit': openOperation('model update',[id],{clear:['overrides.name']});break;
    case 'model-remove': openOperation('model remove',[id]);break;
    case 'model-tab': state.modelKind=id;state.query='';render();break;
    case 'alias-add': aliasForm();break;
    case 'alias-edit': aliasForm(id);break;
    case 'alias-remove': {const [name,protocol]=JSON.parse(id);openOperation('model alias remove',[name],{protocol:[protocol]});break;}
    case 'alias-explain': {const [name,protocol]=JSON.parse(id);openOperation('model explain',[name],{protocol:[protocol]});break;}
    case 'agent-add': agentForm(id);break;
    case 'agent-discover': openOperation('agent discover',[],{agent:[id]});break;
    case 'agent-detail': await agentDetail(id);break;
    case 'agent-source': agentSource(id);break;
    case 'agent-model': agentModel(id);break;
    case 'agent-default': agentDefault(id);break;
    case 'agent-apply': openOperation('agent apply',[id]);break;
    case 'agent-restore': openOperation('agent restore',[id]);break;
    case 'agent-advanced': openOperation('agent inspect',[id]);break;
    case 'proxy-configure': configureProxy();break;
    case 'autostart': formDialog('登录时启动','这会更改本机的登录启动注册。启用后，下次登录会按已保存配置启动网关。',field('enabled','登录时启动',String(state.runtime?.autostart?.login_enabled || false),{options:[['false','关闭'],['true','启用']]}),()=>({operation:'proxy autostart '+(value('enabled')==='true'?'enable':'disable')}),{stayOpen:true});break;
    case 'reveal': await revealKey();break;
    case 'hide-key': clearReveal();break;
    case 'history': await history();break;
    case 'advanced': advanced(id);break;
    case 'operation': openOperation(id);break;
    case 'copy': try {await navigator.clipboard.writeText(id);toast('地址已复制');}catch(_){toast('无法自动复制，请选择地址后手动复制。');}break;
  }
}
document.addEventListener('click',event=>{if(state.busy && event.target.closest('a[data-area],a.brand')){event.preventDefault();toast('正在处理当前操作，请稍候。');return;}const target=event.target.closest('[data-action]');if(target)dispatch(target.dataset.action,target.dataset.id || '').catch(error=>toast(error.message));});
$('dialog-close').addEventListener('click',()=>{if(!state.busy)$('dialog').close();});
$('dialog').addEventListener('cancel',event=>{if(state.busy)event.preventDefault();});
$('dialog').addEventListener('close',()=>{state.dialogGeneration++;$('dialog-content').querySelectorAll('input[type=password]').forEach(i=>{i.value='';});$('dialog-content').replaceChildren();});
$('refresh').innerHTML=icon('refresh');$('advanced-icon').innerHTML=icon('settings');
$('refresh').addEventListener('click',()=>{if(!state.busy)refresh();});
$('advanced').addEventListener('click',()=>{if(!state.busy)advanced();});
window.addEventListener('hashchange',()=>navigate(location.hash.slice(1)));
window.addEventListener('pagehide',()=>{clearReveal();document.querySelectorAll('input[type=password]').forEach(i=>{i.value='';});});
document.addEventListener('visibilitychange',()=>{if(document.hidden)clearReveal();});
async function initialize() {
  $('navigation').innerHTML=Object.entries(areas).map(([key,[title]])=>`<a href="#${key}" class="nav-item" data-area="${key}">${icon(key)}${title}</a>`).join('');
  navigate(location.hash.slice(1));
  try {state.catalog=await http('/api/catalog');if(!Array.isArray(state.catalog))throw Error('操作列表格式不可识别。');await refresh();}
  catch(error){$('content').setAttribute('aria-busy','false');$('content').innerHTML=empty('暂时无法连接工作台',escapeHtml(error.message),button('重试','retry'),'info');$('connection').textContent='连接不可用';}
}
initialize();
