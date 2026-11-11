// Client half: the tmax bundle page in DSH's Plugins view.
//
// It reads the live Host model catalog; it never runs a CLI or receives a key.
// The connect command is shown for the user to run in a terminal, so the page
// stays a truthful status surface in M0.
window.__ModuleLoader__.load({
  id: 'tmax',
  factory(require) {
    const React = require('react')
    const NS = 'tmaxDshSettings'
    const en = {
      title: 'tmax',
      loading: 'Reading model catalog…',
      connected: 'Connected',
      disconnected: 'Not connected',
      failed: 'Provider failed',
      empty: 'No tmax models are registered yet. Use Connect.',
      hint: 'This page is read-only. The command writes only the DSH credential reference; it never prints the key.',
      models: 'models',
      failure: 'Host error',
      connect: 'Connect now',
      connecting: 'Connecting…',
      connectDone: 'Connected. Refreshing model catalog…',
      connectFailed: 'Connect failed',
      connectAction: 'Connect',
      connectedAction: 'Connected',
      usage: 'Usage and limits',
      limitReset: 'resets',
      limitRemaining: 'remaining',
      limitDashboard: 'Provider dashboard',
      usageUnknown: 'Usage information is unavailable.',
      usedPercent: 'used',
      remainingPercent: 'remaining',
      quotaAvailable: 'available',
      quotaSuspended: 'suspended',
      quotaNone: 'no quota data',
      windowRolling: 'rolling',
      windowWeekly: 'weekly',
      windowMonthly: 'monthly',
      gatewayReady: 'Gateway ready',
      gatewayUnknown: 'Gateway not ready',
    }
    const zh = {
      title: 'tmax',
      loading: '正在读取模型目录…',
      connected: '已连接',
      disconnected: '未连接',
      failed: 'provider 加载失败',
      empty: '还没有注册 tmax 模型。点击“立即连接”。',
      hint: '本页只读。命令只写入 DSH 凭据引用，不会输出密钥。',
      models: '个模型',
      failure: 'Host 错误',
      connect: '立即连接',
      connecting: '连接中…',
      connectDone: '已连接，正在刷新模型目录…',
      connectFailed: '连接失败',
      connectAction: '连接',
      connectedAction: '已连接',
      usage: '用量与额度',
      limitReset: '重置',
      limitRemaining: '剩余',
      limitDashboard: '供应商后台',
      usageUnknown: '暂时拿不到用量信息。',
      usedPercent: '已用',
      remainingPercent: '剩余',
      quotaAvailable: '可用',
      quotaSuspended: '已暂停',
      quotaNone: '无额度数据',
      windowRolling: '滚动',
      windowWeekly: '周',
      windowMonthly: '月',
      gatewayReady: 'gateway 已就绪',
      gatewayUnknown: 'gateway 未就绪',
    }
    const h = React.createElement

    function findGroup(catalog) {
      if (!catalog || !Array.isArray(catalog.groups)) return undefined
      return catalog.groups.find(group => group.id === 'tmax' || group.id.endsWith('tmax'))
    }

    function findFailure(catalog) {
      if (!catalog || !Array.isArray(catalog.failures)) return undefined
      return catalog.failures.find(failure => failure.id === 'tmax' || failure.id.endsWith('tmax'))
    }

    function TmaxPanel({ t }) {
      const [state, setState] = React.useState({ phase: 'loading' })
      const [connecting, setConnecting] = React.useState(false)
      const [message, setMessage] = React.useState('')
      const load = React.useCallback(async () => {
        setState({ phase: 'loading' })
        let gateway
        try {
          const statusResponse = await fetch(new URL('_tmax/status', document.baseURI), { credentials: 'same-origin' })
          if (statusResponse.ok) gateway = (await statusResponse.json()).gateway
        } catch {}
        try {
          const response = await ctx.remote.session.modelCatalog()
          if (!response.ok) throw new Error(`${response.error.code}: ${response.error.message}`)
          const group = findGroup(response.value)
          const failure = findFailure(response.value)
          if (group) setState({ phase: 'connected', group, gateway })
          else if (failure) setState({ phase: 'failed', message: failure.message, gateway })
          else setState({ phase: 'disconnected', gateway })
        } catch (error) {
          setState({ phase: 'failed', message: error instanceof Error ? error.message : String(error), gateway })
        }
      }, [])
      React.useEffect(() => { void load() }, [load])
      const connect = async () => {
        setConnecting(true)
        setMessage('')
        try {
          const response = await fetch(new URL('_tmax/connect', document.baseURI), {
            method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json' }, body: '{}',
          })
          const result = await response.json().catch(() => ({}))
          if (!response.ok || result.ok !== true) {
            setMessage(`${t('connectFailed')}: ${result.message || result.code || response.status}`)
            return
          }
          await load()
        } catch (error) {
          setMessage(`${t('connectFailed')}: ${error instanceof Error ? error.message : String(error)}`)
        } finally {
          setConnecting(false)
        }
      }
      const status = state.phase === 'connected'
        ? t('connected')
        : state.phase === 'failed' ? t('failed') : state.phase === 'loading' ? t('loading') : t('disconnected')
      const color = state.phase === 'connected' ? '#15803d' : state.phase === 'failed' ? '#b91c1c' : '#6b7280'
      const children = [
        h('div', { key: 'head', style: { display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' } },
          h('span', { style: { width: 8, height: 8, borderRadius: 99, background: color, display: 'inline-block' } }),
          h('strong', null, `${t('title')}: ${status}`),
          state.phase === 'connected' ? null : h('button', { type: 'button', onClick: connect, disabled: connecting }, connecting ? t('connecting') : t('connectAction'))),
        state.gateway !== undefined && h('div', { key: 'gateway', style: { color: '#6b7280', fontSize: 12 } },
          state.gateway && state.gateway.ready === true ? t('gatewayReady') : t('gatewayUnknown')),
      ]
      if (state.phase === 'connected') {
        children.push(h('div', { key: 'models' },
          h('div', { style: { fontWeight: 600, marginBottom: 4 } }, `${state.group.models.length} ${t('models')}`),
          h('div', { style: { color: '#4b5563', fontSize: 13, lineHeight: 1.6 } },
            state.group.models.map(model => model.name === model.id ? model.id : `${model.name} (${model.id})`).join(' · '))))
      }
      if (state.phase === 'failed') children.push(h('div', { key: 'failure', style: { color: '#b91c1c' } }, `${t('failure')}: ${state.message}`))
      if (state.phase === 'disconnected') children.push(h('p', { key: 'empty', style: { margin: 0 } }, t('empty')))
      if (message !== '') children.push(h('p', { key: 'message', style: { color: '#b91c1c', margin: 0 } }, message))
      children.push(h(Usage, { key: 'usage', t }))
      return h('div', { style: { display: 'grid', gap: 14, maxWidth: 720 } }, children)
    }

    function rowLabel(label, t) {
      if (label === null || label === undefined) return ''
      if (typeof label === 'string') return label
      if (label.key === 'bucket') return [label.name, label.window].filter(Boolean).join(' ')
      if (label.key === 'duration') return `${label.duration ?? ''} ${label.unit ?? ''}`.trim()
      const names = { rolling: 'windowRolling', weekly: 'windowWeekly', monthly: 'windowMonthly', weekly_allowance: 'windowWeekly', limit: 'windowMonthly' }
      return names[label.key] ? t(names[label.key]) : (label.key || '')
    }

    function quotaLabel(state, t) {
      if (state === 'available') return t('quotaAvailable')
      if (state === 'suspended') return t('quotaSuspended')
      return t('quotaNone')
    }

    function Usage({ t }) {
      const [state, setState] = React.useState({ phase: 'loading' })
      React.useEffect(() => {
        let live = true
        fetch(new URL('_tmax/usage', document.baseURI), { credentials: 'same-origin' })
          .then(response => response.ok ? response.json() : undefined)
          .then(value => { if (live) setState(value?.ok ? { phase: 'ready', providers: value.providers || [] } : { phase: 'failed' }) })
          .catch(() => { if (live) setState({ phase: 'failed' }) })
        return () => { live = false }
      }, [])
      if (state.phase === 'loading') return h('div', { style: { color: '#6b7280', fontSize: 12 } }, t('loading'))
      if (state.phase !== 'ready' || state.providers.length === 0) return h('div', { style: { color: '#6b7280', fontSize: 12 } }, t('usageUnknown'))
      return h('div', { key: 'usage', style: { display: 'grid', gap: 8 } },
        h('div', { style: { fontWeight: 600 } }, t('usage')),
        state.providers.filter(provider => (provider.billing?.rows || []).some(row => row.percent !== undefined || row.counters?.limit !== undefined)).sort((a, b) => (a.quota?.state === 'suspended' ? -1 : b.quota?.state === 'suspended' ? 1 : 0)).map(provider => {
          const rows = (provider.billing?.rows || []).filter(row => row.percent !== undefined || row.counters?.limit !== undefined)
          const color = provider.quota?.state === 'suspended' ? '#b91c1c' : provider.quota?.state === 'available' ? '#15803d' : '#6b7280'
          return h('div', { key: provider.id, style: { display: 'grid', gap: 4, borderTop: '1px solid #f3f4f6', paddingTop: 6 } },
            h('div', { style: { display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' } },
              h('strong', { style: { fontSize: 13 } }, provider.name),
              h('span', { style: { color, fontSize: 12 } }, quotaLabel(provider.quota?.state, t))),
            rows.map((row, index) => h('div', { key: String(index), style: { display: 'flex', gap: 8, alignItems: 'center', fontSize: 12 } },
              h('span', { style: { minWidth: 90, color: '#4b5563' } }, rowLabel(row.label, t)),
              typeof row.percent === 'number'
                ? h('span', { style: { flex: 1, height: 6, background: '#e5e7eb', borderRadius: 99 } },
                  h('span', { style: { display: 'block', width: `${Math.max(0, Math.min(100, row.basis === 'remaining_percent' ? 100 - row.percent : row.percent))}%`, height: 6, background: row.status === 'rate-limited' ? '#b91c1c' : '#2563eb', borderRadius: 99 } }))
                : null,
              typeof row.percent === 'number' ? h('span', null, row.basis === 'remaining_percent' ? `${t('remainingPercent')} ${row.percent}%` : `${row.percent}% ${t('usedPercent')}`) : null,
              row.counters?.limit !== undefined ? h('span', null, `${t('limitRemaining')} ${row.counters.remaining ?? '?'} / ${row.counters.limit}`) : null,
              (row.reset?.at || typeof row.reset?.raw === 'number') ? h('span', { style: { color: '#6b7280' } }, `${t('limitReset')} ${new Date(row.reset.at || row.reset.raw).toLocaleDateString()}`) : null)),
            provider.billing?.dashboard
              ? h('a', { href: provider.billing.dashboard, target: '_blank', rel: 'noreferrer', style: { fontSize: 12 } }, t('limitDashboard'))
              : null)
        }))
    }

    let ctx
    function apply(context) {
      ctx = context
      context.effect(() => context.locale.register(NS, { en, zh }), 'tmax: settings locale')
      const t = context.locale.bind(NS)
      context.effect(() => context.slots.inject('settings.models.footer', () => context.slots.register({
        name: 'settings.models.footer', id: 'tmax', order: 10, locale: NS,
      }, TmaxPanel)), 'tmax: models footer')
    }

    return { inject: ['slots', 'locale', 'remote', 'remote.session'], apply }
  },
})
