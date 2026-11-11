// Client half: the tmax access card on DSH's Models settings page.
//
// It reads the live Host model catalog and the secret-free usage route; it
// never runs a CLI itself and never receives a key. Every colour resolves
// through a `--dsw-alias-*` token so the card follows the light and dark
// themes instead of shipping its own palette.
window.__ModuleLoader__.load({
  id: 'tmax',
  factory(require) {
    const React = require('react')
    const NS = 'tmaxDshSettings'
    const h = React.createElement

    const en = {
      title: 'tmax',
      loading: 'Reading model catalog…',
      connected: 'Connected',
      disconnected: 'Not connected',
      failed: 'Provider failed',
      empty: 'No tmax models are registered yet.',
      hint: 'Connecting writes the DSH credential reference. The key is never copied or shown.',
      models: 'models',
      failure: 'Host error',
      connecting: 'Connecting…',
      connectFailed: 'Connect failed',
      connectAction: 'Connect',
      usage: 'Usage and limits',
      usageUnknown: 'No provider reports quotas yet.',
      remaining: 'remaining',
      quotaSuspended: 'Suspended',
      windowRolling: 'rolling',
      windowWeekly: 'weekly',
      windowMonthly: 'monthly',
      windowGeneric: 'limit',
      resets: 'resets',
      gatewayReady: 'gateway ready',
      gatewayUnknown: 'gateway not ready',
      openDashboard: 'Open provider dashboard',
      unitMinute: 'min',
      unitHour: 'h',
    }
    const zh = {
      title: 'tmax',
      loading: '正在读取模型目录…',
      connected: '已连接',
      disconnected: '未连接',
      failed: 'provider 加载失败',
      empty: '还没有注册 tmax 模型。',
      hint: '连接会写入 DSH 凭据引用，密钥不会被复制或显示。',
      models: '个模型',
      failure: 'Host 错误',
      connecting: '连接中…',
      connectFailed: '连接失败',
      connectAction: '连接',
      usage: '用量与额度',
      usageUnknown: '还没有 provider 上报额度。',
      remaining: '剩余',
      quotaSuspended: '已暂停',
      windowRolling: '滚动',
      windowWeekly: '周',
      windowMonthly: '月',
      windowGeneric: '额度',
      resets: '重置',
      gatewayReady: 'gateway 已就绪',
      gatewayUnknown: 'gateway 未就绪',
      openDashboard: '打开供应商后台',
      unitMinute: '分钟',
      unitHour: '小时',
    }

    // Scoped to `tmax-`; injected once so the card can use the app's own
    // material, focus ring, and hover behaviour instead of inline literals.
    const STYLE = `
.tmax-section { display: flex; flex-direction: column; gap: 10px; margin-top: 32px; max-width: 720px; color: var(--dsw-alias-label-primary); }
.tmax-card { border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-xl); padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; }
.tmax-head { display: flex; align-items: center; gap: 10px; min-width: 0; }
.tmax-name { font-size: 14px; line-height: 22px; font-weight: 500; color: var(--dsw-alias-label-primary); }
.tmax-tag { flex: none; padding: 1px 6px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: var(--dsw-radius-xs); font-size: 11px; line-height: 16px; color: var(--dsw-alias-label-secondary); }
.tmax-tag[data-tone='error'] { color: var(--dsw-alias-state-error-primary); border-color: var(--dsw-alias-state-error-primary); }
.tmax-dot { box-sizing: border-box; display: inline-block; flex: none; width: 8px; height: 8px; border-radius: 50%; corner-shape: round; }
.tmax-dot[data-tone='ok'] { background: var(--dsw-alias-state-success-primary); }
.tmax-dot[data-tone='error'] { background: var(--dsw-alias-state-error-primary); }
.tmax-dot[data-tone='idle'] { background: var(--dsw-alias-label-tertiary); }
.tmax-actions { margin-left: auto; display: inline-flex; align-items: center; gap: 8px; }
.tmax-button { box-sizing: border-box; display: inline-flex; align-items: center; justify-content: center; height: 28px; padding: 0 10px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: var(--dsw-radius-sm); background: transparent; color: var(--dsw-alias-label-primary); font: inherit; font-size: 12px; line-height: 18px; cursor: pointer; }
.tmax-button[data-kind='primary'] { border-color: transparent; background: var(--dsw-alias-button-primary-fill); color: var(--dsw-alias-label-primary-foreground); }
@media (hover: hover) { .tmax-button:hover:not(:disabled) { background: var(--dsw-alias-interactive-bg-hover-solid); } .tmax-button[data-kind='primary']:hover:not(:disabled) { background: var(--dsw-alias-button-primary-hover); } }
.tmax-button:disabled { opacity: 0.4; cursor: default; }
.tmax-meta { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmax-models { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); overflow-wrap: anywhere; }
.tmax-error { font-size: 12px; line-height: 18px; color: var(--dsw-alias-state-error-primary); overflow-wrap: anywhere; }
.tmax-divider { height: 0; border-top: 0.5px solid var(--dsw-alias-border-l1); }
.tmax-usage { display: flex; flex-direction: column; gap: 8px; }
.tmax-usage-title { font-size: 12px; line-height: 18px; font-weight: 500; color: var(--dsw-alias-label-secondary); margin-bottom: 2px; }
.tmax-usage-row { display: flex; flex-direction: column; gap: 2px; font-size: 12px; line-height: 18px; }
.tmax-usage-head { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
.tmax-usage-name { color: var(--dsw-alias-label-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
a.tmax-usage-name { text-decoration: none; }
a.tmax-usage-name:hover { text-decoration: underline; text-underline-offset: 2px; }
.tmax-usage-values { display: flex; flex-wrap: wrap; gap: 2px 12px; min-width: 0; padding-left: 10px; }
.tmax-usage-reset { margin-left: auto; flex: none; color: var(--dsw-alias-label-secondary); white-space: nowrap; }
.tmax-value { color: var(--dsw-alias-label-secondary); white-space: nowrap; }
.tmax-value[data-tone='warn'] { color: var(--dsw-alias-state-warn-label); }
.tmax-value[data-tone='error'] { color: var(--dsw-alias-state-error-primary); }
.tmax-hint { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
`

    const WINDOW_NAMES = {
      rolling: 'windowRolling',
      weekly: 'windowWeekly',
      monthly: 'windowMonthly',
      weekly_allowance: 'windowWeekly',
    }

    function windowName(label, index, t) {
      if (label !== null && typeof label === 'object') {
        if (typeof label.key === 'string' && WINDOW_NAMES[label.key] !== undefined) {
          return t(WINDOW_NAMES[label.key])
        }
        const parts = [label.name, label.window].filter(part => typeof part === 'string' && part !== '')
        if (parts.length > 0) return parts.join(' ')
      }
      if (label !== null && typeof label === 'object' && label.key === 'duration') {
        const count = label.duration ?? ''
        const unit = label.unit === 'TIME_UNIT_MINUTE' ? t('unitMinute') : label.unit === 'TIME_UNIT_HOUR' ? t('unitHour') : label.unit ?? ''
        return `${count} ${unit}`.trim()
      }
      return `${t('windowGeneric')} ${index + 1}`
    }

    /**
     * One window as a single direction: how much is left. tmax reports some
     * windows as consumed and others as remaining, and a mixed list makes the
     * reader convert in their head; one direction removes that.
     */
    function reading(row, index, t) {
      const name = windowName(row.label, index, t)
      const generic = name === `${t('windowGeneric')} ${index + 1}`
      const reset = resetAt(row.reset)
      if (typeof row.percent === 'number') {
        const remaining = row.basis === 'remaining_percent' ? row.percent : 100 - row.percent
        return {
          text: `${name} ${t('remaining')} ${Math.max(0, Math.round(remaining))}%`,
          remaining: Math.max(0, remaining),
          generic,
          reset,
        }
      }
      const limit = row.counters?.limit
      if (limit !== undefined && limit !== null) {
        const left = row.counters.remaining
        const total = Number(limit)
        const leftNumber = Number(left)
        const remaining = Number.isFinite(total) && total > 0 && Number.isFinite(leftNumber) ? (leftNumber / total) * 100 : undefined
        return {
          text: `${name} ${t('remaining')} ${left ?? '?'}/${limit}`,
          remaining,
          generic: false,
          reset,
        }
      }
      return undefined
    }

    /**
     * A provider can report several windows it never names — three identical
     * "limit" rows read as noise. Only unnamed windows with the same remaining
     * amount collapse; named windows always keep their own label.
     */
    function collapse(readings, t) {
      const groups = new Map()
      for (const item of readings) {
        const key = item.generic === true ? `g:${Math.round(item.remaining ?? -1)}` : `i:${item.text}`
        const group = groups.get(key)
        if (group === undefined) groups.set(key, { items: [item], generic: item.generic === true })
        else group.items.push(item)
      }
      return [...groups.values()].map(({ items, generic }) => {
        if (!generic || items.length < 2) return items[0]
        const count = items.length
        const remaining = Math.round(items[0].remaining ?? 0)
        return {
          ...items[0],
          text: `${t('windowGeneric')} ×${count} ${t('remaining')} ${remaining}%`,
          reset: items.map(item => item.reset).filter(Boolean).sort((a, b) => a - b)[0],
        }
      })
    }

    /**
     * Providers ordered by how much attention they need: exhausted first, then
     * the tightest window. A provider with no readable window is dropped rather
     * than shown as a row of blanks.
     */
    function usageRows(providers, t) {
      return providers
        .map(provider => {
          const readings = collapse(
            (provider.billing?.rows || []).map((row, index) => reading(row, index, t)).filter(Boolean), t)
          if (readings.length === 0) return undefined
          const tightest = readings.reduce((worst, item) =>
            (item.remaining ?? 100) < (worst.remaining ?? 100) ? item : worst, readings[0])
          const suspended = provider.quota?.state === 'suspended'
          const tightestRemaining = tightest.remaining ?? 100
          return {
            provider,
            readings,
            suspended,
            reset: tightest.reset ?? resetAt(provider.quota?.resetAt),
            tone: suspended || tightestRemaining <= 0 ? 'error' : tightestRemaining <= 20 ? 'warn' : 'ok',
          }
        })
        .filter(Boolean)
        .sort((a, b) => Number(b.suspended) - Number(a.suspended)
          || (a.readings.reduce((n, r) => Math.min(n, r.remaining ?? 100), 100))
            - (b.readings.reduce((n, r) => Math.min(n, r.remaining ?? 100), 100))
          || a.provider.name.localeCompare(b.provider.name))
    }

    function resetAt(reset) {
      const at = reset?.at ?? (typeof reset?.raw === 'number' ? reset.raw : undefined)
      if (at === undefined) return undefined
      const date = new Date(at)
      return Number.isNaN(date.getTime()) ? undefined : date
    }

    function TmaxCard({ t }) {
      const [state, setState] = React.useState({ phase: 'loading' })
      const [usage, setUsage] = React.useState({ phase: 'loading' })
      const [connecting, setConnecting] = React.useState(false)
      const [message, setMessage] = React.useState('')

      const loadUsage = React.useCallback(async () => {
        try {
          const response = await fetch(new URL('_tmax/usage', document.baseURI), { credentials: 'same-origin' })
          const value = response.ok ? await response.json() : undefined
          setUsage(value?.ok === true ? { phase: 'ready', providers: value.providers || [] } : { phase: 'failed' })
        } catch {
          setUsage({ phase: 'failed' })
        }
      }, [])

      const load = React.useCallback(async () => {
        setState({ phase: 'loading' })
        let gateway
        try {
          const response = await fetch(new URL('_tmax/status', document.baseURI), { credentials: 'same-origin' })
          if (response.ok) gateway = (await response.json()).gateway
        } catch {}
        try {
          const response = await ctx.remote.session.modelCatalog()
          if (!response.ok) throw new Error(`${response.error.code}: ${response.error.message}`)
          const groups = response.value.groups || []
          const group = groups.find(item => item.id === 'tmax' || item.id.endsWith('tmax'))
          if (group !== undefined) {
            setState({ phase: 'connected', group, gateway })
            return
          }
          const failures = response.value.failures || []
          const failure = failures.find(item => item.id === 'tmax' || item.id.endsWith('tmax'))
          setState(failure === undefined ? { phase: 'disconnected', gateway } : { phase: 'failed', message: failure.message, gateway })
        } catch (error) {
          setState({ phase: 'failed', message: error instanceof Error ? error.message : String(error), gateway })
        }
      }, [])

      React.useEffect(() => {
        void load()
        void loadUsage()
      }, [load, loadUsage])

      const connect = async () => {
        setConnecting(true)
        setMessage('')
        try {
          const response = await fetch(new URL('_tmax/connect', document.baseURI), {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'content-type': 'application/json' },
            body: '{}',
          })
          const result = await response.json().catch(() => ({}))
          if (!response.ok || result.ok !== true) {
            setMessage(`${t('connectFailed')}: ${result.message || result.code || response.status}`)
            return
          }
          await Promise.all([load(), loadUsage()])
        } catch (error) {
          setMessage(`${t('connectFailed')}: ${error instanceof Error ? error.message : String(error)}`)
        } finally {
          setConnecting(false)
        }
      }

      const connected = state.phase === 'connected'
      const tone = connected ? 'ok' : state.phase === 'failed' ? 'error' : 'idle'
      const statusTag = state.phase === 'loading'
        ? t('loading')
        : connected ? t('connected') : state.phase === 'failed' ? t('failed') : t('disconnected')

      const meta = [state.gateway === undefined ? undefined : state.gateway.ready === true ? t('gatewayReady') : t('gatewayUnknown')]
        .filter(Boolean)
      if (connected) meta.unshift(`${state.group.models.length} ${t('models')}`)

      const children = [
        h('div', { key: 'head', className: 'tmax-head' },
          h('span', { className: 'tmax-dot', 'data-tone': tone }),
          h('span', { className: 'tmax-name' }, t('title')),
          state.phase === 'connected'
            ? null
            : h('span', { className: 'tmax-tag', 'data-tone': tone === 'error' ? 'error' : undefined }, statusTag),
          h('div', { className: 'tmax-actions' },
            connected
              ? null
              : h('button', { type: 'button', className: 'tmax-button', 'data-kind': 'primary', onClick: connect, disabled: connecting },
                connecting ? t('connecting') : t('connectAction')))),
        meta.length > 0 ? h('div', { key: 'meta', className: 'tmax-meta' }, meta.join(' · ')) : null,
      ]

      if (connected) {
        // A registered provider can still project no model; that is its own state,
        // not a silent blank under the model count.
        children.push(state.group.models.length === 0
          ? h('div', { key: 'empty', className: 'tmax-hint' }, t('empty'))
          : h('div', { key: 'models', className: 'tmax-models' },
            state.group.models
              .map(model => model.name === model.id ? model.id : `${model.name} (${model.id})`)
              .join(' · ')))
      }
      if (state.phase === 'failed') {
        children.push(h('div', { key: 'failure', className: 'tmax-error' }, `${t('failure')}: ${state.message}`))
      }
      if (message !== '') children.push(h('div', { key: 'message', className: 'tmax-error' }, message))
      if (!connected) children.push(h('div', { key: 'hint', className: 'tmax-hint' }, t('hint')))

      // Usage is a separate delivery from the catalog: one may be readable while
      // the other is not, so it renders on its own state.
      if (usage.phase === 'ready') {
        const rows = usageRows(usage.providers, t)
        if (rows.length > 0) {
          children.push(h('div', { key: 'divider', className: 'tmax-divider' }))
          children.push(h('div', { key: 'usage', className: 'tmax-usage' },
            h('div', { className: 'tmax-usage-title' }, t('usage')),
            rows.map(entry => {
              const name = entry.provider.name || entry.provider.id
              const dashboard = entry.provider.billing?.dashboard
              const label = dashboard === undefined
                ? h('span', { className: 'tmax-usage-name', title: name }, name)
                : h('a', { className: 'tmax-usage-name', href: dashboard, target: '_blank', rel: 'noreferrer', title: t('openDashboard') }, name)
              return h('div', { key: entry.provider.id, className: 'tmax-usage-row' },
                h('div', { className: 'tmax-usage-head' },
                  label,
                  entry.suspended ? h('span', { className: 'tmax-tag', 'data-tone': 'error' }, t('quotaSuspended')) : null,
                  entry.reset === undefined
                    ? null
                    : h('span', { className: 'tmax-usage-reset' }, `${t('resets')} ${entry.reset.toLocaleDateString()}`)),
                h('div', { className: 'tmax-usage-values' },
                  entry.readings.map((item, index) => h('span', { key: String(index), className: 'tmax-value', 'data-tone': entry.tone === 'error' && (item.remaining ?? 100) <= 0 ? 'error' : 'ok' }, item.text))))
            })))
        } else if (connected) {
          // Quota is only a claim worth making once access works; a first-run
          // card says what is missing instead of what it cannot measure yet.
          children.push(h('div', { key: 'divider', className: 'tmax-divider' }))
          children.push(h('div', { key: 'usage', className: 'tmax-hint' }, t('usageUnknown')))
        }
      }

      return h('section', { className: 'tmax-section' },
        h('div', { className: 'tmax-card' }, children))
    }

    let ctx

    function apply(context) {
      ctx = context
      context.effect(() => {
        // A non-window platform has no document; the card is simply never drawn there.
        if (typeof document === 'undefined') return
        const tag = document.createElement('style')
        tag.dataset.plugin = 'tmax'
        tag.textContent = STYLE
        document.head.appendChild(tag)
        return () => { tag.remove() }
      }, 'tmax: card styles')
      context.effect(() => context.locale.register(NS, { en, zh }), 'tmax: card locale')
      const t = context.locale.bind(NS)
      context.effect(() => context.slots.inject('settings.models.footer', () => context.slots.register({
        name: 'settings.models.footer',
        id: 'tmax',
        order: 10,
        locale: NS,
      }, props => h(TmaxCard, { t: props.t ?? t }))), 'tmax: models footer')
    }

    return { inject: ['slots', 'locale', 'remote', 'remote.session'], apply }
  },
})
