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
      const status = state.phase === 'connected'
        ? t('connected')
        : state.phase === 'failed' ? t('failed') : state.phase === 'loading' ? t('loading') : t('disconnected')
      const color = state.phase === 'connected' ? '#15803d' : state.phase === 'failed' ? '#b91c1c' : '#6b7280'
      const children = [
        h('div', { key: 'head', style: { display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' } },
          h('span', { style: { width: 8, height: 8, borderRadius: 99, background: color, display: 'inline-block' } }),
          h('strong', null, `${t('title')}: ${status}`)),
        state.gateway !== undefined && h('div', { key: 'gateway', style: { color: '#6b7280', fontSize: 12 } },
          state.gateway && state.gateway.ready === true ? t('gatewayReady') : t('gatewayUnknown')),
      ]
      if (state.phase === 'connected') {
        children.push(h('div', { key: 'models' },
          h('div', { style: { fontWeight: 600, marginBottom: 4 } }, `${state.group.models.length} ${t('models')}`),
          h('ul', { style: { margin: 0, paddingLeft: 20, lineHeight: 1.7 } },
            state.group.models.map(model => h('li', { key: model.id },
              model.name === model.id ? model.id : `${model.name} · ${model.id}`)))))
      }
      if (state.phase === 'failed') children.push(h('div', { key: 'failure', style: { color: '#b91c1c' } }, `${t('failure')}: ${state.message}`))
      if (state.phase === 'disconnected') children.push(h('p', { key: 'empty', style: { margin: 0 } }, t('empty')))
      return h('div', { style: { display: 'grid', gap: 14, maxWidth: 720 } }, children)
    }

    function TmaxAction({ t, subject }) {
      const isTmax = subject?.kind === 'bundle' && subject.pkg?.name === 'tmax'
      const [phase, setPhase] = React.useState('idle')
      const [message, setMessage] = React.useState('')
      React.useEffect(() => {
        if (!isTmax) return
        let live = true
        fetch(new URL('_tmax/status', document.baseURI), { credentials: 'same-origin' })
          .then(response => response.ok ? response.json() : undefined)
          .then(status => { if (live && status?.gateway?.ready === true) setPhase('connected') })
          .catch(() => {})
        return () => { live = false }
      }, [isTmax])
      if (!isTmax) return null
      const connect = async () => {
        setPhase('connecting')
        setMessage('')
        try {
          const response = await fetch(new URL('_tmax/connect', document.baseURI), {
            method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json' }, body: '{}',
          })
          const result = await response.json().catch(() => ({}))
          if (!response.ok || result.ok !== true) {
            setPhase('idle')
            setMessage(`${t('connectFailed')}: ${result.message || result.code || response.status}`)
            return
          }
          setPhase('connected')
        } catch (error) {
          setPhase('idle')
          setMessage(`${t('connectFailed')}: ${error instanceof Error ? error.message : String(error)}`)
        }
      }
      return h('span', { style: { display: 'inline-flex', alignItems: 'center', gap: 8 } },
        h('button', { type: 'button', onClick: connect, disabled: phase === 'connecting' },
          phase === 'connected' ? t('connectedAction') : phase === 'connecting' ? t('connecting') : t('connectAction')),
        message ? h('span', { style: { color: '#b91c1c', fontSize: 12 } }, message) : null)
    }

    let ctx
    function apply(context) {
      ctx = context
      context.effect(() => context.locale.register(NS, { en, zh }), 'tmax: settings locale')
      const t = context.locale.bind(NS)
      context.effect(() => context.slots.inject('plugins.bundle.config', () => context.slots.register({
        name: 'plugins.bundle.config',
        key: 'tmax',
        locale: NS,
      }, TmaxPanel)), 'tmax: bundle page')
      context.effect(() => context.slots.inject('plugins.detail.actions', () => context.slots.register({
        name: 'plugins.detail.actions', id: 'tmax-connect', locale: NS,
      }, TmaxAction)), 'tmax: connect action')
    }

    return { inject: ['slots', 'locale', 'remote', 'remote.session'], apply }
  },
})
