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
      stateReady: 'Available',
      statePartial: 'Quota unreadable',
      checking: 'Checking…',
      stateLimited: 'Limited',
      stateGateway: 'Gateway not ready',
      stateUnknown: 'Status unknown',
      stateUnreadable: 'Read failed',
      stateDisconnected: 'Not connected',
      empty: 'This profile exposes no tmax models yet.',
      hint: 'Connecting writes the DSH credential reference. The key is never copied or shown.',
      connect: 'Connect',
      connecting: 'Connecting…',
      connectFailed: 'Connect failed',
      refresh: 'Refresh',
      refreshing: 'Refreshing…',
      access: 'Access',
      target: 'Target',
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
      retryInline: 'Retry',
      unverified: 'unverified',
      availabilityUnknown: 'Availability unknown: the access read failed.',
      defaultModel: 'Default model',
      unknown: 'unknown',
      usageRevalidating: 'Checking provider quotas…',
      staleData: 'stale',
      minutesAgo: 'min ago',
      hoursAgo: 'h ago',
      daysAgo: 'd ago',
      justNow: 'just now',
      left: 'left',
      usedShare: 'used',
      suspended: 'Suspended',
      quotaSuspended: 'Quota exhausted',
      reset: 'resets',
      dashboard: 'Provider dashboard',
      windowRolling: 'rolling',
      windowWeekly: 'weekly',
      windowMonthly: 'monthly',
      windowCredit: 'credit',
      windowTokens: 'Token quota',
      windowTime: 'Time quota',
      windowGeneric: 'limit',
      quotaUnreadable: 'quota format not understood',
      unitMinute: 'min',
      unitHour: 'h',
      blocker: 'Blocker',
      hostError: 'Host error',
    }
    const zh = {
      panel: 'tmax',
      title: 'tmax',
      subtitle: '查看 tmax 管理的模型访问与供应商额度。',
      loading: '正在读取模型目录…',
      stateReady: '可用',
      statePartial: '额度读取失败',
      checking: '检查中…',
      stateLimited: '受限',
      stateGateway: '网关未就绪',
      stateUnknown: '状态未知',
      stateUnreadable: '读取失败',
      stateDisconnected: '未连接',
      empty: '当前 profile 还没有暴露 tmax 模型。',
      hint: '连接会写入 DSH 凭据引用，密钥不会被复制或显示。',
      connect: '连接',
      connecting: '连接中…',
      connectFailed: '连接失败',
      refresh: '刷新',
      refreshing: '刷新中…',
      access: '访问',
      target: '目标',
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
      retryInline: '重试',
      unverified: '未验证',
      availabilityUnknown: '可用性未知：访问信息读取失败。',
      defaultModel: '默认模型',
      unknown: '未知',
      usageRevalidating: '正在刷新供应商额度…',
      staleData: '数据陈旧',
      minutesAgo: '分钟前',
      hoursAgo: '小时前',
      daysAgo: '天前',
      justNow: '刚刚',
      left: '剩余',
      usedShare: '已用',
      suspended: '已暂停',
      quotaSuspended: '额度已用尽',
      reset: '重置',
      dashboard: '供应商后台',
      windowRolling: '滚动',
      windowWeekly: '周',
      windowMonthly: '月',
      windowCredit: '额度',
      windowTokens: 'Token 额度',
      windowTime: '时间额度',
      windowGeneric: '额度',
      quotaUnreadable: '额度格式无法解析',
      unitMinute: '分钟',
      unitHour: '小时',
      blocker: '阻塞项',
      hostError: 'Host 错误',
    }

    const STYLE = `
.tmx-page { display: flex; width: 100%; height: 100%; min-width: 0; min-height: 0; overflow: hidden; color: var(--dsw-alias-label-primary); background: var(--dsw-alias-bg-base); font-size: 14px; line-height: 1.6; }
.tmx-scroll { flex: 1; min-height: 0; overflow: auto; scrollbar-gutter: stable; --dsh-scrollbar-width: 9px; --dsh-scrollbar-thumb-border: 2px; }
.tmx-content { max-width: 960px; margin: 0 auto; padding: 0 clamp(24px, 4vw, 48px) 48px; }
.tmx-heading { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 16px; padding-top: 28px; margin-bottom: 8px; }
.tmx-heading h1 { flex: 0 1 auto; min-width: 0; max-width: 100%; margin: 0; font-size: 20px; line-height: 28px; font-weight: 500; }
.tmx-headState { display: inline-flex; align-items: center; gap: 8px; margin-left: 10px; font-size: 13px; line-height: 20px; font-weight: 400; color: var(--dsw-alias-label-secondary); vertical-align: middle; }
.tmx-headState[data-tone='warn'] { color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
.tmx-headState[data-tone='error'] { color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
/* A first-level page's header action is the sibling pages' 32px capsule. */
.tmx-actions { display: flex; flex: none; align-items: center; gap: 8px; white-space: nowrap; }
.tmx-button { box-sizing: border-box; display: inline-flex; align-items: center; justify-content: center; gap: 6px; height: 32px; padding: 0 12px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: 16px; background: transparent; color: var(--dsw-alias-label-primary); font: inherit; font-size: 13px; line-height: 20px; cursor: pointer; }
.tmx-button[data-kind='primary'] { border-color: transparent; background: var(--dsw-alias-button-primary-fill); color: var(--dsw-alias-label-primary-foreground); }
@media (hover: hover) {
  .tmx-button:hover:not(:disabled) { background: var(--dsw-alias-interactive-bg-hover-solid); }
  .tmx-button[data-kind='primary']:hover:not(:disabled) { background: var(--dsw-alias-button-primary-hover); }
}
.tmx-button:disabled { opacity: 0.4; cursor: default; }
.tmx-button[data-busy='true']:disabled { opacity: 1; background: var(--dsw-alias-interactive-bg-hover); }
.tmx-sub { margin: 0 0 28px; font-size: 13px; line-height: 20px; color: var(--dsw-alias-label-secondary); }
.tmx-dot { box-sizing: border-box; display: inline-block; flex: none; width: 8px; height: 8px; border-radius: 50%; corner-shape: round; }
.tmx-dot[data-tone='ok'] { background: var(--dsw-alias-state-success-primary); }
.tmx-dot[data-tone='warn'] { background: var(--dsw-alias-state-warn-primary); }
.tmx-dot[data-tone='error'] { background: var(--dsw-alias-state-error-primary); }
.tmx-dot[data-tone='idle'] { background: var(--dsw-alias-label-tertiary); }
.tmx-section { margin-bottom: 24px; }
.tmx-sectionTitle { display: flex; align-items: baseline; gap: 10px; margin: 0 0 12px; font-size: 14px; line-height: 22px; font-weight: 500; }
/* The section's own count is an annotation, never a second title. */
.tmx-sectionNote { font-size: 12px; line-height: 18px; font-weight: 400; color: var(--dsw-alias-label-secondary); }
.tmx-card { border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-xl); padding: 12px 14px; }
.tmx-facts { display: grid; grid-template-columns: repeat(auto-fit, minmax(170px, 1fr)); gap: 14px 24px; margin: 0; }
.tmx-fact { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.tmx-fact dt { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-fact dd { margin: 0; font-size: 13px; line-height: 20px; overflow-wrap: anywhere; }
.tmx-factValue { display: inline-flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.tmx-factNote { font-size: 11px; line-height: 16px; color: var(--dsw-alias-label-secondary); overflow-wrap: anywhere; }
.tmx-mono { font-family: var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace); font-size: 12px; }
.tmx-skeletonBar { display: block; width: 60%; height: 10px; border-radius: 4px; background: var(--dsw-alias-interactive-bg-hover); }
.tmx-chips { display: flex; flex-wrap: wrap; gap: 8px; margin: 0; padding: 0; list-style: none; }
.tmx-chip { display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: var(--dsw-radius-md); font-size: 12px; line-height: 18px; }
.tmx-chip[data-tone='error'] { border-color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 55%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-chipKind { font-size: 11px; color: var(--dsw-alias-label-secondary); }
.tmx-note { margin: 12px 0 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-errorText { font-size: 12px; line-height: 18px; color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); overflow-wrap: anywhere; }
.tmx-cardNote { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; margin-top: 12px; }
.tmx-providers { display: flex; flex-direction: column; gap: 12px; }
.tmx-provider { display: flex; flex-direction: column; gap: 10px; }
.tmx-providerHead { display: flex; align-items: center; gap: 10px; min-width: 0; }
.tmx-providerName { display: inline-flex; align-items: center; gap: 5px; min-width: 0; max-width: 100%; font-size: 14px; line-height: 22px; font-weight: 500; color: var(--dsw-alias-label-primary); text-decoration: none; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
a.tmx-providerName:hover { text-decoration: underline; text-underline-offset: 2px; }
.tmx-external { flex: none; margin-top: 1px; color: var(--dsw-alias-label-secondary); }
a.tmx-providerName:hover .tmx-external { color: var(--dsw-alias-label-primary); }
.tmx-age { margin-left: auto; flex: none; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); font-variant-numeric: tabular-nums; }
.tmx-age[data-tone='warn'] { color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
/* One grid per provider card, so every row and every card shares one 100% end. */
.tmx-windows { display: grid; grid-template-columns: 104px minmax(120px, 300px) minmax(52px, auto) minmax(84px, 1fr); align-items: center; gap: 8px 12px; margin: 0; padding: 0; list-style: none; }
.tmx-window { display: grid; grid-template-columns: subgrid; grid-column: 1 / -1; align-items: center; }
.tmx-windowName { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tmx-bar { position: relative; box-sizing: border-box; height: 8px; border-radius: 99px; background: var(--dsw-alias-border-l2); overflow: hidden; }
.tmx-bar[data-zero='true']::after { content: ''; position: absolute; inset-inline-start: 0; top: 0; width: 3px; height: 100%; background: var(--dsw-alias-label-tertiary); }
.tmx-barFill { display: block; height: 100%; border-radius: 99px; background: var(--dsw-alias-state-business-primary); transition: width var(--ds-transition-duration, 200ms) var(--ds-ease-in-out, ease); }
.tmx-barFill[data-tone='warn'] { background: var(--dsw-alias-state-warn-primary); }
.tmx-barFill[data-tone='error'] { background: var(--dsw-alias-state-error-primary); }
.tmx-windowValue { font-size: 12px; line-height: 18px; text-align: end; font-variant-numeric: tabular-nums; }
.tmx-windowValue[data-tone='warn'] { color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
.tmx-windowValue[data-tone='error'] { color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-windowReset { font-size: 12px; line-height: 18px; text-align: end; color: var(--dsw-alias-label-secondary); font-variant-numeric: tabular-nums; }
.tmx-windowReset[data-empty='true'] { visibility: hidden; }
.tmx-windowWide { grid-column: 2 / 4; text-align: start; color: var(--dsw-alias-label-secondary); }
.tmx-suspend { margin: 0; font-size: 12px; line-height: 18px; color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-empty { margin: 0; font-size: 13px; line-height: 20px; color: var(--dsw-alias-label-secondary); }
.tmx-sr { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip-path: inset(50%); white-space: nowrap; border: 0; }
@media (prefers-reduced-motion: reduce) { .tmx-barFill { transition: none; } }
/* Narrow or zoomed: the reset date moves under its row instead of disappearing. */
@media (max-width: 760px) {
  /* The bar keeps a fixed cap so every card shares one 100% end. */
  .tmx-windows { grid-template-columns: 88px minmax(72px, 220px) minmax(46px, auto) minmax(78px, 1fr); }
}
@media (max-width: 560px) {
  /* The bar spans the row, so every card keeps one shared 100% end; the number
     and the reset move beneath it rather than competing for its width. */
  .tmx-windows { grid-template-columns: minmax(0, 1fr) auto; }
  .tmx-windowName { grid-column: 1; grid-row: 1; }
  .tmx-windowValue { grid-column: 2; grid-row: 1; }
  .tmx-bar { grid-column: 1 / -1; grid-row: 2; }
  .tmx-windowReset { grid-column: 1 / -1; grid-row: 3; text-align: start; }
  .tmx-windowReset[data-empty='true'] { display: none; }
  .tmx-windowWide { grid-column: 1 / -1; grid-row: 1; text-align: end; }
}
`


    /** Big counters as the reader's language writes them (7.6B / 75.6亿). */
    function compact(value) {
      if (!Number.isFinite(value)) return '?'
      try {
        return new Intl.NumberFormat(dateLocale, { notation: 'compact', maximumFractionDigits: 1 }).format(value)
      } catch {
        return String(value)
      }
    }

    function formatTimestamp(value) {
      const date = new Date(value)
      return Number.isNaN(date.getTime()) ? '' : date.toLocaleString(dateLocale)
    }

    /** Dates read in the interface's language, not the browser's. */
    let dateLocale
    /** The app publishes the resolved language on <html lang>; follow it. */
    function syncDateLocale() {
      const lang = typeof document === 'undefined' ? undefined : document.documentElement.lang
      if (typeof lang === 'string' && lang !== '') dateLocale = lang
    }
    function formatDate(date) {
      try {
        return new Intl.DateTimeFormat(dateLocale, { year: 'numeric', month: 'short', day: 'numeric' }).format(date)
      } catch {
        return date.toISOString().slice(0, 10)
      }
    }

    function resetAt(reset) {
      const at = reset?.at ?? (typeof reset?.raw === 'number' ? reset.raw : undefined)
      if (at === undefined) return undefined
      const date = new Date(at)
      return Number.isNaN(date.getTime()) ? undefined : date
    }

    const LABEL_TYPES = { TOKENS_LIMIT: 'windowTokens', TIME_LIMIT: 'windowTime' }
    const LABEL_KEYS = { rolling: 'windowRolling', interval: 'windowRolling', weekly: 'windowWeekly', monthly: 'windowMonthly', weekly_allowance: 'windowWeekly' }

    /**
     * A window's name comes from whatever the provider actually discriminates by.
     * Some report a protobuf-ish type and a count, some a bucket name and a
     * window, some a duration. Inventing a name from an unmapped enum would be a
     * guess; showing none would make three rows identical.
     */
    function windowName(label, index, t) {
      if (label !== null && typeof label === 'object') {
        if (typeof label.type === 'string' && LABEL_TYPES[label.type] !== undefined) {
          return t(LABEL_TYPES[label.type])
        }
        if (label.key === 'duration') {
          const count = label.duration ?? ''
          const unit = label.unit === 'TIME_UNIT_MINUTE' ? t('unitMinute') : label.unit === 'TIME_UNIT_HOUR' ? t('unitHour') : label.unit ?? ''
          return `${count} ${unit}`.trim()
        }
        if (typeof label.key === 'string' && LABEL_KEYS[label.key] !== undefined) {
          return t(LABEL_KEYS[label.key])
        }
        if (label.key === 'credit') return t('windowCredit')
        const parts = [label.name, typeof label.window === 'string' ? t(LABEL_KEYS[label.window] ?? '') : '']
          .filter(part => typeof part === 'string' && part !== '')
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
      const raw = JSON.stringify(row.label ?? {})
      const reset = resetAt(row.reset)
      if (typeof row.percent === 'number') {
        const consumed = row.basis === 'remaining_percent' ? 100 - row.percent : row.percent
        return {
          name,
          consumed: Math.max(0, Math.min(100, consumed)),
          text: `${Math.round(consumed)}%`,
          spoken: `${t('usedShare')} ${Math.round(consumed)}%`,
          raw,
          reset,
        }
      }
      const counters = row.counters ?? {}
      const hasTotal = counters.limit !== undefined || counters.total !== undefined
      if (hasTotal) {
        const total = Number(counters.total ?? counters.limit)
        const left = Number(counters.remaining)
        const consumed = Number.isFinite(total) && total > 0 && Number.isFinite(left)
          ? ((total - left) / total) * 100
          : undefined
        return {
          name,
          consumed: consumed === undefined ? undefined : Math.max(0, Math.min(100, consumed)),
          text: `${t('left')} ${compact(left)} / ${compact(total)}`,
          spoken: `${t('left')} ${compact(left)} / ${compact(total)}`,
          raw,
          reset,
        }
      }
      // A row that exists but yields no share still names itself: a text line
      // with no meter, never a bar standing in for a measurement we cannot make.
      return { name, consumed: undefined, text: t('quotaUnreadable'), unreadable: true, raw, reset }
    }

    /**
     * Two windows a provider does not name apart would read identically. Number
     * them only when that happens, so the common single-window case stays clean.
     */
    function disambiguate(readings) {
      const counts = new Map()
      for (const item of readings) counts.set(item.name, (counts.get(item.name) ?? 0) + 1)
      const seen = new Map()
      return readings.map(item => {
        if ((counts.get(item.name) ?? 0) < 2) return item
        const index = (seen.get(item.name) ?? 0) + 1
        seen.set(item.name, index)
        return { ...item, name: `${item.name} ${index}` }
      })
    }

    /**
     * Every provider tmax reported, ordered by how much attention it needs.
     * A provider is never dropped: a page that silently omits one is worse than
     * a page that shows an unreadable row.
     */
    function providerRows(providers, t) {
      return providers
        .map(provider => {
          const windows = disambiguate((provider.billing?.rows || [])
            .map((row, index) => windowReading(row, index, t))
            .filter(Boolean))
          const suspended = provider.quota?.state === 'suspended'
          const known = windows.filter(item => item.consumed !== undefined)
          const worst = known.reduce((max, item) => Math.max(max, item.consumed), 0)
          return {
            provider,
            windows,
            suspended,
            quota: provider.quota,
            worst: known.length === 0 ? 0 : worst,
            tone: suspended || (known.length > 0 && worst >= 100) ? 'error' : worst >= 80 ? 'warn' : 'ok',
          }
        })
        .sort((a, b) => Number(b.suspended) - Number(a.suspended) || b.worst - a.worst
          || (a.provider.name || a.provider.id).localeCompare(b.provider.name || b.provider.id))
    }

    /** Why a provider is suspended and when it comes back, from tmax's own words. */
    function suspensionNote(quota, t) {
      if (quota?.state !== 'suspended') return undefined
      const parts = [t('quotaSuspended')]
      if (typeof quota.window === 'string' && LABEL_KEYS[quota.window] !== undefined) {
        parts.push(t(LABEL_KEYS[quota.window]))
      }
      const rawReset = typeof quota.resetAt === 'string' ? new Date(quota.resetAt) : resetAt(quota.resetAt)
      if (rawReset !== undefined && !Number.isNaN(rawReset.getTime())) {
        parts.push(`${t('reset')} ${formatDate(rawReset)}`)
      }
      return parts.join(' · ')
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
          // Keep what already loaded: a failed refresh must not delete a good
          // reading, it must label it.
          setState(previous => ({
            phase: 'failed',
            message: error instanceof Error ? error.message : String(error),
            value: previous.value,
          }))
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
      // Ages and staleness are functions of wall-clock time; without a tick they
      // would freeze at "just now" for as long as the tab stays open.
      const [, tick] = React.useReducer(value => value + 1, 0)
      React.useEffect(() => {
        const timer = setInterval(tick, 30_000)
        const onVisible = () => { if (document.visibilityState === 'visible') tick() }
        document.addEventListener('visibilitychange', onVisible)
        return () => {
          clearInterval(timer)
          document.removeEventListener('visibilitychange', onVisible)
        }
      }, [])

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
          else throw new Error(value?.error ?? `HTTP ${response.status}`)
        } catch (error) {
          // The cached reading stays, and the failure that produced it is named.
          setUsage(previous => ({
            ...previous,
            phase: 'failed',
            message: error instanceof Error ? error.message : String(error),
          }))
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

      const gateway = status.value?.gateway
      const protocols = [...new Set((status.value?.routes || []).map(item => item.protocol).filter(Boolean))]
      const connected = catalog.phase === 'ready' && catalog.group !== undefined
      const providers = usage.value?.ok === true ? usage.value.providers || [] : []
      const suspended = providers.filter(provider => provider.quota?.state === 'suspended').length
      const verdict = pageVerdict({ catalog, status, usage, connected, suspended, exhausted: anyExhausted(providers) }, t)
      const usageFailed = usage.phase === 'failed' || (usage.phase !== 'loading' && usage.value !== undefined && usage.value.ok !== true)
      const rows = providers.length > 0 ? providerRows(providers, t) : []

      // `ready` is genuinely three-valued: true, false, and never reported.
      const gatewayWord = gateway?.ready === true
        ? t('gatewayReady')
        : gateway?.ready === false ? t('gatewayNotReady') : t('unknown')
      const gatewayDot = h('span', {
        className: 'tmx-dot',
        key: 'dot',
        'data-tone': gateway?.ready === true ? 'ok' : gateway?.ready === false ? 'error' : 'idle',
      })
      const facts = [
        fact(t('target'), status.value?.target?.id),
        h('div', { className: 'tmx-fact', key: 'gateway' }, [
          h('dt', { key: 'dt' }, t('gateway')),
          h('dd', { key: 'dd' }, h('span', { className: 'tmx-factValue' }, [
            gatewayDot,
            h('span', { className: 'tmx-mono', key: 'listen' }, gateway?.listen ?? '—'),
            h('span', { key: 'state' }, gatewayWord),
          ])),
          gateway?.reason_code
            ? h('span', { className: 'tmx-factNote', key: 'reason' }, gateway.reason_code)
            : null,
        ]),
        fact(t('defaultModel'), status.value?.target?.default?.id),
        fact(t('protocol'), protocols.length > 0 ? protocols.join(' · ') : undefined),
      ]
      const accessSection = h('section', { className: 'tmx-section', key: 'access' }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' }, t('access')),
        h('div', { className: 'tmx-card', key: 'card' }, [
          status.phase === 'loading'
            ? h('div', { className: 'tmx-facts tmx-skeleton', key: 'loading' }, [0, 1, 2, 3].map(index =>
              h('div', { className: 'tmx-fact', key: index }, [
                h('dt', { key: 'dt' }, '—'),
                h('dd', { key: 'dd' }, h('span', { className: 'tmx-skeletonBar' })),
              ])))
            : h('dl', { className: 'tmx-facts', key: 'facts' }, facts),
          status.phase === 'failed'
            ? h('div', { className: 'tmx-cardNote', key: 'failure' }, [
              h('span', { className: 'tmx-errorText', key: 'text' },
                `${t('hostError')}: ${status.message ?? t('unknown')}`),
              h('button', { type: 'button', className: 'tmx-button', key: 'retry', onClick: reloadStatus }, t('retryInline')),
            ])
            : null,
          ...blockerNotes(status.value, t),
        ]),
      ])

      const modelsSection = h('section', { className: 'tmx-section', key: 'models' }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' }, [
          t('modelsSection'),
          connected ? h('span', { className: 'tmx-sectionNote', key: 'note' }, `${catalog.group.models.length} ${t('modelsUnit')}`) : null,
        ]),
        h('div', { className: 'tmx-card', key: 'card' }, modelsBody(catalog, connected, status.value?.routes, t)),
      ])

      const usageSection = h('section', { className: 'tmx-section', key: 'usage' }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' }, [
          t('usage'),
          rows.length > 0 ? h('span', { className: 'tmx-sectionNote', key: 'note' }, `${rows.length} ${t('providerUnit')}`) : null,
          quotaBusy ? h('span', { className: 'tmx-sectionNote', key: 'busy' }, t('usageRevalidating')) : null,
        ]),
        usage.phase === 'failed' && rows.length > 0
          ? h('p', { className: 'tmx-cardNote', key: 'stale' }, [
            h('span', { className: 'tmx-errorText', key: 'text' }, `${t('usageFailed')} ${usage.message ?? ''}`.trim()),
            h('button', { type: 'button', className: 'tmx-button', key: 'retry', onClick: refreshQuota }, t('retryInline')),
          ])
          : null,
        usageBody(usage.phase, usageFailed, rows, t, refreshQuota),
      ])

      const heading = h('div', { className: 'tmx-heading', key: 'heading' }, [
        h('h1', { key: 'title' }, [
          t('title'),
          h('span', { className: 'tmx-headState', key: 'state', 'data-tone': verdict.tone }, [
            h('span', { className: 'tmx-dot', key: 'dot', 'data-tone': verdict.tone }),
            verdict.text,
          ]),
        ]),
        h('div', { className: 'tmx-actions', key: 'actions' }, [
          h('button', { type: 'button', className: 'tmx-button', key: 'refresh', onClick: refresh, disabled: busy || quotaBusy, 'data-busy': busy || quotaBusy },
            busy ? t('refreshing') : t('refresh')),
          connected || catalog.phase !== 'ready'
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
            h('p', { className: 'tmx-sr', key: 'live', role: 'status', 'aria-live': 'polite' },
              quotaBusy ? t('usageRevalidating') : message !== '' ? message : ''),
            message === '' ? null : h('p', { className: 'tmx-errorText', key: 'message' }, message),
            accessSection,
            modelsSection,
            usageSection,
          ])))
    }

    /** A blocker is a record, not a string: name its message, fall back to its code. */
    function blockerText(blocker) {
      if (typeof blocker === 'string') return blocker
      if (blocker === null || typeof blocker !== 'object') return String(blocker)
      return blocker.message ?? blocker.reason_code ?? blocker.code ?? ''
    }

    function blockerNotes(status, t) {
      const notes = []
      const blockers = (status?.blockers || []).map(blockerText).filter(text => text !== '')
      if (blockers.length > 0) {
        notes.push(h('p', { className: 'tmx-errorText', key: 'blockers' },
          `${t('blocker')}: ${blockers.join('; ')}`))
      }
      if (status?.lastError) {
        notes.push(h('p', { className: 'tmx-errorText', key: 'error' }, `${t('hostError')}: ${status.lastError}`))
      }
      return notes
    }

    function usageBody(phase, failed, rows, t, onRetry) {
      if (phase === 'loading') return h('div', { className: 'tmx-card tmx-skeleton' }, skeleton(t))
      if (rows.length === 0) {
        return h('div', { className: 'tmx-card' }, [
          failed
            ? h('div', { className: 'tmx-cardNote', key: 'note' }, [
              h('span', { className: 'tmx-errorText', key: 'text' }, t('usageFailed')),
              h('button', { type: 'button', className: 'tmx-button', key: 'retry', onClick: onRetry }, t('retryInline')),
            ])
            : h('p', { className: 'tmx-empty', key: 'empty' }, t('usageUnknown')),
        ])
      }
      return h('div', { className: 'tmx-providers' },
        rows.map(entry => providerCard(entry, t)))
    }

    /** The models section states the reason it has nothing, not just nothing. */
    function modelsBody(catalog, connected, routes, t) {
      if (catalog.phase === 'failed') {
        return [h('div', { className: 'tmx-cardNote', key: 'error' }, [
          h('span', { className: 'tmx-errorText', key: 'text' }, `${t('hostError')}: ${catalog.message}`),
        ])]
      }
      if (!connected) {
        return catalog.phase === 'ready'
          ? [h('p', { className: 'tmx-empty', key: 'empty' }, t('empty')), h('p', { className: 'tmx-note', key: 'hint' }, t('hint'))]
          : [h('p', { className: 'tmx-empty', key: 'empty' }, t('loading'))]
      }
      if (catalog.group.models.length === 0) {
        return [h('p', { className: 'tmx-empty', key: 'empty' }, t('empty'))]
      }
      const kinds = new Map()
      for (const route of routes ?? []) {
        for (const model of route.models ?? []) {
          if (!kinds.has(model.id)) kinds.set(model.id, model)
        }
      }
      const unknownAvailability = (routes ?? []).length === 0
      const chips = h('ul', { className: 'tmx-chips', key: 'chips' },
        catalog.group.models.map(model => {
          const meta = kinds.get(model.id) ?? {}
          return h('li', { className: 'tmx-chip', key: model.id },
            h('span', null, model.name ?? model.id),
            model.name === model.id ? null : h('span', { className: 'tmx-chipKind' }, model.id),
            meta.availability === 'unverified'
              ? h('span', { className: 'tmx-chipKind', key: 'avail' }, t('unverified'))
              : null)
        }))
      return unknownAvailability
        ? [chips, h('p', { className: 'tmx-note', key: 'unknown' }, t('availabilityUnknown'))]
        : [chips]
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
          h('span', { className: 'tmx-windowReset' }, ''))))
    }

    /**
     * The one word in the header. It must be something the page can actually
     * stand behind: a projection existing in DSH is not "ready", and a gateway
     * whose readiness was never reported is not "not ready".
     */
    /** A window at 100% is a blocked route even when nothing was formally suspended. */
    function anyExhausted(providers) {
      return providers.some(provider => (provider.billing?.rows || []).some(row => {
        if (typeof row.percent === 'number') {
          return (row.basis === 'remaining_percent' ? 100 - row.percent : row.percent) >= 100
        }
        const total = Number(row.counters?.total ?? row.counters?.limit)
        const left = Number(row.counters?.remaining)
        return Number.isFinite(total) && total > 0 && Number.isFinite(left) && left <= 0
      }))
    }

    function pageVerdict({ catalog, status, usage, connected, suspended, exhausted }, t) {
      if (catalog.phase === 'loading' || status.phase === 'loading' || usage.phase === 'loading') {
        return { tone: 'idle', text: t('checking') }
      }
      if (catalog.phase === 'failed') return { tone: 'error', text: t('stateUnreadable') }
      if (!connected) {
        return catalog.phase === 'ready' ? { tone: 'idle', text: t('stateDisconnected') } : { tone: 'idle', text: t('checking') }
      }
      if (status.phase !== 'ready') return { tone: 'error', text: t('stateUnknown') }
      // Quota could not be read: access may be fine, but the page cannot claim
      // the whole picture is.
      if (usage.phase === 'failed') return { tone: 'warn', text: t('statePartial') }
      const ready = status.value?.gateway?.ready
      if (ready === false) return { tone: 'error', text: t('stateGateway') }
      if (ready !== true) return { tone: 'idle', text: t('stateUnknown') }
      if (suspended > 0 || exhausted) return { tone: 'warn', text: t('stateLimited') }
      return { tone: 'ok', text: t('stateReady') }
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
              title: `${name} · ${t('dashboard')}`,
            }, [name, h(ExternalMark, { key: 'mark' })]),
          suspended ? h('span', { className: 'tmx-chip', 'data-tone': 'error' }, t('suspended')) : null,
          age === undefined
            ? null
            : h('span', {
              className: 'tmx-age',
              'data-tone': stale ? 'warn' : undefined,
              title: formatTimestamp(observedAt),
            }, stale ? `${t('staleData')} · ${age}` : age)),
        suspensionLine(entry, t),
        windows.length === 0
          ? h('p', { className: 'tmx-empty', key: 'no-windows' }, t('usageUnknown'))
          : h('ul', { className: 'tmx-windows' },
            windows.map((item, index) => {
              const itemTone = item.consumed === undefined ? 'ok' : toneFor(item.consumed)
              // A row with no derivable share gets text, never a bar: a meter is
              // a claim about proportion, and inventing one would be a lie.
              if (item.unreadable === true) {
                return h('li', { className: 'tmx-window', key: String(index) },
                  h('span', { className: 'tmx-windowName', title: item.raw ?? item.name }, item.name),
                  h('span', { className: 'tmx-windowValue tmx-windowWide' }, item.text),
                  h('span', { className: 'tmx-windowReset', 'data-empty': item.reset === undefined ? 'true' : undefined },
                    item.reset === undefined ? '' : formatDate(item.reset)))
              }
              return h('li', { className: 'tmx-window', key: String(index) },
                h('span', { className: 'tmx-windowName', title: item.raw ?? item.name }, item.name),
                h('span', {
                  className: 'tmx-bar',
                  role: 'meter',
                  'aria-valuenow': Math.round(item.consumed ?? 0),
                  'aria-valuemin': 0,
                  'aria-valuemax': 100,
                  'aria-label': `${name} ${item.name}`,
                  'aria-valuetext': item.spoken,
                }, h('span', { className: 'tmx-barFill', 'data-tone': itemTone, style: { width: `${(item.consumed ?? 0) <= 0 ? 0 : Math.max(item.consumed ?? 0, 1.5)}%` } })),
                h('span', { className: 'tmx-windowValue', 'data-tone': itemTone }, item.text),
                h('span', { className: 'tmx-windowReset', 'data-empty': item.reset === undefined ? 'true' : undefined },
                  item.reset === undefined ? '' : formatDate(item.reset)))
            })))
    }

    /** The line under a provider's name: why it is suspended and when it returns. */
    function suspensionLine(entry, t) {
      const note = suspensionNote(entry.quota, t)
      return note === undefined ? null : h('p', { className: 'tmx-suspend', key: 'suspended' }, note)
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
      syncDateLocale()
      context.effect(() => context.locale.subscribe?.(syncDateLocale), 'tmax: date locale')
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
