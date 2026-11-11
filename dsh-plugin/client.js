// Client half: tmax's own page in DSH, reached from the sidebar panel list.
//
// The page is a first-level entry beside Plugins and Schedules, so it owns a
// full content column instead of competing for room in a settings footer. It
// reads the Host model catalog and the secret-free usage route; it never runs a
// CLI itself and never receives a key. Every colour resolves through a
// `--dsw-alias-*` token so the page follows the light and dark themes.
window.__ModuleLoader__.load({
  id: 'tmax',
  factory(require) {
    const React = require('react')
    const NS = 'tmaxDshPage'
    const h = React.createElement

    const en = {
      panel: 'tmax',
      title: 'tmax',
      subtitle: 'Model access and provider limits managed by tmax.',
      loading: 'Reading model catalog…',
      connected: 'Connected',
      disconnected: 'Not connected',
      failed: 'Provider failed',
      empty: 'This profile exposes no tmax models yet.',
      hint: 'Connecting writes the DSH credential reference. The key is never copied or shown.',
      connect: 'Connect',
      connecting: 'Connecting…',
      connectFailed: 'Connect failed',
      refresh: 'Refresh',
      access: 'Access',
      target: 'Target',
      home: 'DSH home',
      gateway: 'Gateway',
      gatewayReady: 'ready',
      gatewayNotReady: 'not ready',
      protocol: 'Protocol',
      modelsSection: 'Models',
      modelsUnit: 'models',
      providerUnit: 'providers',
      usage: 'Usage and limits',
      usageUnknown: 'No provider reports a quota yet.',
      usageFailed: 'Usage is unavailable right now.',
      usageLive: 'Live',
      usageRevalidating: 'Checking provider quotas…',
      staleData: 'stale',
      minutesAgo: 'min ago',
      hoursAgo: 'h ago',
      daysAgo: 'd ago',
      justNow: 'just now',
      left: 'left',
      suspended: 'Suspended',
      dashboard: 'Provider dashboard',
      windowRolling: 'rolling',
      windowWeekly: 'weekly',
      windowMonthly: 'monthly',
      windowGeneric: 'limit',
      unitMinute: 'min',
      unitHour: 'h',
      blocker: 'Blocker',
      hostError: 'Host error',
      retry: 'Retry',
    }
    const zh = {
      panel: 'tmax',
      title: 'tmax',
      subtitle: '查看 tmax 管理的模型访问与供应商额度。',
      loading: '正在读取模型目录…',
      connected: '已连接',
      disconnected: '未连接',
      failed: 'provider 加载失败',
      empty: '当前 profile 还没有暴露 tmax 模型。',
      hint: '连接会写入 DSH 凭据引用，密钥不会被复制或显示。',
      connect: '连接',
      connecting: '连接中…',
      connectFailed: '连接失败',
      refresh: '刷新',
      access: '访问',
      target: '目标',
      home: 'DSH 目录',
      gateway: '网关',
      gatewayReady: '已就绪',
      gatewayNotReady: '未就绪',
      protocol: '协议',
      modelsSection: '模型',
      modelsUnit: '个模型',
      providerUnit: '个供应商',
      usage: '用量与额度',
      usageUnknown: '还没有 provider 上报额度。',
      usageFailed: '暂时拿不到用量数据。',
      usageLive: '实时',
      usageRevalidating: '正在刷新供应商额度…',
      staleData: '数据陈旧',
      minutesAgo: '分钟前',
      hoursAgo: '小时前',
      daysAgo: '天前',
      justNow: '刚刚',
      left: '剩余',
      suspended: '已暂停',
      dashboard: '供应商后台',
      windowRolling: '滚动',
      windowWeekly: '周',
      windowMonthly: '月',
      windowGeneric: '额度',
      unitMinute: '分钟',
      unitHour: '小时',
      blocker: '阻塞项',
      hostError: 'Host 错误',
      retry: '重试',
    }

    const STYLE = `
.tmx-page { display: flex; width: 100%; height: 100%; min-width: 0; min-height: 0; overflow: hidden; color: var(--dsw-alias-label-primary); background: var(--dsw-alias-bg-base); font-size: 14px; line-height: 1.6; }
.tmx-scroll { flex: 1; min-height: 0; overflow: auto; scrollbar-gutter: stable; --dsh-scrollbar-width: 9px; --dsh-scrollbar-thumb-border: 2px; }
.tmx-content { max-width: 960px; margin: 0 auto; padding: 0 clamp(24px, 4vw, 48px) 64px; }
.tmx-heading { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 16px; padding-top: 28px; margin-bottom: 8px; }
.tmx-heading h1 { flex: 0 1 auto; min-width: 0; max-width: 100%; margin: 0; font-size: 20px; line-height: 28px; font-weight: 500; }
.tmx-headState { display: inline-flex; align-items: center; gap: 8px; margin-left: 10px; font-size: 13px; line-height: 20px; font-weight: 400; color: var(--dsw-alias-label-secondary); vertical-align: middle; }
.tmx-actions { display: flex; flex: none; align-items: center; gap: 8px; white-space: nowrap; }
.tmx-sub { margin: 0 0 28px; font-size: 13px; line-height: 20px; color: var(--dsw-alias-label-secondary); }
.tmx-dot { box-sizing: border-box; display: inline-block; flex: none; width: 8px; height: 8px; border-radius: 50%; corner-shape: round; }
.tmx-dot[data-tone='ok'] { background: var(--dsw-alias-state-success-primary); }
.tmx-dot[data-tone='error'] { background: var(--dsw-alias-state-error-primary); }
.tmx-dot[data-tone='idle'] { background: var(--dsw-alias-label-tertiary); }
.tmx-button { box-sizing: border-box; display: inline-flex; align-items: center; justify-content: center; gap: 6px; height: 32px; padding: 0 14px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: var(--dsw-radius-md); background: transparent; color: var(--dsw-alias-label-primary); font: inherit; font-size: 13px; line-height: 20px; cursor: pointer; }
.tmx-button[data-kind='primary'] { border-color: transparent; background: var(--dsw-alias-button-primary-fill); color: var(--dsw-alias-label-primary-foreground); }
@media (hover: hover) {
  .tmx-button:hover:not(:disabled) { background: var(--dsw-alias-interactive-bg-hover-solid); }
  .tmx-button[data-kind='primary']:hover:not(:disabled) { background: var(--dsw-alias-button-primary-hover); }
}
.tmx-button:disabled { opacity: 0.4; cursor: default; }
.tmx-section { margin-bottom: 32px; }
.tmx-sectionTitle { display: flex; align-items: baseline; gap: 10px; margin: 0 0 12px; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-sectionNote { font-size: 12px; line-height: 18px; font-weight: 400; color: var(--dsw-alias-label-secondary); }
.tmx-card { border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-xl); padding: 16px 18px; }
.tmx-facts { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 14px 24px; margin: 0; }
.tmx-fact { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.tmx-fact dt { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-fact dd { margin: 0; font-size: 13px; line-height: 20px; overflow-wrap: anywhere; }
.tmx-factValue { display: inline-flex; align-items: center; gap: 6px; }
.tmx-mono { font-family: var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace); font-size: 12px; }
.tmx-chips { display: flex; flex-wrap: wrap; gap: 8px; margin: 0; padding: 0; list-style: none; }
.tmx-chip { display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: var(--dsw-radius-md); font-size: 12px; line-height: 18px; }
.tmx-chipKind { font-size: 11px; color: var(--dsw-alias-label-secondary); }
.tmx-note { margin: 12px 0 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-error { margin: 12px 0 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-state-error-primary); overflow-wrap: anywhere; }
.tmx-providers { display: flex; flex-direction: column; gap: 12px; }
.tmx-provider { display: flex; flex-direction: column; gap: 12px; }
.tmx-providerHead { display: flex; align-items: center; gap: 10px; min-width: 0; }
.tmx-providerName { display: inline-flex; align-items: center; gap: 5px; min-width: 0; font-size: 14px; line-height: 22px; font-weight: 500; color: var(--dsw-alias-label-primary); text-decoration: none; }
a.tmx-providerName:hover { text-decoration: underline; text-underline-offset: 2px; }
.tmx-external { flex: none; margin-top: 1px; color: var(--dsw-alias-label-secondary); }
.tmx-age { margin-left: auto; flex: none; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); font-variant-numeric: tabular-nums; }
.tmx-age[data-tone='warn'] { color: var(--dsw-alias-state-warn-primary); }
.tmx-providerHead a:hover .tmx-external { color: var(--dsw-alias-label-primary); }
.tmx-windows { display: flex; flex-direction: column; gap: 8px; margin: 0; padding: 0; list-style: none; }
.tmx-window { display: grid; grid-template-columns: 104px minmax(0, 1fr) minmax(52px, auto) minmax(84px, auto); align-items: center; gap: 12px; font-size: 12px; line-height: 18px; }
.tmx-windowName { color: var(--dsw-alias-label-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tmx-bar { position: relative; height: 6px; border-radius: 99px; background: var(--dsw-alias-border-l1); overflow: hidden; }
.tmx-barFill { display: block; height: 100%; border-radius: 99px; background: var(--dsw-alias-state-business-primary); transition: width var(--ds-transition-duration, 200ms) var(--ds-ease-in-out, ease); }
.tmx-barFill[data-tone='warn'] { background: var(--dsw-alias-state-warn-primary); }
.tmx-barFill[data-tone='error'] { background: var(--dsw-alias-state-error-primary); }
.tmx-windowValue { text-align: end; font-variant-numeric: tabular-nums; }
.tmx-windowValue[data-tone='warn'] { color: var(--dsw-alias-state-warn-primary); }
.tmx-windowValue[data-tone='error'] { color: var(--dsw-alias-state-error-primary); }
.tmx-windowReset { text-align: end; color: var(--dsw-alias-label-secondary); font-variant-numeric: tabular-nums; }
.tmx-windowReset[data-empty='true'] { visibility: hidden; }
.tmx-skeleton .tmx-bar { background: var(--dsw-alias-border-l1); overflow: hidden; }
.tmx-empty { margin: 0; font-size: 13px; line-height: 20px; color: var(--dsw-alias-label-secondary); }
@media (max-width: 720px) {
  .tmx-window { grid-template-columns: 88px minmax(0, 1fr) minmax(46px, auto); }
  .tmx-windowReset { display: none; }
}
`

    const WINDOW_NAMES = {
      rolling: 'windowRolling',
      weekly: 'windowWeekly',
      monthly: 'windowMonthly',
      weekly_allowance: 'windowWeekly',
    }

    function resetAt(reset) {
      const at = reset?.at ?? (typeof reset?.raw === 'number' ? reset.raw : undefined)
      if (at === undefined) return undefined
      const date = new Date(at)
      return Number.isNaN(date.getTime()) ? undefined : date
    }

    function windowName(label, index, t) {
      if (label !== null && typeof label === 'object') {
        if (typeof label.key === 'string' && WINDOW_NAMES[label.key] !== undefined) {
          return t(WINDOW_NAMES[label.key])
        }
        if (label.key === 'duration') {
          const count = label.duration ?? ''
          const unit = label.unit === 'TIME_UNIT_MINUTE' ? t('unitMinute') : label.unit === 'TIME_UNIT_HOUR' ? t('unitHour') : label.unit ?? ''
          return `${count} ${unit}`.trim()
        }
        const parts = [label.name, label.window].filter(part => typeof part === 'string' && part !== '')
        if (parts.length > 0) return parts.join(' ')
      }
      return `${t('windowGeneric')} ${index + 1}`
    }

    function toneFor(consumed) {
      return consumed >= 100 ? 'error' : consumed >= 80 ? 'warn' : 'ok'
    }

    /**
     * One window as a share of what it started with. tmax reports some windows
     * as consumed and others as remaining; both become the single consumed
     * fraction a filling bar can show.
     */
    function windowReading(row, index, t) {
      const name = windowName(row.label, index, t)
      const reset = resetAt(row.reset)
      if (typeof row.percent === 'number') {
        const consumed = row.basis === 'remaining_percent' ? 100 - row.percent : row.percent
        return {
          name,
          consumed: Math.max(0, Math.min(100, consumed)),
          text: `${Math.round(consumed)}%`,
          reset,
        }
      }
      const limit = row.counters?.limit
      if (limit !== undefined && limit !== null) {
        const total = Number(limit)
        const used = Number(row.counters.currentValue ?? row.counters.usage ?? (total - Number(row.counters.remaining)))
        const consumed = Number.isFinite(total) && total > 0 && Number.isFinite(used) ? (used / total) * 100 : undefined
        return {
          name,
          consumed: consumed === undefined ? undefined : Math.max(0, Math.min(100, consumed)),
          text: `${t('left')} ${row.counters.remaining ?? '?'}/${limit}`,
          reset,
        }
      }
      return undefined
    }

    /** Providers ordered by how much attention they need, each with its windows. */
    function providerRows(providers, t) {
      return providers
        .map(provider => {
          const windows = (provider.billing?.rows || [])
            .map((row, index) => windowReading(row, index, t))
            .filter(Boolean)
          if (windows.length === 0) return undefined
          const known = windows.filter(item => item.consumed !== undefined)
          const worst = known.reduce((max, item) => Math.max(max, item.consumed), 0)
          const suspended = provider.quota?.state === 'suspended'
          return {
            provider,
            windows,
            suspended,
            worst: known.length === 0 ? 0 : worst,
            tone: suspended || (known.length > 0 && worst >= 100) ? 'error' : worst >= 80 ? 'warn' : 'ok',
          }
        })
        .filter(Boolean)
        .sort((a, b) => Number(b.suspended) - Number(a.suspended) || b.worst - a.worst
          || (a.provider.name || a.provider.id).localeCompare(b.provider.name || b.provider.id))
    }

    /**
     * One GET route as component state. The mapper lives at the call site so this
     * hook keeps a stable `load`: an identity that changes per render would make
     * its effect refetch forever.
     */
    function useEndpoint(path) {
      const [state, setState] = React.useState({ phase: 'loading' })
      const load = React.useCallback(async () => {
        setState(previous => ({ phase: previous.value === undefined ? 'loading' : 'refreshing', value: previous.value }))
        try {
          const response = await fetch(new URL(path, document.baseURI), { credentials: 'same-origin' })
          if (!response.ok) throw new Error(`HTTP ${response.status}`)
          setState({ phase: 'ready', value: await response.json() })
        } catch (error) {
          setState({ phase: 'failed', message: error instanceof Error ? error.message : String(error) })
        }
      }, [path])
      React.useEffect(() => { void load() }, [load])
      return [state, load, setState]
    }

    function TmaxPage({ t }) {
      const [status, reloadStatus] = useEndpoint('/_tmax/status')
      const [usage, reloadUsage, setUsage] = useEndpoint('/_tmax/usage')
      const [catalog, setCatalog] = React.useState({ phase: 'loading' })
      const [busy, setBusy] = React.useState(false)
      const [message, setMessage] = React.useState('')

      const loadCatalog = React.useCallback(async () => {
        try {
          const response = await ctx.remote.session.modelCatalog()
          if (!response.ok) throw new Error(`${response.error.code}: ${response.error.message}`)
          const groups = response.value.groups || []
          const group = groups.find(item => item.id === 'tmax' || item.id.endsWith('tmax'))
          const failures = response.value.failures || []
          const failure = failures.find(item => item.id === 'tmax' || item.id.endsWith('tmax'))
          setCatalog(failure === undefined ? { phase: 'ready', group } : { phase: 'failed', message: failure.message })
        } catch (error) {
          setCatalog({ phase: 'failed', message: error instanceof Error ? error.message : String(error) })
        }
      }, [])

      React.useEffect(() => { void loadCatalog() }, [loadCatalog])

      const [quotaBusy, setQuotaBusy] = React.useState(false)
      const refreshQuota = React.useCallback(async () => {
        setQuotaBusy(true)
        try {
          const response = await fetch(new URL('/_tmax/usage/refresh', document.baseURI), {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'content-type': 'application/json' },
            body: '{}',
          })
          const value = response.ok ? await response.json() : undefined
          if (value?.ok === true) setUsage({ phase: 'ready', value })
        } catch {
          // A failed quota pass leaves the cached reading on screen, aged.
        } finally {
          setQuotaBusy(false)
        }
      }, [setUsage])

      // A quota page opened on purpose should show today's numbers: paint the
      // cached read, then re-observe without blocking that first paint.
      React.useEffect(() => { void refreshQuota() }, [refreshQuota])

      const refresh = async () => {
        setBusy(true)
        setMessage('')
        await Promise.all([reloadStatus(), loadCatalog(), refreshQuota()])
        setBusy(false)
      }

      const connect = async () => {
        setBusy(true)
        setMessage('')
        try {
          const response = await fetch(new URL('/_tmax/connect', document.baseURI), {
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
          await Promise.all([reloadStatus(), reloadUsage(), loadCatalog()])
        } catch (error) {
          setMessage(`${t('connectFailed')}: ${error instanceof Error ? error.message : String(error)}`)
        } finally {
          setBusy(false)
        }
      }

      const connected = catalog.phase === 'ready' && catalog.group !== undefined
      const tone = connected ? 'ok' : catalog.phase === 'failed' ? 'error' : 'idle'
      const statusText = connected ? t('connected') : catalog.phase === 'failed' ? t('failed') : catalog.phase === 'loading' ? t('loading') : t('disconnected')
      const gateway = status.value?.gateway
      const route = (status.value?.routes || [])[0]

      const providers = usage.value?.ok === true ? usage.value.providers || [] : []
      const usageFailed = usage.phase === 'failed' || (usage.phase !== 'loading' && usage.value !== undefined && usage.value.ok !== true)
      const rows = providers.length > 0 ? providerRows(providers, t) : []

      const gatewayDot = h('span', { className: 'tmx-dot', key: 'dot', 'data-tone': gateway?.ready === true ? 'ok' : 'idle' })
      const facts = [
        fact(t('target'), status.value?.target?.id),
        fact(t('home'), status.value?.target?.path, 'tmx-mono'),
        h('div', { className: 'tmx-fact', key: 'gateway' }, [
          h('dt', { key: 'dt' }, t('gateway')),
          h('dd', { key: 'dd' }, h('span', { className: 'tmx-factValue' }, [
            gatewayDot,
            h('span', { className: 'tmx-mono', key: 'listen' }, gateway?.listen ?? '—'),
            h('span', { key: 'state' }, gateway?.ready === true ? t('gatewayReady') : t('gatewayNotReady')),
          ])),
        ]),
        fact(t('protocol'), route?.protocol),
      ]
      const accessSection = h('section', { className: 'tmx-section', key: 'access' }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' }, t('access')),
        h('div', { className: 'tmx-card', key: 'card' }, [
          h('dl', { className: 'tmx-facts', key: 'facts' }, facts),
          ...blockerNotes(status.value, t),
        ]),
      ])

      const modelsSection = h('section', { className: 'tmx-section', key: 'models' }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' }, [
          t('modelsSection'),
          connected ? h('span', { className: 'tmx-sectionNote', key: 'note' }, `${catalog.group.models.length} ${t('modelsUnit')}`) : null,
        ]),
        h('div', { className: 'tmx-card', key: 'card' }, modelsBody(catalog, connected, t)),
      ])

      const usageSection = h('section', { className: 'tmx-section', key: 'usage' }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' }, [
          t('usage'),
          rows.length > 0 ? h('span', { className: 'tmx-sectionNote', key: 'note' }, `${rows.length} ${t('providerUnit')}`) : null,
          quotaBusy ? h('span', { className: 'tmx-sectionNote', key: 'busy' }, t('usageRevalidating')) : null,
        ]),
        usageBody(usage.phase, usageFailed, rows, t),
        usageFailed
          ? h('p', { className: 'tmx-note', key: 'retry' },
            h('button', { type: 'button', className: 'tmx-button', onClick: reloadUsage }, t('retry')))
          : null,
      ])

      const heading = h('div', { className: 'tmx-heading', key: 'heading' }, [
        h('h1', { key: 'title' }, [
          t('title'),
          h('span', { className: 'tmx-headState', key: 'state' }, [
            h('span', { className: 'tmx-dot', key: 'dot', 'data-tone': tone }),
            statusText,
          ]),
        ]),
        h('div', { className: 'tmx-actions', key: 'actions' }, [
          h('button', { type: 'button', className: 'tmx-button', key: 'refresh', onClick: refresh, disabled: busy },
            busy ? t('loading') : t('refresh')),
          connected
            ? null
            : h('button', {
              type: 'button',
              className: 'tmx-button',
              key: 'connect',
              'data-kind': 'primary',
              onClick: connect,
              disabled: busy,
            }, busy ? t('connecting') : t('connect')),
        ]),
      ])

      return h('div', { className: 'tmx-page', 'data-tmax-page': '1' },
        h('div', { className: 'tmx-scroll' },
          h('div', { className: 'tmx-content' }, [
            heading,
            h('p', { className: 'tmx-sub', key: 'sub' }, t('subtitle')),
            message === '' ? null : h('p', { className: 'tmx-error', key: 'message' }, message),
            accessSection,
            modelsSection,
            usageSection,
          ])))
    }

    function blockerNotes(status, t) {
      const notes = []
      const blockers = status?.blockers || []
      if (blockers.length > 0) {
        notes.push(h('p', { className: 'tmx-error', key: 'blockers' },
          `${t('blocker')}: ${blockers.map(blocker => typeof blocker === 'string' ? blocker : JSON.stringify(blocker)).join('; ')}`))
      }
      if (status?.lastError) {
        notes.push(h('p', { className: 'tmx-error', key: 'error' }, `${t('hostError')}: ${status.lastError}`))
      }
      return notes
    }

    function usageBody(phase, failed, rows, t) {
      if (failed) {
        return h('div', { className: 'tmx-card' },
          h('p', { className: 'tmx-empty' }, t('usageFailed')))
      }
      if (phase === 'loading') return h('div', { className: 'tmx-card tmx-skeleton' }, skeleton(t))
      if (rows.length === 0) {
        return h('div', { className: 'tmx-card' }, h('p', { className: 'tmx-empty' }, t('usageUnknown')))
      }
      return h('div', { className: 'tmx-providers' }, rows.map(entry => providerCard(entry, t)))
    }

    /** The models section states the reason it has nothing, not just nothing. */
    function modelsBody(catalog, connected, t) {
      if (catalog.phase === 'failed') {
        return [h('p', { className: 'tmx-empty', key: 'error' }, `${t('hostError')}: ${catalog.message}`)]
      }
      if (!connected) {
        return [
          h('p', { className: 'tmx-empty', key: 'empty' }, t('empty')),
          h('p', { className: 'tmx-note', key: 'hint' }, t('hint')),
        ]
      }
      if (catalog.group.models.length === 0) {
        return [h('p', { className: 'tmx-empty', key: 'empty' }, t('empty'))]
      }
      return [h('ul', { className: 'tmx-chips', key: 'chips' },
        catalog.group.models.map(model => h('li', { className: 'tmx-chip', key: model.id },
          h('span', null, model.name ?? model.id),
          model.name === model.id ? null : h('span', { className: 'tmx-chipKind' }, model.id))))]
    }

    /** A drawn "opens elsewhere" mark, at the size of the text it follows. */
    function ExternalMark() {
      return h('svg', {
        className: 'tmx-external',
        width: 11,
        height: 11,
        viewBox: '0 0 12 12',
        fill: 'none',
        stroke: 'currentColor',
        strokeWidth: 1.3,
        strokeLinecap: 'round',
        strokeLinejoin: 'round',
        'aria-hidden': 'true',
      },
        h('path', { d: 'M4.5 2.5H2.5v7h7V7.5' }),
        h('path', { d: 'M7 2.5h2.5V5' }),
        h('path', { d: 'M9.5 2.5 5.5 6.5' }))
    }

    function fact(label, value, className) {
      return h('div', { className: 'tmx-fact', key: label },
        h('dt', null, label),
        h('dd', { className }, value ?? '—'))
    }

    function skeleton(t) {
      return h('div', { className: 'tmx-windows' },
        [0, 1, 2].map(index => h('div', { className: 'tmx-window', key: index },
          h('span', { className: 'tmx-windowName' }, '—'),
          h('span', { className: 'tmx-bar' }),
          h('span', { className: 'tmx-windowValue' }, '—'),
          h('span', { className: 'tmx-windowReset' }, t('loading')))))
    }

    /** How long ago a quota reading was taken, in the reader's own units. */
    function ageText(observedAt, t) {
      if (typeof observedAt !== 'string') return undefined
      const at = new Date(observedAt)
      if (Number.isNaN(at.getTime())) return undefined
      const seconds = Math.max(0, Math.round((Date.now() - at.getTime()) / 1000))
      if (seconds < 45) return t('justNow')
      const minutes = Math.round(seconds / 60)
      if (minutes < 60) return `${minutes} ${t('minutesAgo')}`
      const hours = Math.round(minutes / 60)
      if (hours < 24) return `${hours} ${t('hoursAgo')}`
      return `${Math.round(hours / 24)} ${t('daysAgo')}`
    }

    /** A reading older than this is not worth presenting as a current fact. */
    const STALE_AFTER_MS = 5 * 60 * 1000

    function isStale(observedAt) {
      if (typeof observedAt !== 'string') return false
      const at = new Date(observedAt).getTime()
      return Number.isNaN(at) ? false : Date.now() - at > STALE_AFTER_MS
    }

    /** One provider: its name, its state, and one bar per window. */
    function providerCard(entry, t) {
      const { provider, windows, suspended, tone } = entry
      const name = provider.name || provider.id
      const dashboard = provider.billing?.dashboard
      const observedAt = provider.observedAt
      const age = ageText(observedAt, t)
      const stale = isStale(observedAt)
      return h('div', { className: 'tmx-card tmx-provider', key: provider.id },
        h('div', { className: 'tmx-providerHead' },
          dashboard === undefined
            ? h('span', { className: 'tmx-providerName', title: name }, name)
            : h('a', {
              className: 'tmx-providerName',
              href: dashboard,
              target: '_blank',
              rel: 'noreferrer',
              title: t('dashboard'),
            }, [name, h(ExternalMark, { key: 'mark' })]),
          suspended ? h('span', { className: 'tmx-chip', 'data-tone': 'error' }, t('suspended')) : null,
          age === undefined
            ? null
            : h('span', {
              className: 'tmx-age',
              'data-tone': stale ? 'warn' : undefined,
              title: observedAt,
            }, stale ? `${t('staleData')} · ${age}` : age)),
        h('ul', { className: 'tmx-windows' },
          windows.map((item, index) => {
            const itemTone = item.consumed === undefined ? 'ok' : toneFor(item.consumed)
            return h('li', { className: 'tmx-window', key: String(index) },
              h('span', { className: 'tmx-windowName', title: item.name }, item.name),
              h('span', { className: 'tmx-bar', role: 'meter', 'aria-valuenow': Math.round(item.consumed ?? 0), 'aria-valuemin': 0, 'aria-valuemax': 100, 'aria-label': `${name} ${item.name}` },
                h('span', { className: 'tmx-barFill', 'data-tone': itemTone, style: { width: `${item.consumed ?? 0}%` } })),
              h('span', { className: 'tmx-windowValue', 'data-tone': itemTone }, item.text),
              h('span', { className: 'tmx-windowReset', 'data-empty': item.reset === undefined ? 'true' : undefined },
                item.reset === undefined ? '' : `${item.reset.toLocaleDateString()}`))
          })))
    }

    /** The sidebar glyph: a gauge, because "how much is left" is what the page answers. */
    function TmaxIcon({ size = 16 }) {
      return h('svg', {
        width: size,
        height: size,
        viewBox: '0 0 16 16',
        fill: 'none',
        stroke: 'currentColor',
        strokeWidth: 1.4,
        strokeLinecap: 'round',
        'aria-hidden': 'true',
      },
        h('path', { d: 'M2.4 12.4a6.4 6.4 0 1 1 11.2 0' }),
        h('path', { d: 'M8 12.1 10.7 8.2' }))
    }

    let ctx

    function apply(context) {
      ctx = context
      context.effect(() => {
        // A non-window platform has no document; the page is simply never drawn there.
        if (typeof document === 'undefined') return
        const tag = document.createElement('style')
        tag.dataset.plugin = 'tmax'
        tag.textContent = STYLE
        document.head.appendChild(tag)
        return () => { tag.remove() }
      }, 'tmax: page styles')
      context.effect(() => context.locale.register(NS, { en, zh }), 'tmax: page locale')
      const t = context.locale.bind(NS)
      context.effect(() => context.slots.inject('sidebar.panellist', () => context.slots.register({
        name: 'sidebar.panellist',
        id: 'tmax',
        order: 20,
        label: () => t('panel'),
        locale: NS,
      }, TmaxIcon)), 'tmax: sidebar entry')
      context.effect(() => context.slots.inject('main', () => context.slots.register({
        name: 'main',
        key: 'tmax',
        locale: NS,
      }, props => h(TmaxPage, { t: props.t ?? t }))), 'tmax: page')
    }

    return { inject: ['slots', 'locale', 'remote', 'remote.session'], apply }
  },
})
