// Client half: a read-only tmax section in Settings.
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
      title: 'tmax access',
      loading: 'Reading model catalog…',
      connected: 'Connected',
      disconnected: 'Not connected',
      failed: 'Provider failed',
      refresh: 'Refresh',
      connectTitle: 'Connect the existing DSH home',
      connectBody: 'Run this once in a terminal, then refresh:',
      connectCommand: 'tmax dsh connect --home "$DSH_HOME"',
      copy: 'Copy command',
      copied: 'Copied',
      empty: 'No tmax models are registered yet. Run the connect command, then refresh.',
      hint: 'This page is read-only. The command writes only the DSH credential reference; it never prints the key.',
      models: 'models',
      failure: 'Host error',
      connect: 'Connect now',
      connecting: 'Connecting…',
      connectDone: 'Connected. Refreshing model catalog…',
      connectFailed: 'Connect failed',
      gatewayReady: 'Gateway ready',
      gatewayUnknown: 'Gateway not ready',
      fallback: 'If the page cannot run the command, use the terminal command below.',
      providers: 'Provider access',
      quota: 'quota',
    }
    const zh = {
      title: 'tmax 访问',
      loading: '正在读取模型目录…',
      connected: '已连接',
      disconnected: '未连接',
      failed: 'provider 加载失败',
      refresh: '刷新',
      connectTitle: '连接现有 DSH home',
      connectBody: '在终端执行一次，然后刷新：',
      connectCommand: 'tmax dsh connect --home "$DSH_HOME"',
      copy: '复制命令',
      copied: '已复制',
      empty: '还没有注册 tmax 模型。执行连接命令后刷新。',
      hint: '本页只读。命令只写入 DSH 凭据引用，不会输出密钥。',
      models: '个模型',
      failure: 'Host 错误',
      connect: '立即连接',
      connecting: '连接中…',
      connectDone: '已连接，正在刷新模型目录…',
      connectFailed: '连接失败',
      gatewayReady: 'gateway 已就绪',
      gatewayUnknown: 'gateway 未就绪',
      fallback: '如果页面无法执行命令，使用下方终端命令。',
      providers: 'provider 访问状态',
      quota: '配额',
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

    function Status({ t }) {
      const [state, setState] = React.useState({ phase: 'loading' })
      const [copied, setCopied] = React.useState(false)
      const [connecting, setConnecting] = React.useState(false)
      const [message, setMessage] = React.useState('')
      const load = React.useCallback(async () => {
        setState({ phase: 'loading' })
        let gateway
        let access
        try {
          const [statusResponse, accessResponse] = await Promise.all([
            fetch(new URL('_tmax/status', document.baseURI), { credentials: 'same-origin' }),
            fetch(new URL('_tmax/access', document.baseURI), { credentials: 'same-origin' }),
          ])
          if (statusResponse.ok) gateway = (await statusResponse.json()).gateway
          if (accessResponse.ok) access = await accessResponse.json()
        } catch {}
        try {
          const response = await ctx.remote.session.modelCatalog()
          if (!response.ok) throw new Error(`${response.error.code}: ${response.error.message}`)
          const group = findGroup(response.value)
          const failure = findFailure(response.value)
          if (group) setState({ phase: 'connected', group, gateway, access })
          else if (failure) setState({ phase: 'failed', message: failure.message, gateway, access })
          else setState({ phase: 'disconnected', gateway, access })
        } catch (error) {
          setState({ phase: 'failed', message: error instanceof Error ? error.message : String(error), gateway, access })
        }
      }, [])
      React.useEffect(() => { void load() }, [load])
      const copy = async () => {
        try {
          await navigator.clipboard.writeText(t('connectCommand'))
          setCopied(true)
          setTimeout(() => setCopied(false), 1500)
        } catch {}
      }
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
            setConnecting(false)
            return
          }
          setMessage(t('connectDone'))
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
          h('button', { type: 'button', onClick: load }, t('refresh')),
          h('button', { type: 'button', onClick: connect, disabled: connecting }, connecting ? t('connecting') : t('connect'))),
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
      if (state.access && Array.isArray(state.access.providers) && state.access.providers.length > 0) {
        children.push(h('div', { key: 'access' },
          h('div', { style: { fontWeight: 600, marginBottom: 4 } }, t('providers')),
          h('ul', { style: { margin: 0, paddingLeft: 20, lineHeight: 1.7 } },
            state.access.providers.slice(0, 12).map(provider => h('li', { key: provider.id },
              `${provider.name || provider.id} · ${provider.authentication || 'unknown'} · ${provider.enabled ? 'enabled' : 'disabled'}`
              + (provider.quota && provider.quota !== 'unsupported' ? ` · ${t('quota')}: ${provider.quota}` : ''))))))
      }
      if (state.phase === 'failed') children.push(h('div', { key: 'failure', style: { color: '#b91c1c' } }, `${t('failure')}: ${state.message}`))
      if (state.phase === 'disconnected') children.push(h('p', { key: 'empty', style: { margin: 0 } }, t('empty')))
      if (message !== '') children.push(h('p', { key: 'message', style: { margin: 0 } }, message))
      children.push(h('div', {
        key: 'connect',
        style: { borderTop: '1px solid #e5e7eb', paddingTop: 12, display: 'grid', gap: 8 },
      },
      h('strong', null, t('connectTitle')),
      h('p', { style: { margin: 0 } }, t('connectBody')),
      h('div', { style: { display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' } },
        h('code', { style: { background: '#f3f4f6', padding: '6px 8px', borderRadius: 6, userSelect: 'all' } }, t('connectCommand')),
        h('button', { type: 'button', onClick: copy }, copied ? t('copied') : t('copy'))),
      h('p', { style: { margin: 0, color: '#6b7280', fontSize: 12 } }, t('hint')),
      h('p', { style: { margin: 0, color: '#6b7280', fontSize: 12 } }, t('fallback'))))
      return h('div', { style: { display: 'grid', gap: 14, maxWidth: 720 } }, children)
    }

    let ctx
    function apply(context) {
      ctx = context
      context.effect(() => context.locale.register(NS, { en, zh }), 'tmax: settings locale')
      const t = context.locale.bind(NS)
      context.effect(() => context.slots.inject('settings.section', () => context.slots.register({
        name: 'settings.section',
        id: 'tmax',
        order: 40,
        label: () => t('title'),
        locale: NS,
      }, Status)), 'tmax: settings section')
    }

    return { inject: ['slots', 'locale', 'remote'], apply }
  },
})
