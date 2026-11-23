// Client half: tmax's own page in DSH, reached from the sidebar panel list.
//
// The page is a first-level entry beside Plugins and Schedules, so it owns a
// full content column instead of competing for room in a settings footer. It
// reads one host read model (`GET /_tmax/state`); it never runs a CLI itself,
// never observes a provider on mount or on visibility, and never receives a
// key. A deliberate Refresh posts `/_tmax/observe`, Apply previews with
// `{ dryRun: true }` before it writes anything, and every repair action
// (`POST /_tmax/action`) is one exact operation shown as a typed result: a
// preview first, an explicit confirmation, and a failure never drawn as
// success.
//
// Every colour resolves through a `--dsw-alias-*` token so the page follows the
// light and dark themes.
window.__ModuleLoader__.load({
  id: 'tmax',
  factory(require) {
    const React = require('react')
    const NS = 'tmaxDshPage'
    const h = React.createElement

    const en = {
      // Shared chrome.
      panel: 'tmax',
      title: 'tmax',
      subtitle: 'Model access and provider limits managed by tmax.',
      colon: ':',
      unknown: 'unknown',
      none: 'none',
      notReported: 'not reported',
      neverObserved: 'never observed',
      loading: 'Reading tmax state…',
      checking: 'Checking…',
      refresh: 'Refresh',
      refreshing: 'Refreshing…',
      retry: 'Retry',
      close: 'Close',
      yes: 'yes',
      no: 'no',
      updated: 'updated',
      configRevision: 'config',
      schema: 'schema',
      // Header verdict. It answers reachability only.
      stateReady: 'Reachable',
      stateGateway: 'Gateway not ready',
      stateUnknown: 'Status unknown',
      stateUnreadable: 'Read failed',
      stateDisconnected: 'Not connected',
      stateNotUsable: 'Registered, not usable',
      stateReload: 'Reload pending',
      stateInactive: 'Configured, not active',
      stateConflict: 'Route conflict',
      stateProjection: 'Routes unavailable',
      stateRouteDisabled: 'Route management disabled',
      stateSchemaUnsupported: 'Unsupported state schema',
      stateLastKnown: 'Last known',
      // Next action: only an action that can change the reported state is offered.
      nextActionTitle: 'Next action',
      nextConnect: 'Apply the tmax routes and credential reference to this profile so DSH can use them.',
      nextReload: 'The home integration is installed but its routes are not live. Reload DSH to activate them.',
      nextConflict: 'A route ownership conflict is reported; resolve it before changing access.',
      nextGateway: 'The tmax gateway is not ready; inspect it in the tmax CLI.',
      nextBlocker: 'tmax reports a blocker that prevents access.',
      nextConfig: 'The tmax configuration is invalid; fix it in the tmax CLI.',
      nextConfigUnavailable: 'The tmax configuration could not be validated; inspect it in the tmax CLI.',
      nextModeDisabled: 'Route management is disabled by manageRoutes=false; install the home integration or set it to auto.',
      nextProjectionRetry: 'The route projection could not be read; retry the host read.',
      nextSectionRetry: 'A section of the host state could not be read; retry the state read.',
      nextRetry: 'The tmax host state could not be read.',
      // Status and access.
      accessTitle: 'Status and access',
      target: 'Target',
      targetPath: 'Home path',
      authority: 'tmax authority',
      dshInstallation: 'DSH install',
      ownership: 'Route ownership',
      ownerHome: 'home integration',
      ownerPage: 'this page',
      ownerNone: 'none',
      ownerOther: 'another integration',
      ownerUnknown: 'unknown',
      scopeHomeWide: 'home-wide',
      scopeProfileScoped: 'profile-scoped',
      ownershipMode: 'Mode',
      modeAuto: 'auto',
      modeEnabled: 'enabled',
      modeDisabled: 'disabled',
      homeEntry: 'entry',
      homeArtifact: 'artifact',
      present: 'present',
      absent: 'absent',
      conflict: 'Conflict',
      applied: 'Applied',
      appliedApplied: 'applied',
      appliedPending: 'pending',
      pending: 'Pending reload',
      pendingNone: 'none',
      tmaxDefault: 'tmax default',
      effectiveDefault: 'DSH effective default',
      differs: 'differs',
      matches: 'matches',
      routesTitle: 'Routes',
      routeProtocol: 'Protocol',
      routeBaseUrl: 'Base URL',
      routeScope: 'Scope',
      routeCredential: 'Credential',
      credentialConfigured: 'configured',
      credentialMissing: 'registered, not usable',
      credentialRef: 'ref',
      credentialsTitle: 'Credentials',
      credentialPath: 'path',
      credentialState: 'state',
      credentialPermissions: 'permissions',
      noRoutes: 'The host reported no routes.',
      routeActive: 'Active',
      routeLive: 'live',
      routeNotRegistered: 'not registered',
      routeBlockers: 'Route blockers',
      // Gateway detail.
      gatewayTitle: 'Gateway',
      gatewayConfigured: 'Configured',
      gatewayReady: 'Ready',
      gatewayNotReady: 'Not ready',
      gatewayListen: 'Address',
      gatewayDaemon: 'Daemon',
      gatewayReason: 'reason',
      gatewaySupervision: 'Supervision',
      gatewayAutostart: 'Autostart',
      savedIntent: 'saved intent',
      registration: 'registration',
      unit: 'unit',
      unitMatches: 'matches',
      unitDrift: 'drift',
      sharedKey: 'Shared key',
      daemonIdentified: 'identified',
      daemonUnidentified: 'unidentified',
      supervisionAvailable: 'available',
      supervisionUnavailable: 'unavailable',
      keyLoaded: 'loaded',
      keyAbsent: 'absent',
      registrationRegistered: 'registered',
      registrationUnavailable: 'unavailable',
      // Config.
      configTitle: 'Config',
      configState: 'State',
      configValid: 'valid',
      configInvalid: 'invalid',
      configUnavailable: 'unavailable',
      configErrors: 'Errors',
      // Models and routing.
      modelsTitle: 'Models and routing',
      modelsNote: 'Availability is tmax evidence, not a DSH claim.',
      modelKind: 'Kind',
      modelContinuity: 'Continuity',
      modelCandidates: 'Candidates',
      modelAvailability: 'Availability',
      capabilities: 'Capabilities',
      contextWindow: 'Context',
      inputModalities: 'Input',
      reasoningEfforts: 'Efforts',
      kindAlias: 'alias',
      kindConcrete: 'concrete',
      continuityStateless: 'stateless',
      continuityStateful: 'stateful',
      availabilityUnverified: 'unverified',
      availabilityVerified: 'verified',
      routingSelected: 'Selects',
      routingPriority: 'Priority',
      routingCandidates: 'Candidates',
      routingExcluded: 'Excluded',
      routingError: 'Routing error',
      routingNone: 'No routing evidence reported for this model.',
      selected: 'selected',
      modelsUnit: 'models',
      // Providers and limits.
      providersTitle: 'Providers and limits',
      tmaxProviders: 'tmax providers',
      tmaxProvidersNote: 'Quota readings are tmax observations with their own time; they are not live facts.',
      agentProviders: 'Providers this DSH profile configures',
      agentProvidersNote: 'Observed independently of tmax; kept separate and never summed.',
      providerDisabled: 'disabled',
      providerSuspended: 'suspended',
      quotaSuspended: 'Quota exhausted',
      affectedTargets: 'targets affected',
      authentication: 'auth',
      integration: 'integration',
      blocker: 'Blocker',
      billingKind: 'quota kind',
      billingPlanAllowance: 'plan allowance',
      billingUnknown: 'unknown',
      freshness: 'freshness',
      freshnessFresh: 'fresh',
      freshnessStale: 'stale',
      freshnessMissing: 'missing',
      freshnessUnqualified: 'unqualified',
      observedAt: 'observed',
      lastAttempt: 'last attempt',
      oldestReading: 'oldest reading',
      unknownQuota: 'quota unknown (no observation reported)',
      unknownQuotaUnsupported: 'quota unknown (no supported source)',
      noQuotaRows: 'no quota rows reported',
      providerError: 'provider error',
      failureOverride: 'failure override',
      failureActive: 'active',
      consecutiveFailures: 'consecutive failures',
      expires: 'expires',
      plan: 'plan',
      renewal: 'renewal',
      dashboard: 'Provider dashboard',
      windowsCaption: 'window · value · reset',
      usedShare: 'used',
      left: 'left',
      reset: 'resets',
      resetUnqualified: 'reset time unqualified',
      windowRolling: 'rolling',
      windowWeekly: 'weekly',
      windowMonthly: 'monthly',
      windowCredit: 'credit',
      windowTokens: 'token quota',
      windowTime: 'time quota',
      windowGeneric: 'limit',
      unitMinute: 'min',
      unitHour: 'h',
      quotaUnreadable: 'quota format not understood',
      notes: 'notes',
      noteCountersUnitUnqualified: 'counter units unqualified',
      noteUsageMayOverlap: 'usage and limits may overlap',
      noteStatusEnumsUnqualified: 'status enums unqualified',
      notePercentFloored: 'consumed percent floored',
      noteCreditNotTokens: 'credit is not tokens',
      noteFieldsUnqualified: 'fields unqualified',
      baseUrl: 'base URL',
      models: 'models',
      agentState: 'state',
      agentObserved: 'observed',
      agentNotObserved: 'not observed',
      agentFailed: 'failed',
      reasonCredential: 'credential unavailable',
      reasonUpstream: 'the vendor refused the quota read',
      reasonFormat: 'the vendor answered in a shape this page does not understand',
      reasonTransport: 'the vendor is unreachable',
      reasonDeadline: 'the vendor did not answer in time',
      reasonFailed: 'the vendor reports this plan as unusable',
      reasonUnsupported: 'no supported quota source',
      reasonUnknown: 'the quota read failed',
      // Diagnostics.
      diagnosticsTitle: 'Diagnostics',
      tmaxVersion: 'tmax version',
      projectionSchema: 'state schema',
      generatedAt: 'state generated',
      lastProjection: 'last inventory read',
      history: 'Request history',
      historyAvailable: 'available',
      historyUnavailable: 'unavailable',
      reasonDaemonIdentity: 'daemon identity unavailable',
      reasonServiceManager: 'service manager unavailable',
      lastError: 'last error',
      noLastError: 'none reported',
      revision: 'configuration revision',
      // Observation results.
      observeRunning: 'Observing provider sources…',
      observeDone: 'Observation finished',
      observeFailed: 'Observation failed',
      observeAt: 'finished',
      observeSourceTmax: 'tmax',
      observeSourceAgent: 'agent',
      observeNoItems: 'No observation outcome reported.',
      observeHint: 'Only this button contacts provider sources; opening the page reads cached state.',
      observeStatusRefreshed: 'refreshed',
      observeStatusBlocked: 'blocked',
      observeStatusFailed: 'failed',
      observeStatusUnchanged: 'unchanged',
      // Apply. The host route keeps its compatibility name `/_tmax/connect`; the
      // user-facing operation is `tmax agent apply`, which writes the target's
      // native DSH files.
      connect: 'Apply',
      connectPreview: 'Apply preview',
      connectPreviewNote: 'No changes are written yet. Applying writes the target\'s DSH routes and credential reference; the key never enters the browser.',
      connectApplying: 'Applying…',
      connectApply: 'Confirm apply',
      connectCancel: 'Cancel',
      connectApplied: 'Applied; tmax state refreshed.',
      connectFailed: 'Apply failed',
      connectNoDetails: 'The host reported no change details.',
      connectRefs: 'references',
      connectChanged: 'changed',
      connectEffects: 'effects',
      connectBlockers: 'blockers',
      connectNextActions: 'next actions',
      connectDetail: 'detail',
      connectState: 'state',
      connectFiles: 'planned files',
      connectChangedFiles: 'changed files',
      connectUnchangedFiles: 'unchanged files',
      connectRecovery: 'recovery',
      recoveryPlanned: 'planned',
      recoveryRetained: 'retained',
      connectFactGaps: 'fact gaps',
      actionWouldWrite: 'would write',
      actionWritten: 'written',
      actionUnchanged: 'unchanged',
      effectNativeWrites: 'native writes',
      effectSecretReads: 'secret reads',
      effectProcessesStarted: 'processes started',
      effectLocksCreated: 'locks created',
      // Explicit repair actions. Each button names one fixed tmax operation.
      actionsLabel: 'Actions',
      actionResumeQuota: 'Resume quota',
      actionResetFailure: 'Reset failure',
      actionObserveQuota: 'Observe quota',
      actionOpenManagement: 'Open management',
      webListener: 'Web listener',
      webRunning: 'running',
      webStopped: 'not running',
      webUrl: 'URL',
      actionPreviewTitle: 'Action preview',
      actionDone: 'Action finished',
      actionFailed: 'Action failed',
      actionWorking: 'Working…',
      actionPreviewNote: 'Nothing has run yet. Confirming runs this one tmax operation; its result is reported below.',
      actionConfirm: 'Confirm',
      actionCancel: 'Cancel',
      actionNoDetails: 'The host reported no action detail.',
      actionCode: 'code',
      actionStatus: 'status',
      actionMessage: 'message',
      actionData: 'data',
      actionStartGateway: 'Start gateway',
      actionStopGateway: 'Stop gateway',
      actionRestartGateway: 'Restart gateway',
      actionGatewayNote: 'Starting or stopping the gateway can take longer and affects every client that uses it.',
      // Phase 3 seats: one marker, one settings card, one routing pane, one tool card.
      markerUnknown: 'tmax state unknown',
      markerReady: 'tmax ready',
      markerNoRoute: 'no tmax route projected',
      markerInactive: 'tmax route not active',
      markerCredential: 'tmax credential not usable',
      markerCredentialUnknown: 'tmax credential readiness unknown',
      markerGatewayUnknown: 'gateway readiness unknown',
      markerGatewayDown: 'gateway not ready',
      markerSuspended: 'quota suspended',
      markerConflict: 'route conflict',
      markerBlocker: 'a blocker is reported',
      markerConfigError: 'the tmax configuration is invalid',
      markerConfigUnavailable: 'the tmax configuration could not be validated',
      markerSectionError: 'a tmax read failed',
      footerTitle: 'tmax models',
      footerGateway: 'gateway',
      footerRoutes: 'routes',
      footerModels: 'models',
      footerActive: 'active',
      footerUsable: 'usable',
      footerReadFailed: 'read failed',
      footerReason: 'why no models appear',
      modelsAvailable: 'tmax models are listed.',
      modelsNone: 'No tmax model is projected for this profile.',
      modelsInactive: 'The tmax route is configured but not active in DSH.',
      modelsCredential: 'The tmax route has no usable credential reference.',
      modelsCredentialUnknown: 'The tmax route credential readiness is unknown.',
      modelsReload: 'A DSH reload is pending before the tmax route can be used.',
      modelsUnknown: 'The tmax state could not be read.',
      footerOpenPage: 'Open tmax',
      footerPageUnavailable: 'The tmax page is not available in this composition.',
      footerReload: 'Reload',
      footerLoading: 'Reading tmax state…',
      routingOption: 'tmax routing',
      routingPaneTitle: 'tmax routing',
      routingNotTmax: 'The selected model is not provided by tmax.',
      routingReload: 'Reload routing',
      routingUnknown: 'The tmax routing evidence could not be read.',
      dismiss: 'Dismiss',
      toolTitle: 'tmax status',
      toolPending: 'reading tmax state…',
      toolUnreadable: 'the tmax status result could not be read',
      toolFailed: 'tmax status reported a failure',
      toolTarget: 'target',
      toolRoutes: 'routes',
      toolProviders: 'providers',
      toolBlockers: 'blockers',
      toolModels: 'models',
      // Request history detail (explicit load; never read on mount).
      historyAvailability: 'availability',
      historyLoad: 'Load history',
      historyReload: 'Reload history',
      historyLoading: 'Loading retained request history…',
      historyReadUnavailable: 'Request history is unavailable.',
      historyEmpty: 'No retained request records were reported.',
      historyCoverage: 'Coverage',
      historyProcess: 'process',
      historyRetained: 'retained records',
      historyEvicted: 'evicted records',
      historyIncomplete: 'incomplete captures',
      historyPriorProcesses: 'prior processes',
      historyDirectTraffic: 'direct traffic',
      historyRange: 'range',
      historyRetention: 'retention',
      historyGap: 'gap',
      historyMore: 'more retained',
      historyTime: 'time',
      historyOutcome: 'outcome',
      historyRequestedModel: 'requested',
      historySelectedModel: 'selected',
      historyRoute: 'target / provider',
      historyReason: 'reason',
      historyGeneratedAt: 'generated',
      historyUnknown: 'coverage unknown',
      // Failures.
      readFailed: 'Read failed',
      hostError: 'Host error',
      hostStatus: 'The tmax host answered',
      hostUnreachable: 'Cannot reach the tmax host',
      staleNotice: 'showing the last successful read',
    }
    const zh = {
      // Shared chrome.
      panel: 'tmax',
      title: 'tmax',
      subtitle: '由 tmax 管理的模型访问与供应商额度。',
      colon: '：',
      unknown: '未知',
      none: '无',
      notReported: '未报告',
      neverObserved: '从未观测',
      loading: '正在读取 tmax 状态…',
      checking: '检查中…',
      refresh: '刷新',
      refreshing: '刷新中…',
      retry: '重试',
      close: '关闭',
      yes: '是',
      no: '否',
      updated: '更新于',
      configRevision: '配置',
      schema: 'schema',
      // Header verdict.
      stateReady: '可用',
      stateGateway: '网关未就绪',
      stateUnknown: '状态未知',
      stateUnreadable: '读取失败',
      stateDisconnected: '未连接',
      stateNotUsable: '已注册，但不可用',
      stateReload: '待重新加载',
      stateInactive: '已配置，未生效',
      stateConflict: '路由冲突',
      stateProjection: '路由不可用',
      stateRouteDisabled: '路由管理已禁用',
      stateSchemaUnsupported: '状态 schema 不受支持',
      stateLastKnown: '上次已知',
      // Next action.
      nextActionTitle: '下一步',
      nextConnect: '将 tmax 路由和凭据引用应用到当前 profile，使 DSH 可以使用它们。',
      nextReload: '已安装 home 集成，但其路由尚未生效。重新加载 DSH 以启用。',
      nextConflict: '上报了路由归属冲突；先解决冲突再更改访问。',
      nextGateway: 'tmax 网关未就绪；请在 tmax CLI 中检查。',
      nextBlocker: 'tmax 上报了阻止访问的阻塞项。',
      nextConfig: 'tmax 配置无效；请在 tmax CLI 中修复。',
      nextConfigUnavailable: '无法校验 tmax 配置；请在 tmax CLI 中检查。',
      nextModeDisabled: 'manageRoutes=false 已禁用路由管理；请安装 home 集成或设为 auto。',
      nextProjectionRetry: '无法读取路由投影；请重试 host 读取。',
      nextSectionRetry: 'host 状态中有部分内容读取失败；请重新读取状态。',
      nextRetry: '无法读取 tmax host 状态。',
      // Status and access.
      accessTitle: '状态与访问',
      target: '目标',
      targetPath: 'Home 路径',
      authority: 'tmax 数据目录',
      dshInstallation: 'DSH 安装',
      ownership: '路由归属',
      ownerHome: 'home 集成',
      ownerPage: '本页',
      ownerNone: '无',
      ownerOther: '其他集成',
      ownerUnknown: '未知',
      scopeHomeWide: '整个 home',
      scopeProfileScoped: '仅当前 profile',
      ownershipMode: '模式',
      modeAuto: '自动',
      modeEnabled: '启用',
      modeDisabled: '禁用',
      homeEntry: '入口',
      homeArtifact: '产物',
      present: '存在',
      absent: '缺失',
      conflict: '冲突',
      applied: '已应用',
      appliedApplied: '已应用',
      appliedPending: '待应用',
      pending: '待重新加载',
      pendingNone: '无',
      tmaxDefault: 'tmax 默认',
      effectiveDefault: 'DSH 生效默认',
      differs: '不一致',
      matches: '一致',
      routesTitle: '路由',
      routeProtocol: '协议',
      routeBaseUrl: 'Base URL',
      routeScope: '作用域',
      routeCredential: '凭据',
      credentialConfigured: '已配置',
      credentialMissing: '已注册，但不可用',
      credentialRef: '引用',
      credentialsTitle: '凭据',
      credentialPath: '路径',
      credentialState: '状态',
      credentialPermissions: '权限',
      noRoutes: 'Host 未报告任何路由。',
      routeActive: '生效状态',
      routeLive: '已生效',
      routeNotRegistered: '未注册',
      routeBlockers: '路由阻塞项',
      // Gateway detail.
      gatewayTitle: '网关',
      gatewayConfigured: '已配置',
      gatewayReady: '就绪',
      gatewayNotReady: '未就绪',
      gatewayListen: '监听地址',
      gatewayDaemon: '守护进程',
      gatewayReason: '原因',
      gatewaySupervision: '服务托管',
      gatewayAutostart: '开机自启',
      savedIntent: '保存的意图',
      registration: '注册状态',
      unit: '单元',
      unitMatches: '匹配',
      unitDrift: '偏离',
      sharedKey: '共享密钥',
      daemonIdentified: '已识别',
      daemonUnidentified: '未识别',
      supervisionAvailable: '可用',
      supervisionUnavailable: '不可用',
      keyLoaded: '已加载',
      keyAbsent: '未加载',
      registrationRegistered: '已注册',
      registrationUnavailable: '不可用',
      // Config.
      configTitle: '配置',
      configState: '状态',
      configValid: '有效',
      configInvalid: '无效',
      configUnavailable: '不可用',
      configErrors: '错误',
      // Models and routing.
      modelsTitle: '模型与路由',
      modelsNote: '可用性属于 tmax 的证据，不是 DSH 的声明。',
      modelKind: '类型',
      modelContinuity: '连续性',
      modelCandidates: '候选数',
      modelAvailability: '可用性',
      capabilities: '能力',
      contextWindow: '上下文',
      inputModalities: '输入',
      reasoningEfforts: '推理强度',
      kindAlias: '别名',
      kindConcrete: '具体模型',
      continuityStateless: '无状态',
      continuityStateful: '有状态',
      availabilityUnverified: '未验证',
      availabilityVerified: '已验证',
      routingSelected: '选中',
      routingPriority: '优先级',
      routingCandidates: '候选',
      routingExcluded: '排除',
      routingError: '路由错误',
      routingNone: 'Host 未报告该模型的路由证据。',
      selected: '已选中',
      modelsUnit: '个模型',
      // Providers and limits.
      providersTitle: '供应商与额度',
      tmaxProviders: 'tmax 供应商',
      tmaxProvidersNote: '额度读数是带自身时间的 tmax 观测，不是实时事实。',
      agentProviders: '当前 DSH profile 配置的供应商',
      agentProvidersNote: '独立于 tmax 观测；单独列出，从不合并。',
      providerDisabled: '已停用',
      providerSuspended: '已暂停',
      quotaSuspended: '额度已用尽',
      affectedTargets: '影响目标数',
      authentication: '认证',
      integration: '集成',
      blocker: '阻塞项',
      billingKind: '额度类型',
      billingPlanAllowance: '套餐额度',
      billingUnknown: '未知',
      freshness: '新鲜度',
      freshnessFresh: '新鲜',
      freshnessStale: '过期',
      freshnessMissing: '缺失',
      freshnessUnqualified: '未定级',
      observedAt: '观测于',
      lastAttempt: '最近尝试',
      oldestReading: '最早读数',
      unknownQuota: '额度未知（未报告观测）',
      unknownQuotaUnsupported: '额度未知（没有支持的来源）',
      noQuotaRows: '未报告额度明细',
      providerError: '供应商错误',
      failureOverride: '故障覆盖',
      failureActive: '生效中',
      consecutiveFailures: '连续失败',
      expires: '到期',
      plan: '套餐',
      renewal: '续期',
      dashboard: '供应商后台',
      windowsCaption: '窗口 · 数值 · 重置',
      usedShare: '已用',
      left: '剩余',
      reset: '重置',
      resetUnqualified: '重置时间未定级',
      windowRolling: '滚动',
      windowWeekly: '周',
      windowMonthly: '月',
      windowCredit: '额度',
      windowTokens: 'Token 额度',
      windowTime: '时间额度',
      windowGeneric: '额度',
      unitMinute: '分钟',
      unitHour: '小时',
      quotaUnreadable: '额度格式无法解析',
      notes: '备注',
      noteCountersUnitUnqualified: '计数单位未定级',
      noteUsageMayOverlap: '用量与额度可能重叠',
      noteStatusEnumsUnqualified: '状态枚举未定级',
      notePercentFloored: '已用百分比已取整',
      noteCreditNotTokens: '额度不是 token 计量',
      noteFieldsUnqualified: '字段未定级',
      baseUrl: 'Base URL',
      models: '模型',
      agentState: '状态',
      agentObserved: '已观测',
      agentNotObserved: '未观测',
      agentFailed: '失败',
      reasonCredential: '凭据不可用',
      reasonUpstream: '厂商拒绝了这次额度读取',
      reasonFormat: '厂商返回的格式无法解析',
      reasonTransport: '无法连接到厂商',
      reasonDeadline: '厂商未在时限内响应',
      reasonFailed: '厂商报告该套餐当前不可用',
      reasonUnsupported: '没有支持的额度来源',
      reasonUnknown: '额度读取失败',
      // Diagnostics.
      diagnosticsTitle: '诊断',
      tmaxVersion: 'tmax 版本',
      projectionSchema: 'state schema',
      generatedAt: '状态生成于',
      lastProjection: '最近清单读取',
      history: '请求历史',
      historyAvailable: '可用',
      historyUnavailable: '不可用',
      reasonDaemonIdentity: '守护进程身份不可用',
      reasonServiceManager: '服务管理器不可用',
      lastError: '最近错误',
      noLastError: '未报告',
      revision: '配置版本',
      // Observation results.
      observeRunning: '正在观测供应商来源…',
      observeDone: '观测完成',
      observeFailed: '观测失败',
      observeAt: '完成于',
      observeSourceTmax: 'tmax',
      observeSourceAgent: 'agent',
      observeNoItems: '未报告观测结果。',
      observeHint: '只有此按钮会访问供应商来源；打开页面只读取缓存状态。',
      observeStatusRefreshed: '已刷新',
      observeStatusBlocked: '被阻止',
      observeStatusFailed: '失败',
      observeStatusUnchanged: '未变化',
      // 应用。Host 路由保留兼容名称 `/_tmax/connect`；面向用户的操作是
      // `tmax agent apply`，它写入目标的 DSH 原生文件。
      connect: '应用',
      connectPreview: '应用预览',
      connectPreviewNote: '尚未写入任何更改。应用会写入目标的 DSH 路由和凭据引用；密钥不会进入浏览器。',
      connectApplying: '正在应用…',
      connectApply: '确认应用',
      connectCancel: '取消',
      connectApplied: '已应用；tmax 状态已刷新。',
      connectFailed: '应用失败',
      connectNoDetails: 'Host 未报告变更细节。',
      connectRefs: '引用',
      connectChanged: '变更',
      connectEffects: '影响',
      connectBlockers: '阻塞项',
      connectNextActions: '下一步',
      connectDetail: '细节',
      connectState: '状态',
      connectFiles: '计划写入的文件',
      connectChangedFiles: '变更文件',
      connectUnchangedFiles: '未变文件',
      connectRecovery: '恢复',
      recoveryPlanned: '计划',
      recoveryRetained: '保留',
      connectFactGaps: '事实缺口',
      actionWouldWrite: '将写入',
      actionWritten: '已写入',
      actionUnchanged: '未变化',
      effectNativeWrites: '原生写入',
      effectSecretReads: '密钥读取',
      effectProcessesStarted: '启动进程',
      effectLocksCreated: '创建锁',
      // 显式修复操作：每个按钮对应一项固定的 tmax 操作。
      actionsLabel: '操作',
      actionResumeQuota: '恢复额度',
      actionResetFailure: '重置失败状态',
      actionObserveQuota: '观测额度',
      actionOpenManagement: '打开管理界面',
      webListener: 'Web 监听',
      webRunning: '运行中',
      webStopped: '未运行',
      webUrl: 'URL',
      actionPreviewTitle: '操作预览',
      actionDone: '操作完成',
      actionFailed: '操作失败',
      actionWorking: '处理中…',
      actionPreviewNote: '尚未执行。确认后只运行这一项 tmax 操作；结果显示在下方。',
      actionConfirm: '确认',
      actionCancel: '取消',
      actionNoDetails: 'Host 未报告操作细节。',
      actionCode: '代码',
      actionStatus: '状态',
      actionMessage: '消息',
      actionData: '数据',
      actionStartGateway: '启动网关',
      actionStopGateway: '停止网关',
      actionRestartGateway: '重启网关',
      actionGatewayNote: '启动或停止网关可能耗时较长，并会影响所有使用它的客户端。',
      // 第三阶段席位：一个标记、一张设置卡片、一个路由面板、一张工具结果卡片。
      markerUnknown: 'tmax 状态未知',
      markerReady: 'tmax 可用',
      markerNoRoute: '未投影 tmax 路由',
      markerInactive: 'tmax 路由未生效',
      markerCredential: 'tmax 凭据不可用',
      markerCredentialUnknown: 'tmax 凭据就绪状态未知',
      markerGatewayUnknown: '网关就绪状态未知',
      markerGatewayDown: '网关未就绪',
      markerSuspended: '额度已暂停',
      markerConflict: '路由冲突',
      markerBlocker: '存在阻塞项',
      markerConfigError: 'tmax 配置无效',
      markerConfigUnavailable: '无法校验 tmax 配置',
      markerSectionError: '部分 tmax 读取失败',
      footerTitle: 'tmax 模型',
      footerGateway: '网关',
      footerRoutes: '路由',
      footerModels: '模型',
      footerActive: '生效',
      footerUsable: '可用',
      footerReadFailed: '读取失败',
      footerReason: '未显示模型的原因',
      modelsAvailable: '已列出 tmax 模型。',
      modelsNone: '此配置未投影任何 tmax 模型。',
      modelsInactive: 'tmax 路由已配置，但在 DSH 中未生效。',
      modelsCredential: 'tmax 路由没有可用的凭据引用。',
      modelsCredentialUnknown: '无法确定 tmax 路由的凭据就绪状态。',
      modelsReload: '需要重新加载 DSH 后 tmax 路由才能使用。',
      modelsUnknown: '无法读取 tmax 状态。',
      footerOpenPage: '打开 tmax',
      footerPageUnavailable: '当前组合中不可打开 tmax 页面。',
      footerReload: '重新读取',
      footerLoading: '正在读取 tmax 状态…',
      routingOption: 'tmax 路由',
      routingPaneTitle: 'tmax 路由',
      routingNotTmax: '当前模型不是由 tmax 提供的。',
      routingReload: '重新读取路由',
      routingUnknown: '无法读取 tmax 路由信息。',
      dismiss: '关闭',
      toolTitle: 'tmax 状态',
      toolPending: '正在读取 tmax 状态…',
      toolUnreadable: '无法读取 tmax 状态结果',
      toolFailed: 'tmax 状态报告失败',
      toolTarget: '目标',
      toolRoutes: '路由',
      toolProviders: '供应商',
      toolBlockers: '阻塞项',
      toolModels: '模型',
      // 请求历史详情：只在显式加载时读取，挂载时不读取。
      historyAvailability: '可用性',
      historyLoad: '加载历史',
      historyReload: '重新加载历史',
      historyLoading: '正在加载保留的请求历史…',
      historyReadUnavailable: '请求历史不可用。',
      historyEmpty: '未报告保留的请求记录。',
      historyCoverage: '覆盖范围',
      historyProcess: '进程',
      historyRetained: '保留记录',
      historyEvicted: '已淘汰记录',
      historyIncomplete: '不完整记录',
      historyPriorProcesses: '之前进程',
      historyDirectTraffic: '直连流量',
      historyRange: '时间范围',
      historyRetention: '保留策略',
      historyGap: '缺口',
      historyMore: '条更多保留记录',
      historyTime: '时间',
      historyOutcome: '结果',
      historyRequestedModel: '请求模型',
      historySelectedModel: '选中模型',
      historyRoute: '目标 / 供应商',
      historyReason: '原因',
      historyGeneratedAt: '生成于',
      historyUnknown: '覆盖范围未知',
      // Failures.
      readFailed: '读取失败',
      hostError: 'Host 错误',
      hostStatus: 'tmax Host 返回',
      hostUnreachable: '无法连接到 tmax Host',
      staleNotice: '显示上次成功读取的结果',
    }

    // Enum tables: the host reports machine values; the page translates the
    // ones it knows and shows any other value verbatim (bounded) rather than
    // inventing a word for it.
    const OWNER_STATES = { home: 'ownerHome', page: 'ownerPage', none: 'ownerNone', other: 'ownerOther', unknown: 'ownerUnknown' }
    const ROUTE_STATES = { active: 'routeLive', inactive: 'routeNotRegistered' }
    const SCOPES = { home_wide: 'scopeHomeWide', profile_scoped: 'scopeProfileScoped' }
    const APPLIED_STATES = { applied: 'appliedApplied', pending: 'appliedPending', unknown: 'unknown' }
    // `compatible` is tmax's real token for an identified daemon; `identified` is
    // the host's normalized word for the same fact.
    const DAEMON_STATES = { identified: 'daemonIdentified', compatible: 'daemonIdentified', unidentified: 'daemonUnidentified', unknown: 'unknown' }
    const SUPERVISION_STATES = { available: 'supervisionAvailable', unavailable: 'supervisionUnavailable', unknown: 'unknown' }
    const REGISTRATION_STATES = { registered: 'registrationRegistered', unavailable: 'registrationUnavailable', unknown: 'unknown' }
    // `ready` is tmax's real shared-key token; `loaded` is the normalized word.
    const KEY_STATES = { loaded: 'keyLoaded', ready: 'keyLoaded', absent: 'keyAbsent', unknown: 'unknown' }
    const CONFIG_STATES = { valid: 'configValid', invalid: 'configInvalid', unavailable: 'configUnavailable' }
    const KIND_STATES = { alias: 'kindAlias', concrete: 'kindConcrete' }
    const CONTINUITY_STATES = { stateless: 'continuityStateless', stateful: 'continuityStateful' }
    const AVAILABILITY_STATES = { verified: 'availabilityVerified', unverified: 'availabilityUnverified', unknown: 'unknown' }
    const FRESHNESS_STATES = { fresh: 'freshnessFresh', stale: 'freshnessStale', missing: 'freshnessMissing', unqualified: 'freshnessUnqualified', unknown: 'unknown' }
    const BILLING_KINDS = { plan_allowance: 'billingPlanAllowance', unknown: 'billingUnknown' }
    const HISTORY_STATES = { available: 'historyAvailable', unavailable: 'historyUnavailable', unknown: 'unknown' }
    const FAILURE_STATES = { active: 'failureActive' }
    const AGENT_STATES = { observed: 'agentObserved', not_observed: 'agentNotObserved', failed: 'agentFailed', unknown: 'unknown' }
    const REASONS = { daemon_identity_unavailable: 'reasonDaemonIdentity', service_manager_unavailable: 'reasonServiceManager' }
    const AGENT_REASONS = {
      credential_unavailable: 'reasonCredential',
      upstream_rejection: 'reasonUpstream',
      source_format_error: 'reasonFormat',
      transport_error: 'reasonTransport',
      observation_deadline: 'reasonDeadline',
      observation_failed: 'reasonFailed',
      unsupported_source: 'reasonUnsupported',
      observation_unknown: 'reasonUnknown',
    }
    const NOTE_KEYS = {
      status_enums_unqualified: 'noteStatusEnumsUnqualified',
      percent_consumed_floored: 'notePercentFloored',
      credit_not_tokens: 'noteCreditNotTokens',
      fields_unqualified: 'noteFieldsUnqualified',
      counters_unit_unqualified: 'noteCountersUnitUnqualified',
      usage_and_limits_may_overlap: 'noteUsageMayOverlap',
    }
    const MODE_STATES = { auto: 'modeAuto', true: 'modeEnabled', false: 'modeDisabled' }
    // Host section names are its own; the page recognizes the spellings it can
    // route and shows anything else in Diagnostics only.
    const SECTION_ALIASES = {
      projection: ['projection', 'projection_read', 'routes'],
      providers: ['providers', 'provider_list', 'provider_list_read', 'providers_read'],
      target: ['target', 'target_read', 'agent', 'agent_inspect', 'agent_inspect_read'],
      gateway: ['gateway', 'gateway_read', 'gateway_status'],
      config: ['config', 'config_read', 'config_validate'],
      agentSettings: ['agent_settings', 'agent-settings', 'agent_providers', 'observations'],
    }
    const PENDING_RELOAD_REASONS = ['home_integration_pending_reload', 'home_integration_reload_pending']
    const OBSERVE_STATUS = {
      refreshed: 'observeStatusRefreshed',
      blocked: 'observeStatusBlocked',
      failed: 'observeStatusFailed',
      unchanged: 'observeStatusUnchanged',
    }
    const REFERENCE_ACTIONS = { would_write: 'actionWouldWrite', written: 'actionWritten', unchanged: 'actionUnchanged' }
    // Only an explicit state names the web listener's lifecycle; a null record
    // is not evidence that it is stopped.
    const WEB_STATES = { running: 'webRunning', stopped: 'webStopped' }
    const CONNECT_EFFECTS = [
      ['native_writes', 'effectNativeWrites'],
      ['secret_reads', 'effectSecretReads'],
      ['processes_started', 'effectProcessesStarted'],
      ['locks_created', 'effectLocksCreated'],
    ]
    // Explicit repair actions: one fixed tmax operation per action and one exact
    // operand set. The page never composes an argv, never chains an action, and
    // never retries one on its own.
    const GATEWAY_ACTIONS = ['gateway_start', 'gateway_stop', 'gateway_restart']
    const ACTION_KINDS = ['resume_quota', 'reset_failure', 'observe_provider', 'open_management', ...GATEWAY_ACTIONS]
    const ACTION_LABELS = {
      resume_quota: 'actionResumeQuota',
      reset_failure: 'actionResetFailure',
      observe_provider: 'actionObserveQuota',
      open_management: 'actionOpenManagement',
      gateway_start: 'actionStartGateway',
      gateway_stop: 'actionStopGateway',
      gateway_restart: 'actionRestartGateway',
    }
    const ACTION_SCOPES = ['quota', 'auth', 'models', 'all']
    const ACTION_FAILURE_STATUSES = ['failed', 'blocked', 'refused', 'rejected', 'error']
    const LABEL_TYPES = { TOKENS_LIMIT: 'windowTokens', TIME_LIMIT: 'windowTime' }
    const LABEL_KEYS = { rolling: 'windowRolling', interval: 'windowRolling', weekly: 'windowWeekly', monthly: 'windowMonthly', weekly_allowance: 'windowWeekly' }

    const STYLE = `
.tmx-page { display: flex; width: 100%; height: 100%; min-width: 0; min-height: 0; overflow: hidden; color: var(--dsw-alias-label-primary); background: var(--dsw-alias-bg-base); font-size: 14px; line-height: 1.6; }
.tmx-scroll { flex: 1; min-height: 0; overflow: auto; scrollbar-gutter: stable; --dsh-scrollbar-width: 9px; --dsh-scrollbar-thumb-border: 2px; }
.tmx-content { max-width: 960px; margin: 0 auto; padding: 0 clamp(24px, 4vw, 48px) 48px; }
.tmx-heading { display: flex; flex-wrap: wrap; align-items: flex-start; justify-content: space-between; gap: 16px; padding-top: 28px; margin-bottom: 8px; }
.tmx-titleBlock { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.tmx-heading h1 { flex: 0 1 auto; min-width: 0; max-width: 100%; margin: 0; font-size: 20px; line-height: 28px; font-weight: 500; }
.tmx-headState { display: inline-flex; align-items: center; gap: 8px; margin-left: 10px; font-size: 13px; line-height: 20px; font-weight: 400; color: var(--dsw-alias-label-secondary); vertical-align: middle; }
.tmx-headState[data-tone='warn'] { color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
.tmx-headState[data-tone='error'] { color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-meta { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
/* A first-level page's header action is the sibling pages' 32px capsule. */
.tmx-actions { display: flex; flex: none; align-items: center; gap: 12px; white-space: nowrap; }
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
.tmx-sectionTitle { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px; margin: 0 0 12px; font-size: 16px; line-height: 24px; font-weight: 500; }
/* The section's own count is an annotation, never a second title. */
.tmx-sectionNote { font-size: 12px; line-height: 18px; font-weight: 400; color: var(--dsw-alias-label-secondary); }
.tmx-subhead { margin: 20px 0 8px; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-caption { margin: -4px 0 10px; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-card { border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-xl); padding: 12px 14px; }
.tmx-facts { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 14px 24px; margin: 0; }
@media (max-width: 900px) { .tmx-facts { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
.tmx-fact { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.tmx-fact dt { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-fact dd { margin: 0; font-size: 13px; line-height: 20px; overflow-wrap: anywhere; }
.tmx-factValue { display: inline-flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.tmx-factNote { font-size: 11px; line-height: 16px; color: var(--dsw-alias-label-secondary); overflow-wrap: anywhere; }
.tmx-mono { font-family: var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace); font-size: 12px; }
.tmx-break { overflow-wrap: anywhere; }
.tmx-skeletonBar { display: block; width: 60%; height: 10px; border-radius: 4px; background: var(--dsw-alias-interactive-bg-hover); }
.tmx-chips { display: flex; flex-wrap: wrap; gap: 8px; margin: 0; padding: 0; list-style: none; }
.tmx-chip { display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: var(--dsw-radius-md); font-size: 12px; line-height: 18px; }
.tmx-chip[data-tone='error'] { border-color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 55%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-chip[data-tone='warn'] { border-color: color-mix(in srgb, var(--dsw-alias-state-warn-primary) 55%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
.tmx-chip[data-tone='ok'] { border-color: color-mix(in srgb, var(--dsw-alias-state-success-primary) 50%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-success-primary) 75%, var(--dsw-alias-label-primary)); }
.tmx-chipKind { font-size: 11px; color: var(--dsw-alias-label-secondary); }
.tmx-note { margin: 8px 0 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-errorText { font-size: 12px; line-height: 18px; color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); overflow-wrap: anywhere; }
.tmx-cardNote { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; margin-top: 12px; }
.tmx-banner { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; margin: 0 0 20px; padding: 10px 12px; border: 0.5px solid color-mix(in srgb, var(--dsw-alias-state-error-primary) 45%, var(--dsw-alias-border-l3)); border-radius: var(--dsw-radius-md); background: var(--dsw-alias-settings-card-fill); }
.tmx-next { display: flex; flex-direction: column; gap: 6px; margin: 4px 0 24px; padding: 14px 16px; border: 0.5px solid color-mix(in srgb, var(--dsw-alias-state-warn-primary) 45%, var(--dsw-alias-border-l3)); border-radius: var(--dsw-radius-xl); background: var(--dsw-alias-settings-card-fill); }
.tmx-nextHead { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 12px; line-height: 18px; font-weight: 500; letter-spacing: 0.03em; color: var(--dsw-alias-label-secondary); }
.tmx-nextText { margin: 0; font-size: 13px; line-height: 20px; }
.tmx-nextCode { margin: 0; font-family: var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace); font-size: 12px; line-height: 18px; color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); overflow-wrap: anywhere; }
.tmx-nextActions { margin-top: 2px; }
.tmx-panel { display: flex; flex-direction: column; gap: 8px; margin: 0 0 24px; padding: 14px 16px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); border-radius: var(--dsw-radius-xl); background: var(--dsw-alias-settings-card-fill); }
.tmx-panelTitle { display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px; margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-panelNote { margin: 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-panelActions { display: flex; flex-wrap: wrap; gap: 12px; }
.tmx-list { display: flex; flex-direction: column; gap: 4px; margin: 0; padding: 0; list-style: none; }
.tmx-listItem { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; font-size: 12px; line-height: 18px; }
.tmx-listMain { min-width: 0; overflow-wrap: anywhere; }
.tmx-connectLines { display: flex; flex-direction: column; gap: 6px; margin: 0; }
.tmx-connectGroup dt { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-connectGroup dd { margin: 0 0 4px; }
.tmx-inlineList { margin: 0; padding: 0 0 0 14px; font-size: 12px; line-height: 18px; }
.tmx-route { display: flex; flex-direction: column; gap: 4px; }
.tmx-routes { display: flex; flex-direction: column; gap: 12px; }
.tmx-routeHead { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0 0 6px; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-model { display: flex; flex-direction: column; gap: 4px; margin-top: 14px; }
.tmx-models { display: flex; flex-direction: column; }
.tmx-modelHead { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-routing { display: flex; flex-direction: column; gap: 4px; margin-top: 8px; }
.tmx-routingLine { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin: 0; font-size: 12px; line-height: 18px; }
.tmx-candidates { display: flex; flex-direction: column; gap: 2px; margin: 2px 0 0; padding: 0; list-style: none; }
.tmx-candidate { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-size: 12px; line-height: 18px; }
.tmx-providers { display: flex; flex-direction: column; gap: 12px; }
.tmx-provider { display: flex; flex-direction: column; gap: 8px; }
.tmx-providerHead { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-providerName { display: inline-flex; align-items: center; gap: 5px; min-width: 0; max-width: 100%; font-size: 14px; line-height: 22px; font-weight: 500; color: var(--dsw-alias-label-primary); text-decoration: none; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
a.tmx-providerName:hover { text-decoration: underline; text-underline-offset: 2px; }
.tmx-external { flex: none; margin-top: 1px; color: var(--dsw-alias-label-secondary); }
a.tmx-providerName:hover .tmx-external { color: var(--dsw-alias-label-primary); }
.tmx-providerMeta { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 12px; margin: 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
/* One repair action per evidence fact, beside the fact it repairs. */
.tmx-providerActions { display: flex; flex-wrap: wrap; gap: 8px; }
.tmx-evidence { margin: 0; font-size: 11px; line-height: 16px; color: var(--dsw-alias-label-secondary); overflow-wrap: anywhere; }
/* One grid per provider card, so every row and every card shares one 100% end. */
.tmx-windows { display: grid; grid-template-columns: 104px minmax(120px, 1fr) 118px 104px; align-items: center; gap: 8px 12px; margin: 0; padding: 0; list-style: none; }
.tmx-window { display: grid; grid-template-columns: subgrid; grid-column: 1 / -1; align-items: center; }
.tmx-windowName { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tmx-bar { position: relative; box-sizing: border-box; height: 8px; border-radius: 99px; background: var(--dsw-alias-border-l2); overflow: hidden; }
.tmx-barFill { display: block; height: 100%; border-radius: 99px; background: var(--dsw-alias-state-business-primary); transition: width var(--ds-transition-duration, 200ms) var(--ds-ease-in-out, ease); }
.tmx-barFill[data-tone='warn'] { background: color-mix(in srgb, var(--dsw-alias-state-warn-primary) 65%, var(--dsw-alias-label-primary)); }
.tmx-barFill[data-tone='error'] { background: color-mix(in srgb, var(--dsw-alias-state-error-primary) 82%, var(--dsw-alias-label-primary)); }
/* A measured zero is a fact, not an empty bar. */
.tmx-barZero { position: absolute; inset-block: 0; inset-inline-start: 0; width: 3px; border-radius: 99px; background: var(--dsw-alias-state-business-primary); }
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
@media (max-width: 760px) {
  /* The bar keeps a fixed cap so every card shares one 100% end. */
  .tmx-windows { grid-template-columns: 84px minmax(72px, 1fr) 96px 96px; }
}
@media (max-width: 560px) {
  /* The bar spans the row, so every card keeps one shared 100% end; the value
     and the reset move beneath it rather than competing for its width. */
  .tmx-windows { grid-template-columns: minmax(0, 1fr) auto; }
  .tmx-windowName { grid-column: 1; grid-row: 1; }
  .tmx-windowValue { grid-column: 2; grid-row: 1; }
  .tmx-bar { grid-column: 1 / -1; grid-row: 2; }
  .tmx-windowReset { grid-column: 1 / -1; grid-row: 3; text-align: start; }
  .tmx-windowReset[data-empty='true'] { display: none; }
  .tmx-windowWide { grid-column: 1 / -1; grid-row: 1; text-align: end; }
}
/* Phase 3 seats: navigation marker, Settings > Models card, model-menu pane, tool card. */
.tmx-navMark { display: inline-flex; align-items: center; gap: 4px; }
.tmx-navMark[data-tone='warn'] .tmx-dot { background: var(--dsw-alias-state-warn-primary); }
.tmx-navMark[data-tone='error'] .tmx-dot { background: var(--dsw-alias-state-error-primary); }
.tmx-navMark[data-wide='false'] { gap: 0; }
.tmx-seatCard { display: flex; flex-direction: column; gap: 10px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-xl); padding: 12px 14px; }
.tmx-seatHead { display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px; margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-seatFacts { margin: 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); overflow-wrap: anywhere; }
.tmx-seatActions { display: flex; flex-wrap: wrap; gap: 8px; }
.tmx-modelOption { box-sizing: border-box; display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 8px; border: 0; border-radius: var(--dsw-radius-md); background: transparent; color: var(--dsw-alias-label-primary); font: inherit; font-size: 13px; line-height: 20px; text-align: start; cursor: pointer; }
@media (hover: hover) { .tmx-modelOption:hover:not(:disabled) { background: var(--dsw-alias-interactive-bg-hover); } }
.tmx-modelOption:disabled { opacity: 0.5; cursor: default; }
.tmx-modelOptionLabel { flex: 1; min-width: 0; }
.tmx-pane { display: flex; flex-direction: column; gap: 10px; min-width: 280px; max-width: 420px; }
.tmx-paneTitle { margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-paneActions { display: flex; flex-wrap: wrap; gap: 8px; }
.tmx-toolCard { display: flex; flex-direction: column; gap: 6px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-md); padding: 10px 12px; }
.tmx-toolHead { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0; font-size: 13px; line-height: 20px; font-weight: 500; }
.tmx-historyCard { display: flex; flex-direction: column; gap: 10px; margin-top: 14px; padding: 12px 14px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); border-radius: var(--dsw-radius-xl); background: var(--dsw-alias-settings-card-fill); }
.tmx-historyHead { margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-historyAvailability { margin: 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-historyCoverageCard { min-width: 0; }
.tmx-historyCoverage { display: flex; flex-wrap: wrap; gap: 4px 16px; margin: 0; padding: 0; list-style: none; }
.tmx-historyCoverageItem { display: inline-flex; flex-wrap: wrap; align-items: center; gap: 6px; min-width: 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-historyCoverageItem[data-gap='true'] { color: var(--dsw-alias-label-primary); }
.tmx-historyTableWrap { max-width: 100%; overflow-x: auto; }
.tmx-historyTable { width: 100%; border-collapse: collapse; font-size: 12px; line-height: 18px; }
.tmx-historyTable th, .tmx-historyTable td { padding: 4px 8px; text-align: start; vertical-align: top; border-bottom: 0.5px solid var(--dsw-alias-border-l2); }
.tmx-historyTable th { font-weight: 500; color: var(--dsw-alias-label-secondary); white-space: nowrap; }
`

    // ---------------------------------------------------------------- formatting

    /** Dates read in the interface's language, not the browser's. */
    let dateLocale
    /** The app publishes the resolved language on <html lang>; follow it. */
    function syncDateLocale() {
      const lang = typeof document === 'undefined' ? undefined : document.documentElement?.lang
      if (typeof lang === 'string' && lang !== '') dateLocale = lang
    }

    /** Wall-clock time in the reader's language; '' when the value is not a time. */
    function clockOf(iso, locale) {
      if (typeof iso !== 'string' || iso === '') return ''
      const date = new Date(iso)
      if (Number.isNaN(date.getTime())) return ''
      const sameDay = date.toDateString() === new Date().toDateString()
      try {
        return new Intl.DateTimeFormat(locale ?? dateLocale, sameDay
          ? { hour: '2-digit', minute: '2-digit' }
          : { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(date)
      } catch {
        return sameDay ? date.toISOString().slice(11, 16) : date.toISOString().slice(5, 16).replace('T', ' ')
      }
    }

    /** A date in the reader's language; `now` is injectable so tests are stable. */
    function formatDate(date, locale, now) {
      if (!(date instanceof Date) || Number.isNaN(date.getTime())) return ''
      const reference = typeof now === 'number' ? now : Date.now()
      const near = Math.abs(date.getTime() - reference) < 36 * 60 * 60 * 1000
      try {
        return new Intl.DateTimeFormat(locale ?? dateLocale, near
          ? { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }
          : { year: 'numeric', month: 'short', day: 'numeric' }).format(date)
      } catch {
        return date.toISOString().slice(0, 16).replace('T', ' ')
      }
    }

    /** Big counters as the reader's language writes them (7.6B / 75.6亿). */
    function compact(value) {
      if (!Number.isFinite(value)) return '?'
      try {
        return new Intl.NumberFormat(dateLocale, { notation: 'compact', maximumFractionDigits: 1 }).format(value)
      } catch {
        return String(value)
      }
    }

    /** Host text is bounded and single-line; it is evidence, not a transcript. */
    function boundedText(value, max = 240) {
      if (value === undefined || value === null) return ''
      const text = String(value).replace(/[\u0000-\u001f\u007f]+/g, ' ').replace(/\s+/g, ' ').trim()
      if (text.length <= max) return text
      return `${text.slice(0, max).trimEnd()}…`
    }

    /** A host code first, then its bounded message; one line, no raw transcript. */
    function hostCodeText(code, message) {
      const label = typeof code === 'string' && code !== '' ? boundedText(code, 48) : ''
      const text = boundedText(message, 240)
      if (label === '' && text === '') return ''
      if (label === '') return text
      if (text === '') return label
      return `${label}: ${text}`
    }

    function enumText(value, table, t) {
      if (value === undefined || value === null || value === '') return t('unknown')
      if ((typeof value === 'string' || typeof value === 'boolean') && table[value] !== undefined) return t(table[value])
      return boundedText(value, 48)
    }

    function numeric(value) {
      if (typeof value === 'number') return Number.isFinite(value) ? value : undefined
      if (typeof value === 'string' && value.trim() !== '') {
        const parsed = Number(value)
        return Number.isFinite(parsed) ? parsed : undefined
      }
      return undefined
    }

    function shortRevision(value) {
      if (typeof value !== 'string' || value === '') return undefined
      return value.length > 8 ? value.slice(0, 8) : value
    }

    /** A reset whose kind/encoding is explicitly unqualified is not a date. */
    function resetQualified(reset) {
      if (typeof reset === 'string') return reset !== ''
      if (reset === null || typeof reset !== 'object') return false
      for (const value of [reset.kind, reset.encoding]) {
        if (typeof value === 'string' && /unqualified/i.test(value)) return false
      }
      return true
    }

    function resetUnqualified(reset) {
      return reset !== null && typeof reset === 'object' && !resetQualified(reset)
    }

    function resetAt(reset) {
      if (typeof reset === 'string') {
        const date = new Date(reset)
        return Number.isNaN(date.getTime()) ? undefined : date
      }
      if (reset === null || typeof reset !== 'object' || !resetQualified(reset)) return undefined
      const at = reset.at ?? (typeof reset.raw === 'number' ? reset.raw : undefined)
      if (at === undefined) return undefined
      const date = new Date(at)
      return Number.isNaN(date.getTime()) ? undefined : date
    }

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
          const count = boundedText(label.duration, 24)
          const unit = label.unit === 'TIME_UNIT_MINUTE' ? t('unitMinute') : label.unit === 'TIME_UNIT_HOUR' ? t('unitHour') : typeof label.unit === 'string' ? boundedText(label.unit, 24) : ''
          return `${count} ${unit}`.trim()
        }
        if (typeof label.key === 'string' && LABEL_KEYS[label.key] !== undefined) {
          return t(LABEL_KEYS[label.key])
        }
        if (label.key === 'credit') return t('windowCredit')
        const parts = []
        if (typeof label.name === 'string' && label.name !== '') parts.push(boundedText(label.name, 48))
        if (typeof label.window === 'string' && LABEL_KEYS[label.window] !== undefined) parts.push(t(LABEL_KEYS[label.window]))
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
     * fraction a filling bar can show. When no share is computable there is no
     * meter at all: a bar is a claim about proportion.
     *
     * `meterNow` and `spoken` always describe the same quantity (used %), so a
     * screen reader and the visible bar cannot disagree.
     */
    function windowReading(row, index, t) {
      if (row === null || typeof row !== 'object') return undefined
      const name = windowName(row.label, index, t)
      const raw = boundedText(JSON.stringify(row.label ?? {}), 120)
      const reset = resetAt(row.reset)
      const resetIsUnqualified = resetUnqualified(row.reset)
      const percent = numeric(row.percent)
      if (percent !== undefined) {
        const consumed = row.basis === 'remaining_percent' ? 100 - percent : percent
        const rounded = Math.round(Math.max(0, Math.min(100, consumed)))
        return {
          name,
          consumed: rounded,
          text: `${rounded}%`,
          spoken: `${t('usedShare')} ${rounded}%`,
          meterNow: rounded,
          raw,
          reset,
          resetUnqualified: resetIsUnqualified,
          zero: rounded === 0,
        }
      }
      const counters = row.counters !== null && typeof row.counters === 'object' ? row.counters : undefined
      const total = numeric(counters?.total ?? counters?.limit)
      const left = numeric(counters?.remaining)
      if (total !== undefined && left !== undefined && total > 0) {
        const consumed = Math.round(Math.max(0, Math.min(100, ((total - left) / total) * 100)))
        return {
          name,
          consumed,
          text: `${t('left')} ${compact(left)} / ${compact(total)}`,
          spoken: `${t('usedShare')} ${consumed}%`,
          meterNow: consumed,
          raw,
          reset,
          resetUnqualified: resetIsUnqualified,
          zero: consumed === 0,
        }
      }
      // A row that exists but yields no share still names itself: a text line
      // with no meter, never a bar standing in for a measurement we cannot make.
      return { name, consumed: undefined, text: t('quotaUnreadable'), unreadable: true, raw, reset, resetUnqualified: resetIsUnqualified }
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

    /** The oldest observation the page is actually drawing, never the newest. */
    function observedAtOf(record) {
      if (record === null || typeof record !== 'object') return undefined
      for (const value of [record.observedAt, record.billing?.observedAt]) {
        if (typeof value !== 'string' || value === '') continue
        const at = new Date(value).getTime()
        if (!Number.isNaN(at)) return at
      }
      return undefined
    }

    function oldestReading(records) {
      let oldest
      for (const record of Array.isArray(records) ? records : []) {
        const at = observedAtOf(record)
        if (at === undefined) continue
        if (oldest === undefined || at < oldest) oldest = at
      }
      return oldest === undefined ? undefined : new Date(oldest).toISOString()
    }

    // ------------------------------------------------------------- state reads

    function routeReadiness(route) {
      if (route?.credentialConfigured === true) return 'usable'
      if (route?.credentialConfigured === false) return 'registered'
      return 'unknown'
    }

    /** Is DSH actually serving this route right now? Unknown is not "no". */
    function routeActiveState(route) {
      if (route?.active === true) return 'active'
      if (route?.active === false) return 'inactive'
      return 'unknown'
    }

    /**
     * A route is usable only when DSH serves it and its credential is
     * configured. A registered-but-inactive route is configured, not connected.
     */
    function usableRoutes(state) {
      const routes = Array.isArray(state?.routes) ? state.routes : []
      return routes.filter(route => route !== null && typeof route === 'object'
        && route.active === true && route.credentialConfigured === true)
    }

    /** Route-level blockers are host evidence: code first, bounded text. */
    function routeBlockerTexts(route) {
      const blockers = Array.isArray(route?.blockers) ? route.blockers : []
      return blockers
        .slice(0, 10)
        .map(blocker => blocker !== null && typeof blocker === 'object'
          ? hostCodeText(blocker.code, blocker.message)
          : hostCodeText(undefined, blocker))
        .filter(text => text !== '')
    }

    /** tmax evidence about a model, never a DSH availability claim. */
    function modelCatalogOf(state) {
      const models = []
      for (const route of Array.isArray(state?.routes) ? state.routes : []) {
        if (route === null || typeof route !== 'object') continue
        for (const model of Array.isArray(route.models) ? route.models : []) {
          if (model !== null && typeof model === 'object') models.push({ provider: route.provider, model })
        }
      }
      return models
    }

    /**
     * The one word in the header. It answers reachability only: a route that
     * exists but is not live in DSH is not connected, and a route whose
     * credential is missing is "registered, not usable".
     */
    function verdict(load, t) {
      if (!load || load.phase === 'loading') return { tone: 'idle', text: t('checking') }
      if (load.schemaUnsupported === true) return { tone: 'error', text: t('stateSchemaUnsupported') }
      const state = load.value
      if (state === undefined || state === null || state.ok !== true) {
        return { tone: 'error', text: t('stateUnreadable') }
      }
      const result = stateVerdict(state, t)
      // A failed read that still shows the last good state says so instead of
      // claiming the stale table is current.
      if (load.phase === 'failed') {
        return { tone: result.tone, text: `${t('stateLastKnown')}${t('colon')} ${result.text}` }
      }
      return result
    }

    function stateVerdict(state, t) {
      // A failed projection read cannot support any claim about routes.
      if (projectionFailed(state)) {
        return { tone: 'error', text: t('stateProjection') }
      }
      const routes = Array.isArray(state.routes) ? state.routes : []
      const ownership = state.ownership ?? {}
      if (routes.length === 0) {
        // manageRoutes=false with no home integration is a configured dead end,
        // not a disconnected accident.
        if (ownershipMode(ownership) === false && ownership.homeIntegration?.present !== true) {
          return { tone: 'warn', text: t('stateRouteDisabled') }
        }
        return { tone: 'idle', text: t('stateDisconnected') }
      }
      const usable = usableRoutes(state)
      if (usable.length > 0) {
        const ready = state.gateway?.ready
        if (ready === true) return { tone: 'ok', text: t('stateReady') }
        if (ready === false) return { tone: 'error', text: t('stateGateway') }
        return { tone: 'idle', text: t('stateUnknown') }
      }
      const active = routes.filter(route => route?.active === true)
      if (active.length === 0) {
        if (ownership.conflict !== null && ownership.conflict !== undefined) {
          return { tone: 'error', text: t('stateConflict') }
        }
        if (pendingReload(ownership)
          || (ownership.owner === 'home' && ownership.homeIntegration?.present === true)) {
          return { tone: 'idle', text: t('stateReload') }
        }
        if (ownership.owner === 'page') return { tone: 'warn', text: t('stateInactive') }
        return { tone: 'idle', text: t('stateUnknown') }
      }
      // A live route with a known-missing credential is registered, not usable;
      // readiness that was never reported stays unknown.
      const missing = active.filter(route => route?.credentialConfigured === false)
      if (missing.length > 0) return { tone: 'warn', text: t('stateNotUsable') }
      return { tone: 'idle', text: t('stateUnknown') }
    }

    /**
     * The one explicit next step, only when one exists and the page can offer
     * it. A broken config blocks every write; a failed projection is a retry;
     * a home integration that is installed but not loaded is a reload.
     */
    function nextAction(load) {
      const state = load?.value
      if (load?.phase === 'failed' && (state === undefined || state === null || state.ok !== true)) {
        return { kind: 'retry' }
      }
      if (state === undefined || state === null || state.ok !== true) return undefined
      const routes = Array.isArray(state.routes) ? state.routes : []
      const ownership = state.ownership ?? {}
      const config = state.config ?? {}
      const owner = ownership.owner
      const conflict = ownership.conflict
      const homePresent = ownership.homeIntegration?.present === true
      const usable = usableRoutes(state)
      const active = routes.filter(route => route !== null && typeof route === 'object' && route.active === true)
      const missing = routes.filter(route => route !== null && typeof route === 'object' && route.credentialConfigured === false)
      const pending = pendingReload(ownership)
      const homePending = pending || (owner === 'home' && homePresent && active.length === 0)
      const readFailures = sectionErrors(state)
      const sectionRetry = readFailures.length > 0 ? { kind: 'retry', reason: 'section' } : undefined

      // A config that is invalid or unreadable blocks every write, so it is
      // named before Apply can be offered.
      if (config.state === 'invalid') return { kind: 'config', reason: 'invalid' }
      if (config.state === 'unavailable') return { kind: 'config', reason: 'unavailable' }
      // manageRoutes=false with no home integration means no route manager.
      if (ownershipMode(ownership) === false && !homePresent && usable.length === 0) return { kind: 'mode' }
      // A failed projection read leaves routes unknown; only a retry is honest.
      if (projectionFailed(state)) return { kind: 'retry', reason: 'projection' }
      // Page routes keep working until DSH reloads; the reload is non-writing.
      if (pending) return { kind: 'reload' }

      if (usable.length === 0) {
        // The home plugin's routes are not live until DSH reloads.
        if (homePending) return { kind: 'reload' }
        if (conflict !== null && conflict !== undefined) {
          return { kind: 'conflict', code: conflictCode(conflict), message: conflictMessage(conflict) }
        }
        if (routes.length === 0) return owner === 'page' ? { kind: 'connect' } : sectionRetry
        // Routes exist but DSH serves none: Apply repairs a credential only
        // when one is known to be missing; an unknown readiness stays unknown.
        if ((owner === 'page' || owner === 'home') && missing.length > 0) return { kind: 'connect' }
        return sectionRetry
      }

      const gateway = state.gateway ?? {}
      if (gateway.ready === false) return { kind: 'gateway', code: gateway.reasonCode }
      const blockers = Array.isArray(state.target?.blockers) ? state.target.blockers : []
      if (blockers.length > 0) {
        const blocker = blockers[0]
        return {
          kind: 'blocker',
          code: blocker !== null && typeof blocker === 'object' ? blocker.code : undefined,
          message: blocker !== null && typeof blocker === 'object' ? blocker.message : blocker,
        }
      }
      // Nothing more urgent is wrong, but some section read failed: retry the
      // read model (GET), never a provider observation.
      return sectionRetry
    }

    function conflictCode(conflict) {
      if (conflict === null || typeof conflict !== 'object') return undefined
      return typeof conflict.code === 'string' ? conflict.code : undefined
    }

    function conflictMessage(conflict) {
      if (typeof conflict === 'string') return conflict
      if (conflict === null || typeof conflict !== 'object') return undefined
      if (typeof conflict.message === 'string') return conflict.message
      const names = conflict.names ?? conflict.routes
      return Array.isArray(names) ? names.join(', ') : undefined
    }

    /** Do tmax's target default and DSH's effective default name one model? */
    function defaultsMatch(tmaxDefault, effectiveDefault) {
      if (tmaxDefault === null || typeof tmaxDefault !== 'object') return undefined
      if (effectiveDefault === null || typeof effectiveDefault !== 'object') return undefined
      if (typeof tmaxDefault.provider !== 'string' || typeof effectiveDefault.provider !== 'string') return undefined
      const left = typeof tmaxDefault.id === 'string' ? tmaxDefault.id : typeof tmaxDefault.model === 'string' ? tmaxDefault.model : undefined
      const right = typeof effectiveDefault.model === 'string' ? effectiveDefault.model : typeof effectiveDefault.id === 'string' ? effectiveDefault.id : undefined
      if (left === undefined || right === undefined) return undefined
      return tmaxDefault.provider === effectiveDefault.provider && left === right
    }

    /** External links are limited to http/https; anything else is not a link. */
    function safeDashboard(value) {
      if (typeof value !== 'string' || value === '') return undefined
      try {
        const url = new URL(value)
        return url.protocol === 'http:' || url.protocol === 'https:' ? url.toString() : undefined
      } catch {
        return undefined
      }
    }

    /** Capabilities are rendered one field at a time: absent is not zero. */
    function normalizeSectionName(value) {
      if (typeof value !== 'string') return ''
      return value
        .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
        .toLowerCase()
        .replace(/[\s-]+/g, '_')
    }

    function sectionErrorText(error) {
      const code = typeof error?.code === 'string' ? boundedText(error.code, 48) : ''
      const message = boundedText(error?.message, 240)
      if (code !== '' && message !== '') return `${code} — ${message}`
      return code !== '' ? code : message
    }

    /** Every host section read failure, normalized and bounded. */
    function sectionErrors(state) {
      const raw = Array.isArray(state?.errors) ? state.errors : []
      return raw.slice(0, 50).map(error => {
        if (error === null || typeof error !== 'object') {
          const message = boundedText(error, 240)
          return { section: 'unknown', code: '', message, text: message }
        }
        const section = normalizeSectionName(error.section)
        const code = typeof error.code === 'string' ? boundedText(error.code, 48) : ''
        const message = boundedText(error.message, 240)
        return { section, code, message, text: sectionErrorText({ code, message }) }
      }).filter(error => error.text !== '')
    }

    function errorsForSection(state, kind) {
      const aliases = (SECTION_ALIASES[kind] ?? [kind]).map(normalizeSectionName)
      return sectionErrors(state).filter(error => aliases.includes(error.section))
    }

    /**
     * A projection read failure leaves routes unknown. The host reports it as a
     * 'projection' section in the future shape, or under 'target' today; a null
     * target is the marker that the projection itself did not answer.
     */
    function projectionFailed(state) {
      if (errorsForSection(state, 'projection').length > 0) return true
      const routes = Array.isArray(state?.routes) ? state.routes : []
      return routes.length === 0
        && (state?.target === null || state?.target === undefined)
        && errorsForSection(state, 'target').length > 0
    }

    function projectionErrors(state) {
      const explicit = errorsForSection(state, 'projection')
      if (explicit.length > 0) return explicit
      return projectionFailed(state) ? errorsForSection(state, 'target') : []
    }

    /** Ownership mode from the host: 'auto', true, false, or unknown. */
    function ownershipMode(ownership) {
      const mode = ownership?.mode
      if (mode === 'true') return true
      if (mode === 'false') return false
      return mode === 'auto' || mode === true || mode === false ? mode : 'unknown'
    }

    /** Both host spellings of the pending reload transition mean one thing. */
    function pendingReload(ownership) {
      return typeof ownership?.reason === 'string' && PENDING_RELOAD_REASONS.includes(ownership.reason)
    }

    /** The unknown-quota sentence for one provider, with its last attempt. */
    function limitMessage(provider, t) {
      const limit = limitState(provider)
      if (limit === 'rows' || limit === 'error') return undefined
      const billing = provider?.billing
      const reason = typeof billing?.reasonCode === 'string' ? billing.reasonCode : ''
      const lastAttempt = typeof billing?.lastAttempt === 'string' && billing.lastAttempt !== ''
        ? boundedText(billing.lastAttempt, 48)
        : undefined
      const base = limit === 'noRows'
        ? t('noQuotaRows')
        : reason === 'unsupported_source' ? t('unknownQuotaUnsupported') : t('unknownQuota')
      return lastAttempt === undefined ? base : `${base} · ${t('lastAttempt')} ${lastAttempt}`
    }

    function billingNoteTexts(provider, t) {
      const notes = Array.isArray(provider?.billing?.notes) ? provider.billing.notes : []
      return notes.slice(0, 8).map(note => NOTE_KEYS[note] === undefined ? boundedText(note, 40) : t(NOTE_KEYS[note]))
    }

    function capabilityFacts(capabilities, t) {
      if (capabilities === null || typeof capabilities !== 'object') {
        return [{ label: t('capabilities'), value: t('notReported') }]
      }
      const context = numeric(capabilities.contextWindow)
      const input = capabilities.input
      const efforts = capabilities.reasoningEfforts
      return [
        { label: t('contextWindow'), value: context === undefined ? t('unknown') : compact(context) },
        {
          label: t('inputModalities'),
          value: Array.isArray(input)
            ? (input.length === 0 ? t('none') : input.map(item => boundedText(item, 24)).join(', '))
            : t('unknown'),
        },
        {
          label: t('reasoningEfforts'),
          value: efforts !== null && typeof efforts === 'object'
            ? (Object.keys(efforts).length === 0 ? t('none') : Object.keys(efforts).map(item => boundedText(item, 24)).join(', '))
            : t('unknown'),
        },
      ]
    }

    function limitState(provider) {
      if (provider?.error !== null && provider?.error !== undefined) return 'error'
      const billing = provider?.billing
      if (billing === null || typeof billing !== 'object') return 'unknown'
      const rows = Array.isArray(billing.rows) ? billing.rows : []
      if (rows.length > 0) return 'rows'
      if (billing.state === 'observed') return 'noRows'
      return 'unknown'
    }

    function historyFact(diagnostics, t) {
      const history = diagnostics?.history
      if (history === null || typeof history !== 'object') return { value: t('unknown') }
      const state = enumText(history.state, HISTORY_STATES, t)
      const reason = history.reasonCode === null || history.reasonCode === undefined
        ? undefined
        : enumText(history.reasonCode, REASONS, t)
      return { value: reason === undefined ? state : `${state} (${reason})` }
    }

    function lastErrorFact(diagnostics, t) {
      const lastError = diagnostics?.lastError
      if (lastError === null) return { value: t('noLastError') }
      if (lastError === undefined) return { value: t('unknown') }
      if (typeof lastError === 'string') return { value: hostCodeText(undefined, lastError) || t('unknown') }
      if (typeof lastError === 'object') {
        return { value: hostCodeText(lastError.code ?? lastError.reasonCode, lastError.message ?? lastError.error) || t('unknown') }
      }
      return { value: boundedText(lastError, 200) || t('unknown') }
    }

    // ------------------------------------------------------- mutation payloads

    function entryText(item) {
      if (typeof item === 'string') return boundedText(item, 200)
      if (typeof item === 'number' || typeof item === 'boolean') return String(item)
      if (item === null || typeof item !== 'object') return ''
      const candidate = item.message ?? item.text ?? item.path ?? item.name ?? item.ref ?? item.id ?? item.code ?? item.qualifiedModel
      if (typeof candidate === 'string' && candidate !== '') return boundedText(candidate, 200)
      try {
        return boundedText(JSON.stringify(item), 200)
      } catch {
        return ''
      }
    }

    /**
     * A connect envelope is host-owned and may grow; the page renders the
     * groups it knows about and ignores the rest rather than inventing detail.
     */
    /** A reference row is one route's credential decision; the value never crosses. */
    function referenceLine(reference, t) {
      if (reference === null || typeof reference !== 'object') return entryText(reference)
      const parts = []
      for (const field of ['route', 'protocol', 'credential_ref']) {
        if (typeof reference[field] === 'string' && reference[field] !== '') parts.push(boundedText(reference[field], 120))
      }
      if (typeof reference.action === 'string' && reference.action !== '') {
        parts.push(enumText(reference.action, REFERENCE_ACTIONS, t))
      }
      return parts.length === 0 ? entryText(reference) : parts.join(' · ')
    }

    function effectLines(effects, t) {
      if (effects === null || typeof effects !== 'object') return []
      const lines = []
      for (const [field, key] of CONNECT_EFFECTS) {
        if (!(field in effects)) continue
        lines.push(`${t(key)}${t('colon')} ${word(effects[field], t)}`)
      }
      for (const [field, value] of Object.entries(effects)) {
        if (CONNECT_EFFECTS.some(([known]) => known === field)) continue
        lines.push(`${boundedText(field, 48)}${t('colon')} ${entryText(value) || t('unknown')}`)
      }
      return lines
    }

    /**
     * A connect envelope is host-owned: tmax's secret-free `agent apply` data on
     * success, or a blocker envelope on refusal. Render the groups the page can
     * state truthfully and ignore anything else rather than inventing detail.
     */
    function connectLines(connect, t) {
      const lines = []
      if (connect === null || typeof connect !== 'object') return lines

      // A preview must disclose what it would touch, not only its references.
      if (connect.target !== null && typeof connect.target === 'object') {
        const items = []
        if (typeof connect.target.id === 'string' && connect.target.id !== '') items.push(`${t('target')}${t('colon')} ${boundedText(connect.target.id, 80)}`)
        if (typeof connect.target.path === 'string' && connect.target.path !== '') items.push(`${t('targetPath')}${t('colon')} ${boundedText(connect.target.path, 160)}`)
        if (items.length > 0) lines.push({ label: t('target'), items })
      } else if (typeof connect.target === 'string' && connect.target !== '') {
        // `agent apply` reports the target as an id string.
        lines.push({ label: t('target'), items: [boundedText(connect.target, 80)] })
      }
      // `tmax agent apply` data: the applied state, the files it planned or
      // changed, its recovery record, and fact gaps. Paths and states only; no
      // native file content ever crosses into the page.
      if (typeof connect.state === 'string' && connect.state !== '') {
        lines.push({ label: t('connectState'), items: [boundedText(connect.state, 64)] })
      }
      const planned = Array.isArray(connect.files) ? connect.files : []
      if (planned.length > 0) {
        const items = planned.slice(0, 12).map(file => {
          if (file === null || typeof file !== 'object') return entryText(file)
          const path = boundedText(file.path, 160)
          const format = typeof file.format === 'string' && file.format !== '' ? boundedText(file.format, 32) : ''
          if (path === '') return format
          return format === '' ? path : `${path}${t('colon')} ${format}`
        }).filter(text => text !== '')
        if (items.length > 0) lines.push({ label: t('connectFiles'), items })
      }
      for (const [field, key] of [['changed_files', 'connectChangedFiles'], ['unchanged_files', 'connectUnchangedFiles']]) {
        const values = Array.isArray(connect[field]) ? connect[field] : []
        const items = values.slice(0, 12).map(value => boundedText(value, 160)).filter(text => text !== '')
        if (items.length > 0) lines.push({ label: t(key), items })
      }
      if (typeof connect.recovery === 'string' && connect.recovery !== '') {
        lines.push({ label: t('connectRecovery'), items: [boundedText(connect.recovery, 80)] })
      } else if (connect.recovery !== null && typeof connect.recovery === 'object') {
        const items = []
        if (typeof connect.recovery.planned === 'string' && connect.recovery.planned !== '') {
          items.push(`${t('recoveryPlanned')}${t('colon')} ${boundedText(connect.recovery.planned, 160)}`)
        }
        if (typeof connect.recovery.retained === 'string' && connect.recovery.retained !== '') {
          items.push(`${t('recoveryRetained')}${t('colon')} ${boundedText(connect.recovery.retained, 80)}`)
        }
        if (items.length > 0) lines.push({ label: t('connectRecovery'), items })
      }
      const factGaps = Array.isArray(connect.fact_gaps) ? connect.fact_gaps : []
      if (factGaps.length > 0) {
        const items = factGaps.slice(0, 12).map(gap => {
          if (gap === null || typeof gap !== 'object') return entryText(gap)
          const subject = [boundedText(gap.entry, 80), boundedText(gap.model, 80)].filter(text => text !== '').join('/')
          const missing = Array.isArray(gap.missing)
            ? gap.missing.map(item => boundedText(item, 48)).filter(text => text !== '').join(', ')
            : boundedText(gap.missing, 120)
          if (subject === '') return missing === '' ? entryText(gap) : missing
          return missing === '' ? subject : `${subject}${t('colon')} ${missing}`
        }).filter(text => text !== '')
        if (items.length > 0) lines.push({ label: t('connectFactGaps'), items })
      }
      if (connect.gateway !== null && typeof connect.gateway === 'object') {
        const gateway = connect.gateway
        const items = []
        if (typeof gateway.listen === 'string' && gateway.listen !== '') items.push(`${t('gatewayListen')}${t('colon')} ${boundedText(gateway.listen, 80)}`)
        items.push(`${t('gatewayReady')}${t('colon')} ${word(gateway.ready, t)}`)
        if (typeof gateway.reason_code === 'string' && gateway.reason_code !== '') {
          items.push(`${t('gatewayReason')}${t('colon')} ${enumText(gateway.reason_code, REASONS, t)}`)
        }
        lines.push({ label: t('gatewayTitle'), items })
      }
      if (connect.credentials !== null && typeof connect.credentials === 'object') {
        const credentials = connect.credentials
        const items = []
        if (typeof credentials.path === 'string' && credentials.path !== '') items.push(`${t('credentialPath')}${t('colon')} ${boundedText(credentials.path, 160)}`)
        if (typeof credentials.state === 'string' && credentials.state !== '') items.push(`${t('credentialState')}${t('colon')} ${boundedText(credentials.state, 40)}`)
        if (typeof credentials.permissions === 'string' || typeof credentials.permissions === 'number') {
          items.push(`${t('credentialPermissions')}${t('colon')} ${boundedText(credentials.permissions, 40)}`)
        }
        if (items.length > 0) lines.push({ label: t('credentialsTitle'), items })
      }

      const references = Array.isArray(connect.references) ? connect.references : []
      if (references.length > 0) {
        lines.push({ label: t('connectRefs'), items: references.slice(0, 20).map(reference => referenceLine(reference, t)) })
      } else if (Array.isArray(connect.refs)) {
        lines.push({ label: t('connectRefs'), items: connect.refs.slice(0, 20).map(entryText).filter(text => text !== '') })
      }

      if (connect.changed !== undefined && connect.changed !== null && typeof connect.changed !== 'object') {
        lines.push({ label: t('connectChanged'), items: [word(connect.changed, t)] })
      } else if (Array.isArray(connect.changed)) {
        const items = connect.changed.slice(0, 12).map(entryText).filter(text => text !== '')
        if (items.length > 0) lines.push({ label: t('connectChanged'), items })
      }

      if (connect.effects !== null && typeof connect.effects === 'object' && !Array.isArray(connect.effects)) {
        const items = effectLines(connect.effects, t)
        if (items.length > 0) lines.push({ label: t('connectEffects'), items })
      } else if (Array.isArray(connect.effects)) {
        const items = connect.effects.slice(0, 12).map(entryText).filter(text => text !== '')
        if (items.length > 0) lines.push({ label: t('connectEffects'), items })
      }

      const blockers = Array.isArray(connect.blockers) ? connect.blockers : []
      if (blockers.length > 0) {
        lines.push({ label: t('connectBlockers'), items: blockers.slice(0, 12).map(entryText).filter(text => text !== '') })
      }
      const nextActions = Array.isArray(connect.next_actions) ? connect.next_actions : Array.isArray(connect.nextActions) ? connect.nextActions : []
      if (nextActions.length > 0) {
        lines.push({ label: t('connectNextActions'), items: nextActions.slice(0, 12).map(entryText).filter(text => text !== '') })
      }
      if (typeof connect.message === 'string' && connect.message !== '') {
        lines.push({ label: t('connectDetail'), items: [boundedText(connect.message, 200)] })
      }
      return lines
    }

    // ------------------------------------------------------------ explicit actions

    /** A failure record is active when it names an override or counts a failure. */
    function failureIsActive(failure) {
      if (failure === null || typeof failure !== 'object') return false
      if (failure.state === 'active') return true
      return (numeric(failure.consecutiveFailures) ?? 0) > 0
    }

    /**
     * The actions one provider's own evidence calls for. A resume is offered
     * only for a reported suspension and a reset only for an active failure; an
     * observation is always an explicit choice. No evidence, no button.
     */
    function providerActions(provider) {
      const id = typeof provider?.id === 'string' ? provider.id : ''
      if (id === '') return []
      const actions = []
      if (provider?.suspension?.state === 'suspended') actions.push({ action: 'resume_quota', provider: id })
      if (failureIsActive(provider?.failure)) actions.push({ action: 'reset_failure', provider: id })
      actions.push({ action: 'observe_provider', provider: id, scope: 'quota' })
      return actions
    }

    /**
     * The management page is a real tmax surface, but only a configured gateway
     * has one to open. One rule decides both the button and the availability.
     */
    function managementAction(gateway) {
      return gateway?.configured === true ? { action: 'open_management' } : undefined
    }

    /**
     * Gateway lifecycle actions only where they can change the reported state:
     * start when it is not ready, stop/restart only when a daemon is actually
     * identified or the gateway reports ready. An unknown readiness never
     * offers an operation that may have nothing to act on.
     */
    /** Whether the reported daemon discovery token names an identified daemon. */
    function daemonIdentified(gateway) {
      return gateway?.daemonDiscoveryState === 'identified' || gateway?.daemonDiscoveryState === 'compatible'
    }

    function gatewayActions(gateway) {
      if (gateway?.configured !== true) return []
      const ready = gateway.ready === true
      const daemon = daemonIdentified(gateway)
      const actions = []
      if (!ready) actions.push({ action: 'gateway_start' })
      if (ready || daemon) actions.push({ action: 'gateway_stop' }, { action: 'gateway_restart' })
      return actions
    }

    /**
     * The exact POST body for one action: one known action, one provider id
     * taken from the reported state, one enum scope, and the preview flag. A
     * descriptor the page cannot place produces no request at all.
     */
    function actionPayload(descriptor, dryRun) {
      if (descriptor === null || typeof descriptor !== 'object') return undefined
      const action = descriptor.action
      if (typeof action !== 'string' || !ACTION_KINDS.includes(action)) return undefined
      if (action === 'open_management' || GATEWAY_ACTIONS.includes(action)) return { action, dryRun: dryRun === true }
      const provider = descriptor.provider
      if (typeof provider !== 'string' || provider === '') return undefined
      if (action === 'observe_provider') {
        const scope = typeof descriptor.scope === 'string' && ACTION_SCOPES.includes(descriptor.scope) ? descriptor.scope : 'quota'
        return { action, provider, scope, dryRun: dryRun === true }
      }
      return { action, provider, dryRun: dryRun === true }
    }

    /**
     * A typed result is a success only when the host says so and its own status
     * is not a failure state. An unreadable, blocked, or refused result is never
     * success, even when the HTTP status is 200.
     */
    function actionSucceeded(value, responseOk) {
      if (responseOk !== true) return false
      if (value === null || typeof value !== 'object' || value.ok !== true) return false
      const status = typeof value.result?.status === 'string' ? value.result.status : ''
      return !ACTION_FAILURE_STATUSES.includes(status)
    }

    /** One bounded line per result-data field; the envelope is host-owned. */
    function actionDataLines(data, t) {
      const items = []
      if (data === null || typeof data !== 'object') return items
      if (Array.isArray(data)) {
        for (const item of data.slice(0, 12)) {
          const text = entryText(item)
          if (text !== '') items.push(text)
        }
        return items
      }
      for (const [field, value] of Object.entries(data).slice(0, 12)) {
        items.push(`${boundedText(field, 48)}${t('colon')} ${entryText(value) || t('unknown')}`)
      }
      return items
    }

    /**
     * `web status` / `web open` facts, which nest the web listener under the
     * daemon. A management preview must disclose that readiness and its
     * blockers before the page starts a listener, so these fields are read by
     * name; anything else stays for the generic renderer. A missing listener
     * record (`daemon.web` null or absent) is unknown, never "stopped": only a
     * state the host actually reported can name a lifecycle.
     */
    function managementDataLines(data, t) {
      const items = []
      if (data === null || typeof data !== 'object') return items
      const daemon = data.daemon !== null && typeof data.daemon === 'object' ? data.daemon : undefined
      if (daemon !== undefined) {
        if (typeof daemon.discovery_state === 'string' && daemon.discovery_state !== '') {
          items.push(`${t('gatewayDaemon')}${t('colon')} ${enumText(daemon.discovery_state, DAEMON_STATES, t)}`)
        }
        if (typeof daemon.reason_code === 'string' && daemon.reason_code !== '') {
          items.push(`${t('gatewayReason')}${t('colon')} ${boundedText(daemon.reason_code, 48)}`)
        }
      }
      const web = daemon?.web !== null && typeof daemon?.web === 'object' ? daemon.web : undefined
      if (web !== undefined) {
        // A state the host reported is named; an unrecognized value is shown
        // verbatim (bounded), and an absent one is unknown.
        items.push(`${t('webListener')}${t('colon')} ${enumText(web.state, WEB_STATES, t)}`)
        if (typeof web.listen === 'string' && web.listen !== '') items.push(`${t('gatewayListen')}${t('colon')} ${boundedText(web.listen, 80)}`)
        const url = safeDashboard(web.url)
        if (url !== undefined) items.push(`${t('webUrl')}${t('colon')} ${boundedText(url, 200)}`)
      } else if (daemon !== undefined) {
        // null/absent `daemon.web` is not a stopped listener: nothing was reported.
        items.push(`${t('webListener')}${t('colon')} ${t('notReported')}`)
      }
      const gateway = data.gateway !== null && typeof data.gateway === 'object' ? data.gateway : undefined
      if (gateway !== undefined) {
        items.push(`${t('gatewayReady')}${t('colon')} ${word(gateway.ready, t)}`)
        if (typeof gateway.listen === 'string' && gateway.listen !== '') {
          items.push(`${t('gatewayListen')}${t('colon')} ${boundedText(gateway.listen, 80)}`)
        }
      }
      return items
    }

    /**
     * A result envelope is host-owned: show the typed outcome and the groups the
     * page can state truthfully, all bounded, never as a raw tmax transcript.
     */
    function actionResultLines(result, t, action) {
      const lines = []
      if (result === null || typeof result !== 'object') return lines
      if (typeof result.code === 'string' && result.code !== '') {
        lines.push({ label: t('actionCode'), items: [boundedText(result.code, 48)] })
      }
      if (typeof result.status === 'string' && result.status !== '') {
        lines.push({ label: t('actionStatus'), items: [boundedText(result.status, 48)] })
      }
      if (typeof result.message === 'string' && result.message !== '') {
        lines.push({ label: t('actionMessage'), items: [boundedText(result.message, 240)] })
      }
      const data = action === 'open_management' || GATEWAY_ACTIONS.includes(action)
        ? managementDataLines(result.data, t)
        : []
      const dataLines = data.length > 0 ? data : actionDataLines(result.data, t)
      if (dataLines.length > 0) lines.push({ label: t('actionData'), items: dataLines })
      const blockers = Array.isArray(result.blockers) ? result.blockers : []
      if (blockers.length > 0) {
        const items = blockers.slice(0, 12)
          .map(item => item !== null && typeof item === 'object'
            ? hostCodeText(item.code, item.message) || entryText(item)
            : entryText(item))
          .filter(text => text !== '')
        if (items.length > 0) lines.push({ label: t('connectBlockers'), items })
      }
      const nextActions = Array.isArray(result.next_actions)
        ? result.next_actions
        : Array.isArray(result.nextActions) ? result.nextActions : []
      if (nextActions.length > 0) {
        const items = nextActions.slice(0, 12).map(entryText).filter(text => text !== '')
        if (items.length > 0) lines.push({ label: t('connectNextActions'), items })
      }
      if (result.effects !== null && typeof result.effects === 'object' && !Array.isArray(result.effects)) {
        const items = effectLines(result.effects, t)
        if (items.length > 0) lines.push({ label: t('connectEffects'), items })
      }
      return lines
    }

    function observeItems(value) {
      const raw = Array.isArray(value?.items) ? value.items : []
      const items = []
      for (let index = 0; index < raw.length && items.length < 50; index += 1) {
        const item = raw[index]
        const source = boundedText(item?.source, 32)
        const id = boundedText(item?.id, 80)
        const status = boundedText(item?.status, 24)
        const code = typeof item?.code === 'string' ? boundedText(item.code, 48) : ''
        const message = boundedText(item?.message, 200)
        items.push({
          key: `${source}:${id}:${index}`,
          source,
          id,
          status,
          tone: status === 'refreshed' ? 'ok' : status === 'failed' ? 'error' : status === 'blocked' ? 'warn' : 'idle',
          detail: hostCodeText(code, message),
        })
      }
      return items
    }

    // ------------------------------------------------------------- fetch plumbing

    async function readBody(response) {
      try {
        return await response.json()
      } catch {
        return undefined
      }
    }

    function stateError(value, status) {
      if (value !== null && typeof value === 'object') {
        const text = hostCodeText(value.code, value.message ?? value.error)
        if (text !== '') return text
        const lastError = value.diagnostics?.lastError
        if (lastError !== null && lastError !== undefined) {
          const lastText = typeof lastError === 'object'
            ? hostCodeText(lastError.code ?? lastError.reasonCode, lastError.message ?? lastError.error)
            : hostCodeText(undefined, lastError)
          if (lastText !== '') return lastText
        }
      }
      return `HTTP ${status}`
    }

    async function postJson(path, body) {
      const response = await fetch(new URL(path, document.baseURI), {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      })
      return { response, value: await readBody(response) }
    }

    function plainError(error) {
      return error instanceof Error ? error.message : String(error)
    }

    /**
     * The one consumer gate for a host read model, shared by the page and every
     * seat. Only an object carrying `schemaVersion === 1` and `ok === true` is
     * a state; an unsupported schema is a typed failure, and a refused model is
     * an error. Neither is ever adopted as evidence.
     * @returns {{ object: boolean, state?: object, schemaUnsupported?: boolean, message?: string }}
     */
    function stateGate(value, response) {
      if (value === null || typeof value !== 'object') return { object: false }
      if (value.schemaVersion !== 1) {
        return { object: true, schemaUnsupported: true, message: boundedText(value.schemaVersion, 24) }
      }
      if (value.ok !== true) return { object: true, message: stateError(value, response?.status) }
      return { object: true, state: value }
    }

    /**
     * Adopt a state returned by an explicit operation. An unsupported schema is
     * a visible failure and its payload is never used.
     * @returns {boolean} whether the value was a state object at all
     */
    function adoptState(setLoad, nextState, response, t) {
      const gate = stateGate(nextState, response)
      if (gate.object !== true) return false
      if (gate.state !== undefined) {
        rememberState(gate.state)
        setLoad({ phase: 'ready', value: gate.state })
        return true
      }
      if (gate.schemaUnsupported === true) {
        setLoad({ phase: 'failed', schemaUnsupported: true, message: `${t('schema')} ${gate.message}` })
        return true
      }
      setLoad(previous => ({ phase: 'failed', value: previous.value, message: gate.message }))
      return true
    }

    function describeError(message, t) {
      if (typeof message !== 'string' || message === '') return t('hostUnreachable')
      if (/failed to fetch|networkerror|load failed|network error|fetch failed/i.test(message)) return t('hostUnreachable')
      const status = /^HTTP (\d{3})$/.exec(message)
      if (status !== null) return `${t('hostStatus')} ${status[1]}`
      return boundedText(message, 200)
    }

    // ---------------------------------------------------------------- components

    /**
     * One GET read model. The page never adds a timer or a visibility listener:
     * a mount reads cached state once, and only an explicit action reads again.
     */
    function TmaxPage({ t }) {
      const [load, setLoad] = React.useState({ phase: 'loading' })
      const [observe, setObserve] = React.useState({ phase: 'idle', items: [] })
      const [connect, setConnect] = React.useState({ phase: 'idle', lines: [] })
      // One explicit repair action at a time, in one panel: preview, then an
      // explicit confirmation, then a typed result. Never chained.
      const [repair, setRepair] = React.useState({ phase: 'idle' })

      const loadState = React.useCallback(async () => {
        setLoad(previous => ({ phase: previous.value === undefined ? 'loading' : 'refreshing', value: previous.value }))
        try {
          const response = await fetch(new URL('/_tmax/state', document.baseURI), { credentials: 'same-origin' })
          const value = await readBody(response)
          if (!response.ok) throw new Error(stateError(value, response.status))
          if (value === null || typeof value !== 'object') throw new Error(`HTTP ${response.status}`)
          // The state schema is a hard consumer gate: an unknown version is a
          // failure, never a payload this page guesses at.
          if (value.schemaVersion !== 1) {
            setLoad({ phase: 'failed', schemaUnsupported: true, message: `${t('schema')} ${boundedText(value.schemaVersion, 24)}` })
            return
          }
          if (value.ok !== true) {
            setLoad({
              phase: 'failed',
              message: stateError(value, response.status),
              errors: Array.isArray(value.errors) ? value.errors : [],
            })
            return
          }
          setLoad({ phase: 'ready', value })
        } catch (error) {
          // Keep what already loaded: a failed read must label the data, not
          // delete a good reading.
          setLoad(previous => ({ phase: 'failed', value: previous.value, message: plainError(error) }))
        }
      }, [t])

      React.useEffect(() => { void loadState() }, [loadState])

      const refresh = React.useCallback(async () => {
        setObserve({ phase: 'running', items: [] })
        try {
          const { response, value } = await postJson('/_tmax/observe', { scope: 'all' })
          const ok = response.ok && value?.ok === true
          const nextState = value?.state !== null && typeof value?.state === 'object' ? value.state : undefined
          const adopted = adoptState(setLoad, nextState, response, t)
          if (!adopted && ok) await loadState()
          // A refused observation read leaves the old table as last-known data.
          if (!adopted && !response.ok) {
            setLoad(previous => ({ phase: 'failed', value: previous.value, message: stateError(value, response.status) }))
          }
          const partial = response.ok
          setObserve({
            phase: ok ? 'done' : 'failed',
            items: observeItems(value),
            generatedAt: typeof value?.generatedAt === 'string' ? value.generatedAt : undefined,
            // A 200/207 with failed items is explained by those items; only a
            // refused request needs a host error line of its own.
            message: ok ? undefined : partial ? hostCodeText(value?.code, value?.message) || undefined : stateError(value, response.status),
          })
        } catch (error) {
          // Old values stay on screen; only the failure is added, and the table
          // is labeled as last-known rather than current.
          setObserve({ phase: 'failed', items: [], message: plainError(error) })
          setLoad(previous => ({ phase: 'failed', value: previous.value, message: plainError(error) }))
        }
      }, [loadState, t])

      const previewConnect = React.useCallback(async () => {
        setConnect({ phase: 'previewing', lines: [] })
        try {
          const { response, value } = await postJson('/_tmax/connect', { dryRun: true })
          if (!response.ok || value?.ok !== true) {
            setConnect({ phase: 'failed', lines: connectLines(value?.connect, t), message: stateError(value, response.status) })
            return
          }
          setConnect({ phase: 'preview', lines: connectLines(value.connect, t) })
        } catch (error) {
          setConnect({ phase: 'failed', lines: [], message: plainError(error) })
        }
      }, [t])

      const executeConnect = React.useCallback(async () => {
        setConnect(previous => ({ ...previous, phase: 'executing' }))
        try {
          const { response, value } = await postJson('/_tmax/connect', { dryRun: false })
          if (!response.ok || value?.ok !== true) {
            setConnect({ phase: 'failed', lines: connectLines(value?.connect, t), message: stateError(value, response.status) })
            return
          }
          const nextState = value?.state !== null && typeof value?.state === 'object' ? value.state : undefined
          if (!adoptState(setLoad, nextState, response, t)) await loadState()
          setConnect({ phase: 'applied', lines: connectLines(value.connect, t) })
        } catch (error) {
          setConnect({ phase: 'failed', lines: [], message: plainError(error) })
        }
      }, [loadState, t])

      const cancelConnect = React.useCallback(() => { setConnect({ phase: 'idle', lines: [] }) }, [])

      /**
       * One action, previewed or executed. The payload is rebuilt from the
       * descriptor the button carried, so no raw client string ever reaches the
       * host; a preview and its confirmation post the same operands.
       */
      const runRepair = React.useCallback(async (descriptor, execute) => {
        const payload = actionPayload(descriptor, execute !== true)
        if (payload === undefined) return
        const request = { action: payload.action, provider: payload.provider, scope: payload.scope }
        setRepair({ phase: execute === true ? 'executing' : 'previewing', request })
        try {
          const { response, value } = await postJson('/_tmax/action', payload)
          const result = value !== null && typeof value === 'object' && value.result !== null && typeof value.result === 'object'
            ? value.result
            : undefined
          const succeeded = actionSucceeded(value, response.ok)
          // A returned read model is adopted on 200/207; a refused action keeps
          // the last good state on screen.
          const nextState = response.ok && value?.state !== null && typeof value?.state === 'object' ? value.state : undefined
          if (nextState !== undefined) adoptState(setLoad, nextState, response, t)
          // The typed result owns the failure copy; only a resultless refusal
          // needs an HTTP/host line of its own.
          const failureDetail = hostCodeText(result?.code, result?.message)
          setRepair({
            phase: succeeded ? (payload.dryRun ? 'preview' : 'applied') : 'failed',
            request,
            result,
            lines: actionResultLines(result, t, payload.action),
            message: succeeded ? undefined : failureDetail === '' ? stateError(value, response.status) : undefined,
          })
        } catch (error) {
          setRepair(previous => ({ phase: 'failed', request: previous.request, result: undefined, lines: [], message: plainError(error) }))
        }
      }, [t])

      const confirmRepair = React.useCallback(() => {
        const request = repair.request
        if (request === undefined) return
        void runRepair(request, true)
      }, [repair.request, runRepair])

      const cancelRepair = React.useCallback(() => { setRepair({ phase: 'idle' }) }, [])

      const state = load.value !== null && typeof load.value === 'object' && load.value.ok === true ? load.value : undefined
      const verdictInfo = verdict(load, t)
      const action = state === undefined ? undefined : nextAction(load)
      const busy = observe.phase === 'running'
        || connect.phase === 'previewing' || connect.phase === 'executing'
        || repair.phase === 'previewing' || repair.phase === 'executing'
      const repairHandlers = {
        run: descriptor => { void runRepair(descriptor, false) },
        disabled: busy,
      }

      const refreshButton = h('button', {
        type: 'button',
        className: 'tmx-button',
        key: 'refresh',
        onClick: refresh,
        disabled: busy || load.phase === 'loading',
        'data-busy': busy ? 'true' : undefined,
      }, busy ? t('refreshing') : t('refresh'))
      const retryButton = h('button', {
        type: 'button',
        className: 'tmx-button',
        key: 'retry',
        onClick: loadState,
        disabled: load.phase === 'refreshing',
      }, t('retry'))

      return h('div', { className: 'tmx-page', 'data-tmax-page': '1' },
        h('div', { className: 'tmx-scroll' },
          h('div', { className: 'tmx-content' }, [
            headerBlock(state, verdictInfo, t, state === undefined && load.phase === 'failed' ? retryButton : refreshButton),
            h('p', { className: 'tmx-sub', key: 'sub' }, t('subtitle')),
            h('p', { className: 'tmx-sr', key: 'live', role: 'status', 'aria-live': 'polite' },
              announce({ load, observe, connect, repair, verdict: verdictInfo.text }, t)),
            staleBanner(load, t, loadState),
            state === undefined && load.phase === 'failed' ? initialError(load, t, loadState) : null,
            action === undefined ? null : nextActionPanel(action, t, { connect: previewConnect, retry: loadState, disabled: busy }),
            repairPanel(repair, t, { confirm: confirmRepair, cancel: cancelRepair, disabled: busy }),
            observePanel(observe, t),
            connectPanel(connect, t, { execute: executeConnect, cancel: cancelConnect }),
            state === undefined && load.phase === 'loading' ? loadingSection(t) : null,
            state === undefined ? null : accessSection(state, t, repairHandlers),
            state === undefined ? null : modelsSection(state, t),
            state === undefined ? null : providersSection(state, t, repairHandlers),
            state === undefined ? null : diagnosticsSection(state, t),
          ])))
    }

    function headerBlock(state, verdictInfo, t, action) {
      const meta = state === undefined
        ? null
        : `${t('updated')} ${state.generatedAt === undefined ? t('unknown') : clockOf(state.generatedAt) || t('unknown')} · ${t('configRevision')} ${shortRevision(state.configurationRevision) ?? t('unknown')} · ${t('schema')} ${state.schemaVersion === undefined ? t('unknown') : state.schemaVersion}`
      return h('div', { className: 'tmx-heading', key: 'heading' }, [
        h('div', { className: 'tmx-titleBlock', key: 'titleBlock' }, [
          h('h1', { key: 'title' }, [
            t('title'),
            h('span', { className: 'tmx-headState', key: 'state', 'data-tone': verdictInfo.tone }, [
              h('span', { className: 'tmx-dot', key: 'dot', 'data-tone': verdictInfo.tone, 'aria-hidden': 'true' }),
              verdictInfo.text,
            ]),
          ]),
          meta === null ? null : h('div', { className: 'tmx-meta', key: 'meta' }, meta),
        ]),
        h('div', { className: 'tmx-actions', key: 'actions' }, action),
      ])
    }

    function staleBanner(load, t, retry) {
      if (load.phase !== 'failed' || load.value === undefined) return null
      return h('div', { className: 'tmx-banner', key: 'stale' }, [
        h('span', { className: 'tmx-errorText', key: 'text' },
          `${t('hostError')}${t('colon')} ${describeError(load.message, t)} · ${t('staleNotice')}`),
        h('button', { type: 'button', className: 'tmx-button', key: 'retry', onClick: retry }, t('retry')),
      ])
    }

    function initialError(load, t, retry) {
      const failures = sectionErrors(load)
      const headline = load.schemaUnsupported === true
        ? `${t('stateSchemaUnsupported')}${t('colon')} ${boundedText(load.message, 120)}`
        : `${t('hostError')}${t('colon')} ${describeError(load.message, t)}`
      return h('div', { className: 'tmx-card', key: 'initialError' }, [
        h('p', { className: 'tmx-errorText', key: 'text' }, headline),
        ...failures.map((error, index) =>
          h('p', { className: 'tmx-errorText', key: `failure:${index}` }, `${t('readFailed')}${t('colon')} ${error.text}`)),
        h('div', { className: 'tmx-cardNote', key: 'actions' },
          h('button', { type: 'button', className: 'tmx-button', key: 'retry', onClick: retry }, t('retry'))),
      ])
    }

    function loadingSection(t) {
      return h('section', { className: 'tmx-section', key: 'loading' }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' }, t('accessTitle')),
        h('div', { className: 'tmx-card', key: 'card' },
          h('div', { className: 'tmx-facts' }, [0, 1, 2, 3].map(index =>
            h('div', { className: 'tmx-fact', key: index }, [
              h('dt', { key: 'dt' }, ' '),
              h('dd', { key: 'dd' }, h('span', { className: 'tmx-skeletonBar' })),
            ])))),
      ])
    }

    function dot(tone) {
      // Decorative: the adjacent text already states the fact.
      return h('span', { className: 'tmx-dot', key: 'dot', 'data-tone': tone, 'aria-hidden': 'true' })
    }

    function chip(text, tone) {
      return h('span', { className: 'tmx-chip', 'data-tone': tone }, text)
    }

    function fact(label, value, note) {
      return h('div', { className: 'tmx-fact', key: label }, [
        h('dt', { key: 'dt' }, label),
        h('dd', { key: 'dd' }, value),
        note === undefined ? null : h('span', { className: 'tmx-factNote', key: 'note' }, note),
      ])
    }

    function textValue(value, t, max = 160) {
      if (typeof value === 'string' && value !== '') return boundedText(value, max)
      if (typeof value === 'number' && Number.isFinite(value)) return String(value)
      if (typeof value === 'boolean') return value ? t('yes') : t('no')
      return t('unknown')
    }

    function word(value, t) {
      return value === true ? t('yes') : value === false ? t('no') : t('unknown')
    }

    function boolWord(value, t, whenTrue, whenFalse) {
      return value === true ? whenTrue : value === false ? whenFalse : t('unknown')
    }

    function defaultIdentity(value, modelField, effortField, t) {
      if (value === null || typeof value !== 'object') return t('unknown')
      const provider = typeof value.provider === 'string' && value.provider !== ''
        ? boundedText(value.provider, 80)
        : t('unknown')
      const model = typeof value[modelField] === 'string'
        ? boundedText(value[modelField], 80)
        : typeof value.model === 'string' ? boundedText(value.model, 80) : t('unknown')
      const effort = typeof value[effortField] === 'string' && value[effortField] !== '' ? value[effortField] : undefined
      const base = `${provider}/${model}`
      return effort === undefined ? base : `${base} (${boundedText(effort, 16)})`
    }

    function homeIntegrationNote(home, t) {
      if (home === null || typeof home !== 'object') return t('unknown')
      const state = value => value === true ? t('present') : value === false ? t('absent') : t('unknown')
      return `${t('homeEntry')} ${state(home.entry)} · ${t('homeArtifact')} ${state(home.artifact)}`
    }

    function pendingText(value, t) {
      if (value === null) return t('pendingNone')
      if (value === undefined) return t('unknown')
      if (typeof value === 'string') return boundedText(value, 160)
      if (Array.isArray(value)) return value.length === 0 ? t('pendingNone') : `${value.length}`
      if (typeof value === 'object') {
        const text = entryText(value)
        return text === '' ? t('unknown') : text
      }
      return boundedText(value, 160)
    }

    function sectionView(key, title, note, body) {
      return h('section', { className: 'tmx-section', key }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' },
          note === undefined ? title : [title, h('span', { className: 'tmx-sectionNote', key: 'note' }, note)]),
        ...(Array.isArray(body) ? body : [body]),
      ])
    }

    function readFailures(errors, t, prefix) {
      return errors.map((error, index) =>
        h('p', { className: 'tmx-errorText', key: `${prefix}:${index}` }, `${t('readFailed')}${t('colon')} ${error.text}`))
    }

    function accessSection(state, t, handlers) {
      const target = state.target ?? {}
      const ownership = state.ownership ?? {}
      const routes = Array.isArray(state.routes) ? state.routes : []
      const gateway = state.gateway ?? {}
      const config = state.config ?? {}
      const projectionFailures = projectionErrors(state)
      // A projection failure already owns the routes block; do not repeat the
      // same host error as a target note above it.
      const targetErrors = projectionFailures.length > 0 ? [] : errorsForSection(state, 'target')
      const gatewayErrors = errorsForSection(state, 'gateway')
      const configErrors = errorsForSection(state, 'config')
      const match = defaultsMatch(target.default, target.effectiveDefault)
      const blockers = Array.isArray(target.blockers) ? target.blockers : []
      const conflict = ownership.conflict
      const conflictText = conflict === null || conflict === undefined
        ? undefined
        : hostCodeText(conflictCode(conflict), conflictMessage(conflict))
      const facts = [
        fact(t('target'), textValue(target.id, t)),
        fact(t('targetPath'), h('span', { className: 'tmx-mono' }, textValue(target.path, t))),
        fact(t('authority'), h('span', { className: 'tmx-mono' }, textValue(state.authority, t))),
        fact(t('dshInstallation'), target.installation === undefined || target.installation === null
          ? t('unknown')
          : `${textValue(target.installation.version, t)} (${textValue(target.installation.state, t)})`),
        fact(t('ownership'), `${enumText(ownership.owner, OWNER_STATES, t)} · ${enumText(ownership.scope, SCOPES, t)}`,
          homeIntegrationNote(ownership.homeIntegration, t)),
        fact(t('ownershipMode'), enumText(ownershipMode(ownership), MODE_STATES, t)),
        fact(t('applied'), enumText(target.appliedState, APPLIED_STATES, t)),
        fact(t('pending'), pendingText(target.pendingNativeChanges, t)),
        fact(t('tmaxDefault'), defaultIdentity(target.default, 'id', 'effort', t)),
        fact(t('effectiveDefault'), h('span', { className: 'tmx-factValue' }, [
          defaultIdentity(target.effectiveDefault, 'model', 'reasoningEffort', t),
          match === undefined ? null : chip(match ? t('matches') : t('differs'), match ? 'idle' : 'warn'),
        ])),
      ]
      const blockerText = blockers.slice(0, 5)
        .map(blocker => blocker !== null && typeof blocker === 'object' ? hostCodeText(blocker.code, blocker.message) : hostCodeText(undefined, blocker))
        .filter(text => text !== '')
        .join('; ')
      const body = [h('div', { className: 'tmx-card', key: 'facts' }, [
        h('dl', { className: 'tmx-facts', key: 'dl' }, facts),
        blockerText === '' ? null : h('p', { className: 'tmx-errorText', key: 'blockers' },
          `${t('blocker')}${t('colon')} ${boundedText(blockerText, 480)}`),
        conflictText === undefined ? null : h('p', { className: 'tmx-errorText', key: 'conflict' }, `${t('conflict')}${t('colon')} ${conflictText}`),
        ...readFailures(targetErrors, t, 'targetFailure'),
      ])]
      body.push(h('h3', { className: 'tmx-subhead', key: 'routesHead' }, t('routesTitle')))
      if (projectionFailures.length > 0) {
        // A failed projection read means routes are unknown, not absent.
        body.push(h('div', { className: 'tmx-card', key: 'projectionFailure' }, readFailures(projectionFailures, t, 'projectionFailure')))
      }
      if (routes.length > 0) {
        body.push(h('div', { className: 'tmx-routes', key: 'routes' }, routes.map((route, index) => routeCard(route, index, t))))
      } else if (projectionFailures.length === 0) {
        body.push(h('div', { className: 'tmx-card', key: 'noRoutes' }, h('p', { className: 'tmx-empty' }, t('noRoutes'))))
      }
      body.push(h('h3', { className: 'tmx-subhead', key: 'gatewayHead' }, t('gatewayTitle')))
      body.push(gatewayCard(gateway, gatewayErrors, t, handlers))
      body.push(h('h3', { className: 'tmx-subhead', key: 'configHead' }, t('configTitle')))
      body.push(configCard(config, configErrors, t))
      return sectionView('access', t('accessTitle'), undefined, body)
    }

    function routeCard(route, index, t) {
      const readiness = routeReadiness(route)
      const credential = readiness === 'usable'
        ? chip(t('credentialConfigured'), 'ok')
        : readiness === 'registered'
          ? chip(t('credentialMissing'), 'error')
          : h('span', null, t('unknown'))
      const activeState = routeActiveState(route)
      const liveness = activeState === 'active'
        ? chip(t('routeLive'), 'ok')
        : activeState === 'inactive'
          ? chip(t('routeNotRegistered'), 'idle')
          : h('span', null, t('unknown'))
      const ref = typeof route?.credentialRef === 'string' && route.credentialRef !== ''
        ? h('p', { className: 'tmx-factNote', key: 'ref' }, `${t('credentialRef')}${t('colon')} ${boundedText(route.credentialRef, 96)}`)
        : null
      const blockerText = routeBlockerTexts(route).join('; ')
      return h('div', { className: 'tmx-card tmx-route', key: `route:${index}` }, [
        h('h4', { className: 'tmx-routeHead', key: 'head' }, textValue(route?.provider, t)),
        h('dl', { className: 'tmx-facts', key: 'facts' }, [
          fact(t('routeActive'), h('span', { className: 'tmx-factValue' }, [liveness])),
          fact(t('routeProtocol'), textValue(route?.protocol, t)),
          fact(t('routeBaseUrl'), h('span', { className: 'tmx-mono tmx-break' }, textValue(route?.baseUrl, t, 200))),
          fact(t('routeScope'), enumText(route?.scope, SCOPES, t)),
          fact(t('routeCredential'), h('span', { className: 'tmx-factValue' }, [credential])),
        ]),
        ref,
        blockerText === ''
          ? null
          : h('p', { className: 'tmx-errorText', key: 'blockers' },
            `${t('routeBlockers')}${t('colon')} ${boundedText(blockerText, 480)}`),
      ])
    }

    function gatewayCard(gateway, errors, t, handlers) {
      const supervision = gateway.supervision ?? {}
      const autostart = gateway.autostart ?? {}
      const shared = gateway.sharedKey ?? {}
      const readyWord = gateway.ready === true ? t('gatewayReady') : gateway.ready === false ? t('gatewayNotReady') : t('unknown')
      const reasonText = typeof gateway.reasonCode === 'string' && gateway.reasonCode !== ''
        ? enumText(gateway.reasonCode, REASONS, t)
        : undefined
      // One action row: the lifecycle verbs the reported state allows, then the
      // management surface. Each previews through the repair panel first.
      const actionButtons = handlers === undefined ? [] : gatewayActions(gateway).map(descriptor => h('button', {
        type: 'button',
        className: 'tmx-button',
        key: `action:${descriptor.action}`,
        'data-action': descriptor.action,
        onClick: () => handlers.run(descriptor),
        disabled: handlers.disabled,
      }, t(ACTION_LABELS[descriptor.action])))
      const management = managementAction(gateway)
      if (handlers !== undefined && management !== undefined) {
        actionButtons.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'openManagement',
          'data-action': management.action,
          onClick: () => handlers.run(management),
          disabled: handlers.disabled,
        }, t('actionOpenManagement')))
      }
      const actionRow = actionButtons.length === 0
        ? null
        : h('div', { className: 'tmx-providerActions', key: 'actions' }, actionButtons)
      return h('div', { className: 'tmx-card', key: 'gateway' }, [
        h('dl', { className: 'tmx-facts', key: 'dl' }, [
          fact(t('gatewayListen'), h('span', { className: 'tmx-mono' }, textValue(gateway.listen, t))),
          fact(t('gatewayConfigured'), word(gateway.configured, t)),
          fact(t('gatewayReady'), h('span', { className: 'tmx-factValue' }, [
            dot(gateway.ready === true ? 'ok' : gateway.ready === false ? 'error' : 'idle'),
            readyWord,
          ]), reasonText),
          fact(t('gatewayDaemon'), enumText(gateway.daemonDiscoveryState, DAEMON_STATES, t)),
          fact(t('gatewaySupervision'), enumText(supervision.state, SUPERVISION_STATES, t),
            supervision.reasonCode === null || supervision.reasonCode === undefined ? undefined : enumText(supervision.reasonCode, REASONS, t)),
          fact(t('gatewayAutostart'), [
            `${t('savedIntent')} ${word(autostart.savedIntent, t)} · `,
            `${t('registration')} ${enumText(autostart.registration, REGISTRATION_STATES, t)} · `,
            `${t('unit')} ${boolWord(autostart.unitMatches, t, t('unitMatches'), t('unitDrift'))}`,
          ].join('')),
          fact(t('sharedKey'), enumText(shared.loadedState, KEY_STATES, t),
            shared.reasonCode === null || shared.reasonCode === undefined ? undefined : enumText(shared.reasonCode, REASONS, t)),
        ]),
        ...readFailures(errors, t, 'gatewayFailure'),
        actionRow,
      ])
    }

    function configCard(config, errors, t) {
      const revisions = typeof config.revision === 'string' ? config.revision : undefined
      const configErrors = Array.isArray(config.errors) ? config.errors : []
      return h('div', { className: 'tmx-card', key: 'config' }, [
        h('dl', { className: 'tmx-facts', key: 'dl' }, [
          fact(t('configState'), enumText(config.state, CONFIG_STATES, t)),
          fact(t('revision'), h('span', { className: 'tmx-mono tmx-break', title: revisions ?? undefined },
            shortRevision(revisions) ?? t('unknown'))),
          fact(t('configErrors'), configErrors.length === 0 ? t('none') : String(configErrors.length)),
        ]),
        ...readFailures(errors, t, 'configFailure'),
      ])
    }

    function modelsSection(state, t) {
      const routes = Array.isArray(state.routes) ? state.routes : []
      const routing = Array.isArray(state.routing) ? state.routing : []
      const projectionFailures = projectionErrors(state)
      if (routes.length === 0) {
        return sectionView('models', t('modelsTitle'), t('modelsNote'),
          projectionFailures.length > 0
            ? h('div', { className: 'tmx-card' }, readFailures(projectionFailures, t, 'modelsFailure'))
            : h('div', { className: 'tmx-card' }, h('p', { className: 'tmx-empty' }, t('noRoutes'))))
      }
      const modelCount = modelCatalogOf(state).length
      const note = `${t('modelsNote')} · ${modelCount} ${t('modelsUnit')}`
      const body = []
      if (projectionFailures.length > 0) {
        body.push(h('div', { className: 'tmx-card', key: 'projectionFailure' }, readFailures(projectionFailures, t, 'modelsFailure')))
      }
      body.push(...routes.map((route, index) => {
        const models = Array.isArray(route?.models) ? route.models : []
        return h('div', { className: 'tmx-card', key: `models:${index}` }, [
          h('h4', { className: 'tmx-routeHead', key: 'head' }, textValue(route?.provider, t)),
          models.length === 0
            ? h('p', { className: 'tmx-empty', key: 'empty' }, t('notReported'))
            : h('div', { className: 'tmx-models', key: 'models' }, models.map((model, modelIndex) => modelCard(model, routing, modelIndex, t))),
        ])
      }))
      return sectionView('models', t('modelsTitle'), note, body)
    }

    function modelCard(model, routing, index, t) {
      const id = typeof model?.id === 'string' && model.id !== '' ? model.id : undefined
      const rawName = typeof model?.name === 'string' && model.name !== '' ? model.name : id ?? t('unknown')
      // Host-derived names are bounded for display; the raw id still drives the
      // exact routing lookup below.
      const name = boundedText(rawName, 120)
      const routingEntry = id === undefined
        ? undefined
        : routing.find(item => item !== null && typeof item === 'object' && item.model === id)
      const candidateCount = numeric(model?.candidateCount)
      const capabilities = capabilityFacts(model?.capabilities, t)
      return h('div', { className: 'tmx-model', key: `model:${id ?? index}` }, [
        h('h4', { className: 'tmx-modelHead', key: 'head' }, [
          name,
          id === undefined || id === rawName ? null : h('span', { className: 'tmx-chipKind', key: 'id' }, boundedText(id, 120)),
        ]),
        h('dl', { className: 'tmx-facts', key: 'facts' }, [
          fact(t('modelKind'), enumText(model?.kind, KIND_STATES, t)),
          fact(t('modelContinuity'), enumText(model?.continuity, CONTINUITY_STATES, t)),
          fact(t('modelCandidates'), candidateCount === undefined ? t('unknown') : String(candidateCount)),
          fact(t('modelAvailability'), enumText(model?.availability, AVAILABILITY_STATES, t)),
        ]),
        h('dl', { className: 'tmx-facts', key: 'capabilities' }, capabilities.map(item => fact(item.label, item.value))),
        routingBlock(routingEntry, t),
      ])
    }

    function routingBlock(entry, t) {
      if (entry === undefined) {
        return h('p', { className: 'tmx-note', key: 'routing' }, t('routingNone'))
      }
      const selection = entry.selection ?? {}
      const selected = typeof selection.qualifiedModel === 'string' && selection.qualifiedModel !== ''
        ? selection.qualifiedModel
        : [typeof selection.provider === 'string' ? selection.provider : undefined,
          typeof selection.model === 'string' ? selection.model : undefined].filter(Boolean).join('/')
      const priority = numeric(selection.priority)
      const candidates = Array.isArray(entry.candidates) ? entry.candidates : []
      const excluded = Array.isArray(entry.excluded) ? entry.excluded : []
      const errorText = entry.error === null || entry.error === undefined
        ? undefined
        : typeof entry.error === 'object'
          ? hostCodeText(entry.error.code, entry.error.message)
          : hostCodeText(undefined, entry.error)
      return h('div', { className: 'tmx-routing', key: 'routing' }, [
        h('p', { className: 'tmx-routingLine', key: 'selection' }, [
          h('span', { className: 'tmx-chipKind', key: 'label' }, t('routingSelected')),
          h('span', { className: 'tmx-mono tmx-break', key: 'model' }, selected === '' ? t('unknown') : boundedText(selected, 160)),
          priority === undefined ? null : h('span', { className: 'tmx-chipKind', key: 'priority' }, `${t('routingPriority')} ${priority}`),
        ]),
        candidates.length === 0 ? null : h('ul', { className: 'tmx-candidates', key: 'candidates' },
          candidates.slice(0, 20).map((candidate, index) => h('li', { className: 'tmx-candidate', key: index }, [
            h('span', { className: 'tmx-chipKind', key: 'label' }, t('routingCandidates')),
            h('span', { className: 'tmx-mono tmx-break', key: 'model' }, textValue(candidate?.qualifiedModel, t, 160)),
            numeric(candidate?.priority) === undefined
              ? null
              : h('span', { className: 'tmx-chipKind', key: 'priority' }, `${t('routingPriority')} ${numeric(candidate.priority)}`),
            candidate?.selected === true ? chip(t('selected'), 'ok') : null,
          ]))),
        excluded.length === 0 ? null : h('ul', { className: 'tmx-candidates', key: 'excluded' },
          excluded.slice(0, 20).map((item, index) => h('li', { className: 'tmx-candidate', key: index }, [
            h('span', { className: 'tmx-chipKind', key: 'label' }, t('routingExcluded')),
            h('span', { className: 'tmx-break', key: 'text' }, entryText(item) || t('unknown')),
          ]))),
        errorText === undefined ? null : h('p', { className: 'tmx-errorText', key: 'error' },
          `${t('routingError')}${t('colon')} ${errorText}`),
      ])
    }

    function providersSection(state, t, handlers) {
      const providers = state.providers !== null && typeof state.providers === 'object' ? state.providers : {}
      const tmaxProviders = Array.isArray(providers.tmax) ? providers.tmax : []
      const agentProviders = Array.isArray(providers.agent) ? providers.agent : []
      const providersErrors = errorsForSection(state, 'providers')
      // The host reports an unavailable settings service in the providers
      // section; it belongs beside the agent-configured list, not the tmax one.
      const agentSettingErrors = providersErrors.filter(error => /agent.*settings/i.test(error.code))
      const providerErrors = providersErrors.filter(error => !agentSettingErrors.includes(error))
      const agentErrors = [...errorsForSection(state, 'agentSettings'), ...agentSettingErrors]
      const oldest = oldestReading([...tmaxProviders, ...agentProviders])
      const body = [
        h('p', { className: 'tmx-caption', key: 'tmaxNote' }, oldest === undefined
          ? t('tmaxProvidersNote')
          : `${t('tmaxProvidersNote')} · ${t('oldestReading')} ${clockOf(oldest) || t('unknown')}`),
        ...readFailures(providerErrors, t, 'providerFailure'),
        h('h3', { className: 'tmx-subhead', key: 'tmaxHead' }, t('tmaxProviders')),
      ]
      if (tmaxProviders.length === 0) {
        // A failed provider read is not an empty provider list.
        if (providerErrors.length === 0) {
          body.push(h('div', { className: 'tmx-card', key: 'tmaxEmpty' }, h('p', { className: 'tmx-empty' }, t('unknownQuota'))))
        }
      } else {
        body.push(h('div', { className: 'tmx-providers', key: 'tmaxList' }, tmaxProviders.map((provider, index) => providerCard(provider, index, t, handlers))))
      }
      body.push(h('h3', { className: 'tmx-subhead', key: 'agentHead' }, t('agentProviders')))
      body.push(h('p', { className: 'tmx-caption', key: 'agentNote' }, t('agentProvidersNote')))
      body.push(...readFailures(agentErrors, t, 'agentFailure'))
      if (agentProviders.length === 0) {
        // The provider-list read covers both lists; an agent-settings read can
        // fail on its own. Either way, an empty list is not a fact.
        if (agentErrors.length === 0 && providerErrors.length === 0) {
          body.push(h('div', { className: 'tmx-card', key: 'agentEmpty' }, h('p', { className: 'tmx-empty' }, t('notReported'))))
        }
      } else {
        body.push(h('div', { className: 'tmx-providers', key: 'agentList' }, agentProviders.map((record, index) => agentCard(record, index, t))))
      }
      return sectionView('providers', t('providersTitle'), undefined, body)
    }

    function windowsFor(provider, t) {
      const readings = (Array.isArray(provider?.billing?.rows) ? provider.billing.rows : [])
        .map((row, index) => windowReading(row, index, t))
        .filter(Boolean)
      return disambiguate(readings)
    }

    function providerCard(provider, index, t, handlers) {
      const billing = provider?.billing !== null && typeof provider?.billing === 'object' ? provider.billing : undefined
      const name = boundedText(typeof provider?.name === 'string' && provider.name !== ''
        ? provider.name
        : typeof provider?.id === 'string' && provider.id !== '' ? provider.id : t('unknown'), 120)
      const dashboard = safeDashboard(billing?.dashboard)
      const title = dashboard === undefined
        ? h('span', { className: 'tmx-providerName', title: name, key: 'name' }, name)
        : h('a', {
          className: 'tmx-providerName',
          href: dashboard,
          target: '_blank',
          rel: 'noreferrer',
          title: `${name} · ${t('dashboard')}`,
          key: 'name',
        }, [name, h(ExternalMark, { key: 'mark' })])
      const chips = [
        provider?.enabled === false ? chip(t('providerDisabled'), 'idle') : null,
        provider?.suspension?.state === 'suspended' ? chip(t('providerSuspended'), 'error') : null,
      ]
      const meta = []
      if (typeof provider?.authentication === 'string' && provider.authentication !== '') {
        meta.push(`${t('authentication')} ${boundedText(provider.authentication, 32)}`)
      }
      if (typeof provider?.integration === 'string' && provider.integration !== '') {
        meta.push(`${t('integration')} ${boundedText(provider.integration, 48)}`)
      }
      const affected = numeric(provider?.affectedTargets)
      if (affected !== undefined) meta.push(`${t('affectedTargets')} ${affected}`)
      if (billing !== undefined) {
        meta.push(`${t('billingKind')} ${enumText(billing.kind, BILLING_KINDS, t)}`)
        meta.push(`${t('freshness')} ${enumText(billing.freshness, FRESHNESS_STATES, t)}`)
        meta.push(`${t('observedAt')} ${billing.observedAt === null ? t('neverObserved') : clockOf(billing.observedAt) || t('unknown')}`)
      }
      const limit = limitState(provider)
      const windows = limit === 'rows' ? windowsFor(provider, t) : []
      const suspension = suspensionLine(provider?.suspension, t)
      const failure = failureLine(provider?.failure, t)
      const error = provider?.error === null || provider?.error === undefined
        ? undefined
        : hostCodeText(provider.error.code, provider.error.message)
      const noteTexts = billingNoteTexts(provider, t)
      const plan = subscriptionLine(billing?.subscription ?? provider?.subscription ?? provider?.plan, t)
      const content = [
        h('h4', { className: 'tmx-providerHead', key: 'head' }, [title, ...chips]),
      ]
      if (meta.length > 0) content.push(h('p', { className: 'tmx-providerMeta', key: 'meta' }, meta.join(' · ')))
      if (plan !== undefined) content.push(harnessLine(plan, 'plan'))
      if (limit === 'rows') {
        content.push(h('p', { className: 'tmx-factNote', key: 'caption' }, t('windowsCaption')))
        content.push(h('ul', { className: 'tmx-windows', key: 'windows' },
          windows.map((item, windowIndex) => windowRow(item, windowIndex, name, t))))
      } else {
        content.push(h('p', { className: 'tmx-empty', key: 'limits' },
          limit === 'error'
            ? `${t('providerError')}${t('colon')} ${error ?? t('unknown')}`
            : limitMessage(provider, t)))
      }
      if (suspension !== undefined) content.push(h('p', { className: 'tmx-suspend', key: 'suspension' }, suspension))
      if (failure !== undefined) content.push(h('p', { className: 'tmx-suspend', key: 'failure' }, failure))
      if (error !== undefined && limit !== 'error') {
        content.push(h('p', { className: 'tmx-errorText', key: 'error' }, `${t('providerError')}${t('colon')} ${error}`))
      }
      if (noteTexts.length > 0) {
        content.push(h('p', { className: 'tmx-evidence', key: 'notes' }, `${t('notes')}${t('colon')} ${noteTexts.join(' · ')}`))
      }
      // A button exists only where its evidence does: a suspension, an active
      // failure, or an explicit observation. The observation is the page's only
      // quota-scope contact with a provider source, and it is a deliberate click.
      const actionButtons = handlers === undefined ? [] : providerActions(provider).map(descriptor => h('button', {
        type: 'button',
        className: 'tmx-button',
        key: `action:${descriptor.action}`,
        'data-action': descriptor.action,
        onClick: () => handlers.run(descriptor),
        disabled: handlers.disabled,
      }, t(ACTION_LABELS[descriptor.action])))
      if (actionButtons.length > 0) {
        content.push(h('div', {
          className: 'tmx-providerActions',
          key: 'actions',
          role: 'group',
          'aria-label': t('actionsLabel'),
        }, actionButtons))
      }
      return h('article', { className: 'tmx-card tmx-provider', key: `provider:${provider?.id ?? index}` }, content)
    }

    function suspensionLine(suspension, t) {
      if (suspension?.state !== 'suspended') return undefined
      const parts = [t('quotaSuspended')]
      if (typeof suspension.window === 'string' && LABEL_KEYS[suspension.window] !== undefined) {
        parts.push(t(LABEL_KEYS[suspension.window]))
      }
      const reset = resetAt(suspension.resetAt)
      if (reset !== undefined) parts.push(`${t('reset')} ${formatDate(reset)}`)
      if (typeof suspension.reasonCode === 'string' && suspension.reasonCode !== '') {
        parts.push(boundedText(suspension.reasonCode, 48))
      }
      return parts.join(' · ')
    }

    function failureLine(failure, t) {
      if (failure === null || typeof failure !== 'object') return undefined
      const failures = numeric(failure.consecutiveFailures) ?? 0
      const state = typeof failure.state === 'string' ? failure.state : undefined
      // A normal provider reports an uninvolved state; only an actual failure
      // or an active override is evidence worth drawing.
      if (failures === 0 && state !== 'active') return undefined
      const parts = [`${t('failureOverride')} ${enumText(state, FAILURE_STATES, t)}`]
      if (failures > 0) parts.push(`${t('consecutiveFailures')} ${failures}`)
      const expires = typeof failure.expiresAt === 'string' ? new Date(failure.expiresAt) : undefined
      if (expires !== undefined && !Number.isNaN(expires.getTime())) parts.push(`${t('expires')} ${formatDate(expires)}`)
      if (typeof failure.reasonCode === 'string' && failure.reasonCode !== '') parts.push(boundedText(failure.reasonCode, 48))
      return parts.join(' · ')
    }

    function subscriptionLine(subscription, t) {
      if (subscription === null || typeof subscription !== 'object') return undefined
      const parts = []
      const level = subscription.plan ?? subscription.level ?? subscription.kind
      const planText = typeof level === 'string' ? boundedText(level, 32) : undefined
      if (planText !== undefined) parts.push(`${t('plan')} ${planText}`)
      const renewal = subscription.renewalAt ?? subscription.renewsAt ?? subscription.expiresAt ?? subscription.renewal
      const renewalDate = typeof renewal === 'string' || typeof renewal === 'number' ? new Date(renewal) : undefined
      if (renewalDate !== undefined && !Number.isNaN(renewalDate.getTime())) {
        parts.push(`${t('renewal')} ${formatDate(renewalDate)}`)
      }
      if (parts.length === 0) return undefined
      return parts.join(' · ')
    }

    function harnessLine(text, key) {
      return h('p', { className: 'tmx-factNote', key: `line:${key}` }, text)
    }

    function agentCard(record, index, t) {
      const name = boundedText(typeof record?.displayName === 'string' && record.displayName !== ''
        ? record.displayName
        : typeof record?.provider === 'string' && record.provider !== '' ? record.provider : t('unknown'), 120)
      const meta = []
      if (typeof record?.provider === 'string' && record.provider !== '') meta.push(boundedText(record.provider, 80))
      if (typeof record?.baseURL === 'string' && record.baseURL !== '') meta.push(`${t('baseUrl')} ${boundedText(record.baseURL, 120)}`)
      meta.push(`${t('agentState')} ${enumText(record?.state, AGENT_STATES, t)}`)
      if (record?.observedAt === null) meta.push(`${t('observedAt')} ${t('neverObserved')}`)
      else if (typeof record?.observedAt === 'string') meta.push(`${t('observedAt')} ${clockOf(record.observedAt) || t('unknown')}`)
      const models = Array.isArray(record?.models) ? record.models : []
      const rows = Array.isArray(record?.rows) ? record.rows : []
      const readings = disambiguate(rows.map((row, rowIndex) => windowReading(row, rowIndex, t)).filter(Boolean))
      const reasonCode = typeof record?.reasonCode === 'string' && record.reasonCode !== '' ? record.reasonCode : undefined
      const reason = reasonCode === undefined ? undefined : enumText(reasonCode, AGENT_REASONS, t)
      const message = boundedText(record?.message, 200)
      const plan = subscriptionLine(record?.plan, t)
      return h('article', { className: 'tmx-card tmx-provider', key: `agent:${record?.id ?? index}` }, [
        h('h4', { className: 'tmx-providerHead', key: 'head' },
          h('span', { className: 'tmx-providerName', title: name }, name)),
        h('p', { className: 'tmx-providerMeta', key: 'meta' }, meta.join(' · ')),
        models.length === 0 ? null : h('p', { className: 'tmx-factNote', key: 'models' }, `${t('models')}${t('colon')} ${models.slice(0, 20).map(model => boundedText(model, 48)).join(' · ')}`),
        plan === undefined ? null : harnessLine(plan, 'plan'),
        reason === undefined ? null : h('p', { className: 'tmx-suspend', key: 'reason' },
          message === '' ? reason : `${reason}${t('colon')} ${message}`),
        readings.length === 0
          // The reason line already explains why no reading exists; two
          // sentences for one absence would read like two facts.
          ? (reasonCode === undefined
            ? h('p', { className: 'tmx-empty', key: 'limits' }, t('unknownQuota'))
            : null)
          : h('ul', { className: 'tmx-windows', key: 'windows' },
            readings.map((item, windowIndex) => windowRow(item, windowIndex, name, t))),
      ])
    }

    function windowRow(item, index, providerName, t) {
      const reset = item.resetUnqualified === true
        ? h('span', { className: 'tmx-windowReset', key: 'reset' }, t('resetUnqualified'))
        : item.reset === undefined
          ? h('span', { className: 'tmx-windowReset', key: 'reset', 'data-empty': 'true' }, '')
          : h('span', { className: 'tmx-windowReset', key: 'reset' }, `${t('reset')} ${formatDate(item.reset)}`)
      if (item.unreadable === true) {
        return h('li', { className: 'tmx-window', key: String(index) }, [
          h('span', { className: 'tmx-windowName', title: item.raw ?? item.name, key: 'name' }, item.name),
          h('span', { className: 'tmx-windowValue tmx-windowWide', key: 'value' }, item.text),
          reset,
        ])
      }
      const tone = toneFor(item.consumed)
      const fill = item.zero === true
        ? h('span', { className: 'tmx-barZero', key: 'zero' })
        : h('span', {
          className: 'tmx-barFill',
          key: 'fill',
          'data-tone': tone,
          style: { width: `${Math.max(item.consumed, 1.5)}%` },
        })
      return h('li', { className: 'tmx-window', key: String(index) }, [
        h('span', { className: 'tmx-windowName', title: item.raw ?? item.name, key: 'name' }, item.name),
        h('span', {
          className: 'tmx-bar',
          key: 'bar',
          role: 'meter',
          'aria-valuenow': item.meterNow,
          'aria-valuemin': 0,
          'aria-valuemax': 100,
          'aria-label': `${providerName} ${item.name}`,
          'aria-valuetext': item.spoken,
        }, fill),
        h('span', { className: 'tmx-windowValue', key: 'value', 'data-tone': tone }, item.text),
        reset,
      ])
    }


    // ---------------------------------------------------------- request history
    //
    // History is a retained, bounded, process-scoped record. The page reads it
    // only after an explicit click, says what it covers and what is missing, and
    // reports an unavailable read as unavailable rather than an empty list.

    /** Rows rendered per read; the rest is named as retained but not drawn. */
    const HISTORY_ROW_LIMIT = 20

    /**
     * Coverage as the host reports it, with material gaps named. Accepts the
     * host's camelCase mapping and tmax's own snake_case fields; absent or
     * unreadable coverage is unknown, never an empty history.
     */
    function historyCoverageLines(coverage, t) {
      if (coverage === null || typeof coverage !== 'object' || Array.isArray(coverage)) {
        return [{ label: t('historyCoverage'), value: t('historyUnknown'), gap: true }]
      }
      const field = (camel, snake) => coverage[camel] !== undefined ? coverage[camel] : coverage[snake]
      const lines = []
      const process = field('processInstance', 'process_instance')
      if (typeof process === 'string' && process !== '') lines.push({ label: t('historyProcess'), value: boundedText(process, 80) })
      const scope = coverage.scope
      if (typeof scope === 'string' && scope !== '') lines.push({ label: t('historyCoverage'), value: boundedText(scope, 48) })
      const retained = numeric(field('retainedCount', 'retained_count'))
      if (retained !== undefined) lines.push({ label: t('historyRetained'), value: String(retained) })
      const evicted = numeric(field('evicted'))
      if (evicted !== undefined) lines.push({ label: t('historyEvicted'), value: String(evicted), gap: evicted > 0 })
      const incomplete = field('incompleteCapture', 'incomplete_capture')
      const incompleteCount = numeric(incomplete)
      if (typeof incomplete === 'boolean') {
        lines.push({ label: t('historyIncomplete'), value: incomplete ? t('yes') : t('no'), gap: incomplete })
      } else if (incompleteCount !== undefined) {
        lines.push({ label: t('historyIncomplete'), value: String(incompleteCount), gap: incompleteCount > 0 })
      }
      const prior = field('priorProcessCoverage', 'prior_process_coverage')
      if (typeof prior === 'string' && prior !== '') {
        lines.push({ label: t('historyPriorProcesses'), value: boundedText(prior, 48), gap: prior !== 'available' })
      }
      const direct = field('directTrafficCoverage', 'direct_traffic_coverage')
      if (typeof direct === 'string' && direct !== '') {
        lines.push({ label: t('historyDirectTraffic'), value: boundedText(direct, 48), gap: direct !== 'observed' })
      }
      const oldest = field('oldest')
      const newest = field('newest')
      if (typeof oldest === 'string' || typeof newest === 'string') {
        const from = typeof oldest === 'string' ? clockOf(oldest) || boundedText(oldest, 40) : t('unknown')
        const to = typeof newest === 'string' ? clockOf(newest) || boundedText(newest, 40) : t('unknown')
        lines.push({ label: t('historyRange'), value: `${from} – ${to}` })
      }
      const retention = field('retention')
      if (typeof retention === 'string' && retention !== '') lines.push({ label: t('historyRetention'), value: boundedText(retention, 120) })
      const gaps = field('gaps')
      if (Array.isArray(gaps)) {
        for (const gap of gaps.slice(0, 6)) {
          const text = typeof gap === 'string' ? boundedText(gap, 80) : entryText(gap)
          if (text !== '') lines.push({ label: t('historyGap'), value: text, gap: true })
        }
      }
      if (lines.length === 0) return [{ label: t('historyCoverage'), value: t('historyUnknown'), gap: true }]
      return lines
    }

    /**
     * Map retained records to bounded table rows. Body/secret fields are never
     * read: the contract carries metadata only, and only these fields are used.
     */
    function historyRows(records) {
      const raw = Array.isArray(records) ? records : []
      const rows = raw.slice(0, HISTORY_ROW_LIMIT).map((record, index) => {
        const item = record !== null && typeof record === 'object' ? record : {}
        const request = boundedText(item.request, 64)
        const route = [boundedText(item.target, 60), boundedText(item.provider, 60), boundedText(item.agentProvider, 60)]
          .filter(text => text !== '')
          .join(' · ')
        return {
          key: `${request === '' ? 'row' : request}:${index}`,
          time: typeof item.time === 'string' ? boundedText(item.time, 40) : '',
          outcome: boundedText(item.outcome, 32),
          requestedModel: boundedText(item.requestedModel ?? item.model, 80),
          qualifiedModel: boundedText(item.qualifiedModel, 120),
          route,
          reason: boundedText(item.reasonCode, 64),
          durationMs: numeric(item.durationMs),
        }
      })
      return { rows, total: raw.length, shown: rows.length }
    }

    /** How many retained records the drawn table does not show. */
    function historyRemaining(result) {
      const records = Array.isArray(result?.records) ? result.records.length : 0
      const omitted = numeric(result?.omitted)
      const beyond = Math.max(0, records - HISTORY_ROW_LIMIT)
      return beyond + (omitted === undefined ? 0 : Math.max(0, Math.trunc(omitted)))
    }

    /** A failed history read as code + bounded message + blockers; never success. */
    function historyFailure(value, status) {
      const record = value !== null && typeof value === 'object' ? value : {}
      const code = typeof record.code === 'string' && record.code !== ''
        ? boundedText(record.code, 48)
        : status === 0 ? 'history_unreachable' : `http_${status}`
      const message = typeof record.message === 'string' && record.message !== ''
        ? boundedText(record.message, 240)
        : typeof record.error === 'string' && record.error !== '' ? boundedText(record.error, 240) : ''
      const blockers = Array.isArray(record.blockers)
        ? record.blockers.slice(0, 6).map(blocker => hostCodeText(blocker?.code, blocker?.message) || entryText(blocker)).filter(text => text !== '')
        : []
      return { code, message, blockers }
    }

    /** Outcome tone for the history table; only known words get a colour. */
    function historyTone(outcome) {
      const text = typeof outcome === 'string' ? outcome.toLowerCase() : ''
      if (text === 'failed' || text === 'error' || text === 'rejected' || text === 'blocked') return 'error'
      if (text === 'completed' || text === 'succeeded' || text === 'success' || text === 'ok') return 'ok'
      return 'idle'
    }

    function historyCoverageView(coverage, t) {
      return h('ul', { className: 'tmx-historyCoverage' }, historyCoverageLines(coverage, t).map((line, index) =>
        h('li', {
          className: 'tmx-historyCoverageItem',
          key: `${line.label}:${index}`,
          'data-gap': line.gap === true ? 'true' : undefined,
        }, [
          h('span', { className: 'tmx-chipKind', key: 'label' }, line.label),
          h('span', { className: 'tmx-break', key: 'value' }, line.value),
          line.gap === true ? chip(t('historyGap'), 'warn') : null,
        ])))
    }

    /**
     * Explicit history detail. The only fetch runs from the button: a mount, a
     * focus change, or a visibility change reads nothing.
     */
    function HistoryPanel(props) {
      const t = props.t
      const [load, setLoad] = React.useState({ phase: 'idle' })
      const availability = historyFact({ history: props.history }, t).value

      async function loadHistory() {
        setLoad({ phase: 'loading' })
        try {
          const response = await fetch(new URL('/_tmax/history', document.baseURI), { credentials: 'same-origin' })
          const value = await readBody(response)
          if (!response.ok || value === null || typeof value !== 'object' || value.ok !== true) {
            setLoad({
              phase: 'failed',
              failure: historyFailure(value, response.status),
              coverage: value !== null && typeof value === 'object' ? value.coverage : undefined,
            })
            return
          }
          if (!Array.isArray(value.records)) {
            // A success envelope without a record list is unknown coverage, not
            // an empty history.
            setLoad({
              phase: 'failed',
              failure: { code: 'history_unknown', message: t('historyUnknown'), blockers: [] },
              coverage: value.coverage,
            })
            return
          }
          setLoad({ phase: 'ready', result: value })
        } catch (error) {
          setLoad({ phase: 'failed', failure: { code: 'history_unreachable', message: plainError(error), blockers: [] } })
        }
      }

      const busy = load.phase === 'loading'
      const actions = []
      if (busy) {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'busy',
          disabled: true,
          'data-busy': 'true',
        }, t('historyLoading')))
      } else {
        const label = load.phase === 'ready' ? t('historyReload') : load.phase === 'failed' ? t('retry') : t('historyLoad')
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'load',
          'data-kind': load.phase === 'idle' ? 'primary' : undefined,
          'data-action': 'history_load',
          onClick: () => { void loadHistory() },
        }, label))
      }
      if (load.phase === 'ready' || load.phase === 'failed') {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'close',
          onClick: () => setLoad({ phase: 'idle' }),
        }, t('close')))
      }

      const body = [h('p', { className: 'tmx-historyAvailability', key: 'availability' },
        `${t('historyAvailability')}${t('colon')} ${availability}`)]
      if (load.phase === 'loading') {
        body.push(h('p', { className: 'tmx-note', key: 'loading' }, t('historyLoading')))
      } else if (load.phase === 'failed') {
        body.push(h('p', { className: 'tmx-errorText tmx-break', key: 'unavailable' },
          `${t('historyReadUnavailable')} ${t('colon')} ${hostCodeText(load.failure.code, load.failure.message) || load.failure.code}`))
        for (const blocker of load.failure.blockers) {
          body.push(h('p', { className: 'tmx-errorText tmx-break', key: `blocker:${blocker}` },
            `${t('toolBlockers')}${t('colon')} ${blocker}`))
        }
        if (load.coverage !== undefined && load.coverage !== null) {
          body.push(h('div', { className: 'tmx-historyCoverageCard', key: 'coverage' }, historyCoverageView(load.coverage, t)))
        }
      } else if (load.phase === 'ready') {
        const result = load.result
        body.push(h('div', { className: 'tmx-historyCoverageCard', key: 'coverage' }, historyCoverageView(result.coverage, t)))
        if (typeof result.generatedAt === 'string' && result.generatedAt !== '') {
          body.push(h('p', { className: 'tmx-note', key: 'generated' },
            `${t('historyGeneratedAt')}${t('colon')} ${clockOf(result.generatedAt) || boundedText(result.generatedAt, 40)}`))
        }
        const rows = historyRows(result.records)
        if (rows.rows.length === 0) {
          body.push(h('p', { className: 'tmx-empty', key: 'empty' }, t('historyEmpty')))
        } else {
          body.push(h('div', { className: 'tmx-historyTableWrap', key: 'table' },
            h('table', { className: 'tmx-historyTable' }, [
              h('thead', { key: 'head' }, h('tr', null, [
                h('th', { key: 'time', scope: 'col' }, t('historyTime')),
                h('th', { key: 'outcome', scope: 'col' }, t('historyOutcome')),
                h('th', { key: 'requested', scope: 'col' }, t('historyRequestedModel')),
                h('th', { key: 'selected', scope: 'col' }, t('historySelectedModel')),
                h('th', { key: 'route', scope: 'col' }, t('historyRoute')),
                h('th', { key: 'reason', scope: 'col' }, t('historyReason')),
              ])),
              h('tbody', { key: 'body' }, rows.rows.map(row => h('tr', { key: row.key }, [
                h('td', { key: 'time' }, clockOf(row.time) || row.time || t('unknown')),
                h('td', { key: 'outcome' }, row.outcome === '' ? t('unknown') : chip(row.outcome, historyTone(row.outcome))),
                h('td', { className: 'tmx-mono tmx-break', key: 'requested' }, row.requestedModel || t('unknown')),
                h('td', { className: 'tmx-mono tmx-break', key: 'selected' }, row.qualifiedModel || t('unknown')),
                h('td', { className: 'tmx-break', key: 'route' }, row.route || t('unknown')),
                h('td', { className: 'tmx-break', key: 'reason' }, row.reason || t('none')),
              ]))),
            ])))
        }
        const more = historyRemaining(result)
        if (more > 0) body.push(h('p', { className: 'tmx-note', key: 'more' }, `${more} ${t('historyMore')}`))
      }
      return h('div', { className: 'tmx-historyCard', 'data-tmax-history': load.phase, key: 'history' }, [
        h('h3', { className: 'tmx-historyHead', key: 'head' }, t('history')),
        ...body,
        h('div', { className: 'tmx-seatActions', key: 'actions' }, actions),
      ])
    }

    function diagnosticsSection(state, t) {
      const diagnostics = state.diagnostics ?? {}
      const config = state.config ?? {}
      const history = historyFact(diagnostics, t)
      const lastError = lastErrorFact(diagnostics, t)
      const revision = typeof state.configurationRevision === 'string' ? state.configurationRevision : undefined
      const errors = Array.isArray(config.errors) ? config.errors : []
      const facts = [
        fact(t('tmaxVersion'), textValue(state.tmaxVersion, t)),
        fact(t('projectionSchema'), state.schemaVersion === undefined ? t('unknown') : String(state.schemaVersion)),
        fact(t('generatedAt'), state.generatedAt === undefined ? t('unknown') : clockOf(state.generatedAt) || t('unknown')),
        fact(t('lastProjection'), diagnostics.lastProjectionAt === undefined ? t('unknown') : clockOf(diagnostics.lastProjectionAt) || t('unknown')),
        fact(t('history'), history.value),
        fact(t('lastError'), lastError.value),
        fact(t('revision'), h('span', { className: 'tmx-mono tmx-break', title: revision ?? undefined }, shortRevision(revision) ?? t('unknown'))),
      ]
      const body = [h('div', { className: 'tmx-card', key: 'facts' }, h('dl', { className: 'tmx-facts' }, facts))]
      const readErrors = sectionErrors(state)
      if (readErrors.length > 0) {
        body.push(h('ul', { className: 'tmx-list', key: 'readErrors' }, readErrors.map((error, index) =>
          h('li', { className: 'tmx-listItem', key: index }, h('span', { className: 'tmx-errorText tmx-break' },
            `${t('readFailed')}${t('colon')} ${error.text}`)))))
      }
      if (errors.length > 0) {
        body.push(h('ul', { className: 'tmx-list', key: 'errors' }, errors.slice(0, 20).map((error, index) => {
          // A config error is host evidence too: its code leads, its text is bounded.
          const text = error !== null && typeof error === 'object'
            ? hostCodeText(error.code ?? error.reasonCode, error.message ?? error.error) || entryText(error)
            : hostCodeText(undefined, error)
          return h('li', { className: 'tmx-listItem', key: index }, h('span', { className: 'tmx-errorText tmx-break' },
            text === '' ? t('unknown') : text))
        })))
      }
      // Request history: availability first, then one explicit read on demand.
      body.push(h(HistoryPanel, { t, history: diagnostics.history, key: 'historyPanel' }))
      return sectionView('diagnostics', t('diagnosticsTitle'), undefined, body)
    }

    function statusLabel(status, t) {
      return status === '' ? t('unknown') : enumText(status, OBSERVE_STATUS, t)
    }

    function sourceLabel(source, t) {
      if (source === 'tmax') return t('observeSourceTmax')
      if (source === 'agent') return t('observeSourceAgent')
      return source === '' ? t('unknown') : source
    }

    function observePanel(observe, t) {
      if (observe === null || observe === undefined || observe.phase === 'idle') return null
      const title = observe.phase === 'failed'
        ? t('observeFailed')
        : observe.phase === 'running' ? t('observeRunning') : t('observeDone')
      return h('div', { className: 'tmx-panel', key: 'observe' }, [
        h('h2', { className: 'tmx-panelTitle', key: 'title' }, [
          title,
          observe.generatedAt === undefined
            ? null
            : h('span', { className: 'tmx-sectionNote', key: 'at' }, `${t('observeAt')} ${clockOf(observe.generatedAt) || t('unknown')}`),
        ]),
        observe.phase === 'failed' && observe.message !== undefined
          ? h('p', { className: 'tmx-errorText', key: 'message' }, `${t('hostError')}${t('colon')} ${describeError(observe.message, t)}`)
          : null,
        observe.phase === 'running'
          ? null
          : observe.items.length === 0
            ? h('p', { className: 'tmx-note', key: 'none' }, t('observeNoItems'))
            : h('ul', { className: 'tmx-list', key: 'items' }, observe.items.map(item =>
              h('li', { className: 'tmx-listItem', key: item.key }, [
                dot(item.tone),
                h('span', { className: 'tmx-listMain', key: 'main' },
                  item.id === '' ? sourceLabel(item.source, t) : `${sourceLabel(item.source, t)} · ${item.id}`),
                h('span', { className: 'tmx-chipKind', key: 'status' }, statusLabel(item.status, t)),
                item.detail === '' ? null : h('span', { className: 'tmx-factNote tmx-break', key: 'detail' }, item.detail),
              ]))),
        h('p', { className: 'tmx-note', key: 'hint' }, t('observeHint')),
      ])
    }

    function connectPanel(connect, t, handlers) {
      if (connect === null || connect === undefined || connect.phase === 'idle') return null
      const preview = connect.phase === 'preview'
      const applying = connect.phase === 'executing'
      const failed = connect.phase === 'failed'
      const title = failed
        ? t('connectFailed')
        : connect.phase === 'applied' ? t('connectApplied') : t('connectPreview')
      const actions = []
      if (preview) {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'apply',
          'data-kind': 'primary',
          onClick: handlers.execute,
          disabled: applying,
        }, t('connectApply')))
        actions.push(h('button', { type: 'button', className: 'tmx-button', key: 'cancel', onClick: handlers.cancel }, t('connectCancel')))
      } else if (applying || connect.phase === 'previewing') {
        actions.push(h('button', { type: 'button', className: 'tmx-button', key: 'busy', 'data-kind': 'primary', disabled: true, 'data-busy': 'true' },
          applying ? t('connectApplying') : t('checking')))
      } else {
        actions.push(h('button', { type: 'button', className: 'tmx-button', key: 'close', onClick: handlers.cancel }, t('close')))
      }
      return h('div', { className: 'tmx-panel', key: 'connect' }, [
        h('h2', { className: 'tmx-panelTitle', key: 'title' }, title),
        preview ? h('p', { className: 'tmx-panelNote', key: 'note' }, t('connectPreviewNote')) : null,
        failed && connect.message !== undefined
          ? h('p', { className: 'tmx-errorText', key: 'message' }, `${t('hostError')}${t('colon')} ${describeError(connect.message, t)}`)
          : null,
        connect.lines.length === 0
          ? h('p', { className: 'tmx-note', key: 'none' }, t('connectNoDetails'))
          : h('dl', { className: 'tmx-connectLines', key: 'lines' }, connect.lines.map(group =>
            h('div', { className: 'tmx-connectGroup', key: group.label }, [
              h('dt', { key: 'label' }, group.label),
              h('dd', { key: 'items' }, h('ul', { className: 'tmx-inlineList' },
                group.items.map((item, index) => h('li', { className: 'tmx-break', key: index }, item)))),
            ]))),
        h('div', { className: 'tmx-panelActions', key: 'actions' }, actions),
      ])
    }

    /**
     * One generic repair panel, separate from Refresh and Apply. A provider
     * action and Open management share it: the same preview gate, the same typed
     * result groups, and the same explicit confirmation.
     */
    function repairPanel(repair, t, handlers) {
      if (repair === null || repair === undefined || repair.phase === 'idle') return null
      const request = repair.request ?? {}
      const name = ACTION_LABELS[request.action] === undefined ? t('unknown') : t(ACTION_LABELS[request.action])
      const provider = typeof request.provider === 'string' && request.provider !== ''
        ? ` · ${boundedText(request.provider, 96)}`
        : ''
      const failed = repair.phase === 'failed'
      const title = failed
        ? t('actionFailed')
        : repair.phase === 'applied'
          ? t('actionDone')
          : repair.phase === 'preview' ? t('actionPreviewTitle') : t('actionWorking')
      const actions = []
      if (repair.phase === 'preview') {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'confirm',
          'data-kind': 'primary',
          'data-action': 'confirm',
          onClick: handlers.confirm,
          disabled: handlers.disabled,
        }, t('actionConfirm')))
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'cancel',
          'data-action': 'cancel',
          onClick: handlers.cancel,
        }, t('actionCancel')))
      } else if (repair.phase === 'previewing' || repair.phase === 'executing') {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'busy',
          'data-kind': 'primary',
          disabled: true,
          'data-busy': 'true',
        }, t('actionWorking')))
      } else {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'close',
          'data-action': 'close',
          onClick: handlers.cancel,
        }, t('close')))
      }
      const lines = Array.isArray(repair.lines) ? repair.lines : []
      const gatewayAction = GATEWAY_ACTIONS.includes(request.action)
      return h('div', { className: 'tmx-panel', key: 'repair', 'data-repair-phase': repair.phase }, [
        h('h2', { className: 'tmx-panelTitle', key: 'title' }, `${title}${t('colon')} ${name}${provider}`),
        repair.phase === 'preview'
          ? h('p', { className: 'tmx-panelNote', key: 'note' }, gatewayAction ? t('actionGatewayNote') : t('actionPreviewNote'))
          : null,
        failed && repair.message !== undefined
          ? h('p', { className: 'tmx-errorText', key: 'message' }, `${t('hostError')}${t('colon')} ${describeError(repair.message, t)}`)
          : null,
        repair.phase === 'previewing' || repair.phase === 'executing'
          // The host has not answered yet, so no result detail is claimed.
          ? null
          : lines.length === 0
            ? h('p', { className: 'tmx-note', key: 'none' }, t('actionNoDetails'))
            : h('dl', { className: 'tmx-connectLines', key: 'lines' }, lines.map(group =>
              h('div', { className: 'tmx-connectGroup', key: group.label }, [
                h('dt', { key: 'label' }, group.label),
                h('dd', { key: 'items' }, h('ul', { className: 'tmx-inlineList' },
                  group.items.map((item, index) => h('li', { className: 'tmx-break', key: index }, item)))),
              ]))),
        h('div', { className: 'tmx-panelActions', key: 'actions' }, actions),
      ])
    }

    function nextActionPanel(action, t, handlers) {
      let text
      switch (action.kind) {
        case 'connect': text = t('nextConnect'); break
        case 'reload': text = t('nextReload'); break
        case 'conflict': text = t('nextConflict'); break
        case 'gateway': text = t('nextGateway'); break
        case 'blocker': text = t('nextBlocker'); break
        case 'config': text = action.reason === 'unavailable' ? t('nextConfigUnavailable') : t('nextConfig'); break
        case 'retry':
          text = action.reason === 'projection'
            ? t('nextProjectionRetry')
            : action.reason === 'section' ? t('nextSectionRetry') : t('nextRetry')
          break
        case 'mode': text = t('nextModeDisabled'); break
        default: text = t('unknown')
      }
      const code = action.kind === 'blocker' || action.kind === 'conflict'
        ? hostCodeText(action.code, action.message)
        : action.kind === 'gateway' && typeof action.code === 'string' ? boundedText(action.code, 48) : ''
      const button = action.kind === 'connect'
        ? h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'connect',
          'data-kind': 'primary',
          onClick: handlers.connect,
          disabled: handlers.disabled,
        }, t('connect'))
        : action.kind === 'retry'
          ? h('button', {
            type: 'button',
            className: 'tmx-button',
            key: 'retry',
            onClick: handlers.retry,
          }, t('retry'))
          : null
      const pendingKind = action.kind === 'connect' || action.kind === 'reload'
        || action.kind === 'retry' || action.kind === 'mode'
      return h('div', { className: 'tmx-next', key: 'next' }, [
        h('p', { className: 'tmx-nextHead', key: 'head' }, [
          dot(pendingKind ? 'warn' : 'error'),
          t('nextActionTitle'),
        ]),
        h('p', { className: 'tmx-nextText', key: 'text' }, text),
        code === '' ? null : h('p', { className: 'tmx-nextCode', key: 'code' }, code),
        button === null ? null : h('div', { className: 'tmx-nextActions', key: 'actions' }, button),
      ])
    }

    function announce({ load, observe, connect, repair, verdict }, t) {
      if (load.schemaUnsupported === true) return t('stateSchemaUnsupported')
      if (observe.phase === 'failed') {
        return observe.message === undefined
          ? t('observeFailed')
          : `${t('observeFailed')}${t('colon')} ${describeError(observe.message, t)}`
      }
      if (observe.phase === 'running') return t('observeRunning')
      if (connect.phase === 'failed') {
        return connect.message === undefined
          ? t('connectFailed')
          : `${t('connectFailed')}${t('colon')} ${describeError(connect.message, t)}`
      }
      if (repair !== null && repair !== undefined && repair.phase === 'failed') {
        return repair.message === undefined
          ? t('actionFailed')
          : `${t('actionFailed')}${t('colon')} ${describeError(repair.message, t)}`
      }
      if (load.phase === 'failed' && load.message !== undefined) {
        return `${t('hostError')}${t('colon')} ${describeError(load.message, t)}`
      }
      return verdict
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


    // ------------------------------------------------------- Phase 3 seat data
    //
    // The DSH-native seats (sidebar marker, Settings > Models card, model-menu
    // option and pane, tool result card) read the same host read model as the
    // page. They share one short-lived GET and never post, never observe, and
    // never run an action on mount.

    /** How long one cached read model serves the seats before an explicit reload. */
    const SNAPSHOT_TTL_MS = 10_000
    /** Latest shared seats read: `{ at, result?, pending? }`. */
    let snapshotCache

    /** Record a read model the page or a seat just adopted, so the seats agree. */
    function rememberState(state) {
      if (state === null || typeof state !== 'object' || state.schemaVersion !== 1 || state.ok !== true) return
      snapshotCache = { at: Date.now(), result: { state } }
    }

    /**
     * One cached GET /_tmax/state for every seat. Resolves to a usable read
     * model or the reason there is none; it never posts and never observes.
     */
    function readStateSnapshot(force) {
      const now = Date.now()
      if (force !== true && snapshotCache !== undefined) {
        if (snapshotCache.result !== undefined && now - snapshotCache.at < SNAPSHOT_TTL_MS) {
          return Promise.resolve(snapshotCache.result)
        }
        if (snapshotCache.pending !== undefined) return snapshotCache.pending
      }
      const pending = (async () => {
        try {
          const response = await fetch(new URL('/_tmax/state', document.baseURI), { credentials: 'same-origin' })
          const value = await readBody(response)
          if (!response.ok) return { message: stateError(value, response.status) }
          if (value === null || typeof value !== 'object') return { message: `HTTP ${response.status}` }
          // The same schema gate the page applies: an unknown version is a
          // failure, never a payload a seat guesses at.
          if (value.schemaVersion !== 1) return { schemaUnsupported: true, message: String(value.schemaVersion) }
          if (value.ok !== true) return { message: stateError(value, response.status) }
          return { state: value }
        } catch (error) {
          return { message: plainError(error) }
        }
      })().then(result => {
        snapshotCache = { at: Date.now(), result }
        return result
      })
      snapshotCache = { at: now, pending }
      return pending
    }

    function seatLoadOf(result) {
      if (result !== null && typeof result === 'object' && result.state !== undefined) return { phase: 'ready', state: result.state }
      return { phase: 'failed', schemaUnsupported: result?.schemaUnsupported === true, message: result?.message }
    }

    /** One seat's read model: a GET on mount, an explicit reload, and adoption. */
    function useSeatSnapshot(t) {
      const [load, setLoad] = React.useState({ phase: 'loading' })
      // Adoption goes through the page's schema/ok gate: a seat may not render
      // an unsupported or refused payload as ready evidence.
      const adopt = React.useCallback((state, response) => {
        const gate = stateGate(state, response)
        if (gate.object !== true) return false
        if (gate.state !== undefined) {
          rememberState(gate.state)
          setLoad({ phase: 'ready', state: gate.state })
          return true
        }
        if (gate.schemaUnsupported === true) {
          setLoad({ phase: 'failed', schemaUnsupported: true, message: `${t('schema')} ${gate.message}` })
          return true
        }
        setLoad(previous => ({ phase: 'failed', state: previous.state, message: gate.message }))
        return true
      }, [t])
      const reload = React.useCallback(() => {
        setLoad(previous => ({ phase: 'refreshing', state: previous.state }))
        readStateSnapshot(true).then(result => setLoad(seatLoadOf(result)))
      }, [])
      React.useEffect(() => {
        let live = true
        readStateSnapshot(false).then(result => { if (live) setLoad(seatLoadOf(result)) })
        return () => { live = false }
      }, [])
      return [load, { reload, adopt }]
    }

    /**
     * The one navigation marker's severity. Error is a reported failure or
     * blocker; warn is a route that cannot serve yet; ok is an active, usable
     * route with a ready gateway; unknown is no read model at all.
     */
    function markerStatus(state) {
      if (state === null || typeof state !== 'object' || state.ok !== true) return { tone: 'unknown', key: 'markerUnknown' }
      const routes = Array.isArray(state.routes) ? state.routes : []
      const providers = Array.isArray(state.providers?.tmax) ? state.providers.tmax : []
      const gateway = state.gateway !== null && typeof state.gateway === 'object' ? state.gateway : {}
      const target = state.target !== null && typeof state.target === 'object' ? state.target : {}
      const ownership = state.ownership !== null && typeof state.ownership === 'object' ? state.ownership : {}
      const config = state.config !== null && typeof state.config === 'object' ? state.config : {}
      const readErrors = Array.isArray(state.errors) ? state.errors : []
      // A configuration that cannot be applied is the loudest fact on the row,
      // whatever the other sections appear to say.
      if (config.state === 'invalid') return { tone: 'error', key: 'markerConfigError' }
      if (providers.some(provider => provider?.suspension?.state === 'suspended')) return { tone: 'error', key: 'markerSuspended' }
      if (gateway.ready === false) return { tone: 'error', key: 'markerGatewayDown' }
      if (ownership.conflict !== null && ownership.conflict !== undefined) return { tone: 'error', key: 'markerConflict' }
      const blockers = (Array.isArray(target.blockers) ? target.blockers : [])
        .concat(routes.flatMap(route => Array.isArray(route?.blockers) ? route.blockers : []))
      if (blockers.length > 0) return { tone: 'error', key: 'markerBlocker' }
      // Degraded, not healthy: something the marker depends on is unknown.
      if (config.state === 'unavailable') return { tone: 'warn', key: 'markerConfigUnavailable' }
      if (readErrors.length > 0) return { tone: 'warn', key: 'markerSectionError' }
      if (routes.length === 0) return { tone: 'warn', key: 'markerNoRoute' }
      if (gateway.ready !== true) return { tone: 'warn', key: 'markerGatewayUnknown' }
      const active = routes.filter(route => route?.active === true)
      if (active.length === 0) return { tone: 'warn', key: 'markerInactive' }
      // A credential known to be missing is a warning; readiness tmax did not
      // report is a different warning. Unknown is never drawn as missing.
      // A credential known to be missing is a warning; readiness tmax did not
      // report is a different warning. Unknown is never drawn as missing.
      if (!active.some(route => route?.credentialConfigured === true)) {
        if (active.some(route => route?.credentialConfigured === false)) return { tone: 'warn', key: 'markerCredential' }
        return { tone: 'warn', key: 'markerCredentialUnknown' }
      }
      return { tone: 'ok', key: 'markerReady' }
    }

    /** Why Settings > Models does or does not list tmax models. */
    function modelsAvailability(state) {
      if (state === null || typeof state !== 'object' || state.ok !== true) return 'unknown'
      // A failed projection or any failed section read leaves the model list
      // unknown: it must never read as "no models".
      if (projectionFailed(state)) return 'unknown'
      if (sectionErrors(state).length > 0) return 'unknown'
      const ownership = state.ownership !== null && typeof state.ownership === 'object' ? state.ownership : {}
      if (pendingReload(ownership)) return 'reload'
      // A state without a route list has nothing to count; it is not "no models".
      if (!Array.isArray(state.routes)) return 'unknown'
      const routes = state.routes
      if (routes.length === 0) return 'none'
      const active = routes.filter(route => route?.active === true)
      if (active.length === 0) return 'inactive'
      // Only a credential tmax reports as missing explains absent models; a
      // readiness it never reported leaves the list unknown, not "not usable".
      if (!active.some(route => route?.credentialConfigured === true)) {
        return active.some(route => route?.credentialConfigured === false) ? 'credential' : 'credentialUnknown'
      }
      return 'available'
    }

    /** The tmax route model a session selection names, or undefined when it is not tmax-provided. */
    function tmaxSelection(state, selection) {
      if (state === null || typeof state !== 'object' || state.ok !== true) return undefined
      if (selection === null || typeof selection !== 'object') return undefined
      const provider = typeof selection.provider === 'string' ? selection.provider : ''
      const modelId = typeof selection.model === 'string' ? selection.model : ''
      if (provider === '' || modelId === '') return undefined
      const routes = Array.isArray(state.routes) ? state.routes : []
      for (const route of routes) {
        if (route?.provider !== provider) continue
        const models = Array.isArray(route.models) ? route.models : []
        const model = models.find(item => item !== null && typeof item === 'object' && item.id === modelId)
        if (model !== undefined) return { route, model }
      }
      return undefined
    }

    /**
     * The tmax_status result as data, when its text blocks carry the JSON the
     * host renders. A parse failure keeps its bounded text and is never success.
     */
    function toolStatusSummary(content) {
      if (!Array.isArray(content)) return undefined
      const parts = []
      for (const block of content) {
        if (block !== null && typeof block === 'object' && block.type === 'text' && typeof block.text === 'string') parts.push(block.text)
      }
      const text = parts.join('\n')
      if (text.trim() === '') return undefined
      let value
      try {
        value = JSON.parse(text)
      } catch {
        return { parsed: false, text: boundedText(text, 400) }
      }
      if (value === null || typeof value !== 'object' || Array.isArray(value)) return { parsed: false, text: boundedText(text, 400) }
      const target = value.target !== null && typeof value.target === 'object' ? value.target : {}
      const gateway = value.gateway !== null && typeof value.gateway === 'object' ? value.gateway : {}
      const routes = Array.isArray(value.routes) ? value.routes : undefined
      const providers = value.providers !== null && typeof value.providers === 'object' ? value.providers : {}
      const blockers = Array.isArray(value.blockers) ? value.blockers : undefined
      return {
        parsed: true,
        ok: value.ok === true,
        targetId: typeof target.id === 'string' && target.id !== '' ? boundedText(target.id, 80) : undefined,
        targetPath: typeof target.path === 'string' && target.path !== '' ? boundedText(target.path, 160) : undefined,
        gatewayReady: gateway.ready === true ? true : gateway.ready === false ? false : undefined,
        // A missing list is unknown; only a reported empty list is a zero count.
        routeCount: routes === undefined ? undefined : routes.length,
        routeNames: routes === undefined ? [] : routes.slice(0, 6)
          .map(route => boundedText(route?.provider ?? route?.name, 60))
          .filter(name => name !== ''),
        modelCount: routes === undefined
          ? undefined
          : routes.reduce((count, route) => count + (Array.isArray(route?.models) ? route.models.length : 0), 0),
        providerCount: Array.isArray(providers.tmax) ? providers.tmax.length : undefined,
        agentProviderCount: Array.isArray(providers.agent) ? providers.agent.length : undefined,
        blockerCount: blockers === undefined ? undefined : blockers.length,
        blockers: blockers === undefined
          ? []
          : blockers.slice(0, 4)
            .map(blocker => hostCodeText(blocker?.code, blocker?.message))
            .filter(text => text !== ''),
      }
    }

    /**
     * Ask the layout for the tmax page. A composition without the layout service
     * or without that panel leaves the control a no-op, never an error.
     */
    function layoutService(context) {
      try {
        const layout = typeof context?.get === 'function' ? context.get('layout') : undefined
        return layout !== null && typeof layout === 'object' ? layout : undefined
      } catch {
        return undefined
      }
    }

    function hasTmaxPage(context) {
      const layout = layoutService(context)
      return layout !== undefined && typeof layout.selectPanel === 'function'
    }

    function openTmaxPage(context) {
      const layout = layoutService(context)
      if (layout === undefined || typeof layout.selectPanel !== 'function') return false
      try {
        layout.selectPanel('tmax')
        return true
      } catch {
        // The layout throws when this plugin's main key is not registered in the
        // composition; a click reports "unavailable" instead of escaping.
        return false
      }
    }

    // --------------------------------------------------- Phase 3 seat components

    /** The tmax sidebar row's marker: a dot, plus text that describes the row. */
    function SidebarTrailingMarker(props) {
      const t = props.t
      const [load] = useSeatSnapshot(t)
      const marker = markerStatus(load.phase === 'ready' ? load.state : undefined)
      return h('span', {
        className: 'tmx-navMark',
        'data-tone': marker.tone,
        'data-wide': props.wide === true ? 'true' : 'false',
        title: `${t('title')}${t('colon')} ${t(marker.key)}`,
      }, [
        h('span', { className: 'tmx-dot', key: 'dot', 'data-tone': marker.tone, 'aria-hidden': 'true' }),
        h('span', { className: 'tmx-sr', key: 'text' }, t(marker.key)),
      ])
    }

    /** Settings > Models: whether tmax models are usable, why not, and the one control that helps. */
    function ModelsFooter(props) {
      const t = props.t
      const [load, snapshot] = useSeatSnapshot(t)
      const [connect, setConnect] = React.useState({ phase: 'idle', lines: [] })
      const [openFailed, setOpenFailed] = React.useState(false)
      const state = load.phase === 'ready' ? load.state : undefined
      // Only a usable read model supplies evidence. A missing or failed read
      // renders unknown, never a zero count it did not observe.
      const read = state !== undefined && state.ok === true ? state : undefined
      const availability = modelsAvailability(read)
      // A readable state whose projection/section read failed has no route or
      // model evidence: render unknown counts, never a fabricated zero.
      const evidenceMissing = read === undefined || projectionFailed(read) || sectionErrors(read).length > 0
      const routes = evidenceMissing || !Array.isArray(read.routes) ? undefined : read.routes
      const activeCount = routes === undefined ? undefined : routes.filter(route => route?.active === true).length
      // Usable needs both liveness and a credential tmax reports as configured.
      // A route whose readiness was never reported is unknown, not unusable: its
      // count would be a claim the host did not make.
      const credentialKnown = routes !== undefined
        && routes.every(route => route?.active !== true || typeof route?.credentialConfigured === 'boolean')
      const usableCount = routes === undefined || !credentialKnown
        ? undefined
        : routes.filter(route => route?.active === true && route?.credentialConfigured === true).length
      const modelCount = routes === undefined ? undefined : modelCatalogOf(read).length
      const gateway = read?.gateway !== null && typeof read?.gateway === 'object' ? read.gateway : undefined
      // A failed gateway section is not a gateway that reported unknown: the
      // fact says the read itself failed and draws no readiness claim.
      const gatewayReadFailed = read !== undefined && errorsForSection(read, 'gateway').length > 0
      const gatewayText = gatewayReadFailed || gateway === undefined
        ? t('unknown')
        : gateway.ready === true ? t('gatewayReady') : gateway.ready === false ? t('gatewayNotReady') : t('unknown')
      // Apply is offered only when the page's own next action is Apply: the
      // same predicate that keeps a broken config, a failed read, a pending
      // reload, or an inert owner ahead of a native write.
      const pageAction = read === undefined ? undefined : nextAction({ phase: 'ready', value: read })
      const connectable = pageAction !== undefined && pageAction.kind === 'connect'
      const reloadable = load.phase === 'failed' || (pageAction !== undefined && pageAction.kind === 'retry')

      async function previewConnect() {
        setConnect({ phase: 'previewing', lines: [] })
        try {
          const { response, value } = await postJson('/_tmax/connect', { dryRun: true })
          if (!response.ok || value?.ok !== true) {
            setConnect({ phase: 'failed', lines: connectLines(value?.connect, t), message: stateError(value, response.status) })
            return
          }
          setConnect({ phase: 'preview', lines: connectLines(value.connect, t) })
        } catch (error) {
          setConnect({ phase: 'failed', lines: [], message: plainError(error) })
        }
      }

      async function executeConnect() {
        setConnect(previous => ({ ...previous, phase: 'executing' }))
        try {
          const { response, value } = await postJson('/_tmax/connect', { dryRun: false })
          if (!response.ok || value?.ok !== true) {
            setConnect({ phase: 'failed', lines: connectLines(value?.connect, t), message: stateError(value, response.status) })
            return
          }
          setConnect({ phase: 'applied', lines: connectLines(value.connect, t) })
          // A returned read model is adopted without a second GET, through the
          // same schema/ok gate as the page: a refused or unsupported payload
          // is a visible failed read, never adopted evidence.
          const nextState = value?.state !== null && typeof value?.state === 'object' ? value.state : undefined
          if (nextState !== undefined) snapshot.adopt(nextState, response)
          else snapshot.reload()
        } catch (error) {
          setConnect({ phase: 'failed', lines: [], message: plainError(error) })
        }
      }

      const reasonKey = availability === 'reload'
        ? 'modelsReload'
        : availability === 'none'
          ? 'modelsNone'
          : availability === 'inactive'
            ? 'modelsInactive'
            : availability === 'credential'
              ? 'modelsCredential'
              : availability === 'credentialUnknown'
                ? 'modelsCredentialUnknown'
                : availability === 'available' ? 'modelsAvailable' : 'modelsUnknown'
      const facts = [`${t('footerGateway')}${t('colon')} ${gatewayText}${gatewayReadFailed ? ` · ${t('footerReadFailed')}` : ''}`]
      if (routes === undefined || activeCount === undefined) {
        facts.push(`${t('footerRoutes')}${t('colon')} ${t('unknown')}`)
      } else {
        facts.push(`${t('footerRoutes')}${t('colon')} ${activeCount}/${routes.length} ${t('footerActive')} · ${usableCount === undefined ? t('unknown') : usableCount} ${t('footerUsable')}`)
      }
      facts.push(`${t('footerModels')}${t('colon')} ${modelCount === undefined ? t('unknown') : modelCount}`)
      // Seat data is one shared GET; say when the read model was generated so a
      // mounted card cannot look newer than the evidence behind it.
      const generatedAt = read === undefined ? '' : clockOf(read.generatedAt)
      if (generatedAt !== '') facts.push(`${t('updated')} ${generatedAt}`)
      const pageAvailable = typeof props.hasLayout === 'function' ? props.hasLayout() === true : false
      const pageUnavailable = pageAvailable !== true || openFailed
      const actions = []
      if (connectable) {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          'data-kind': 'primary',
          key: 'connect',
          onClick: () => { void previewConnect() },
          disabled: connect.phase === 'previewing' || connect.phase === 'executing',
        }, t('connect')))
      }
      if (reloadable) {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'reload',
          onClick: () => snapshot.reload(),
          disabled: load.phase === 'refreshing',
        }, t('footerReload')))
      }
      actions.push(h('button', {
        type: 'button',
        className: 'tmx-button',
        key: 'open',
        title: pageUnavailable ? t('footerPageUnavailable') : undefined,
        disabled: pageUnavailable,
        onClick: () => {
          if (typeof props.openPage !== 'function') return
          // A composition can expose the layout service while this plugin's
          // page key is not registered; a click must not throw out of the card.
          if (props.openPage() === false) setOpenFailed(true)
        },
      }, t('footerOpenPage')))
      const body = [
        h('h3', { className: 'tmx-seatHead', key: 'head' }, [
          t('footerTitle'),
          availability === 'available' ? chip(t('modelsAvailable'), 'ok') : null,
        ]),
        h('p', { className: 'tmx-seatFacts', key: 'facts' }, facts.join(' · ')),
      ]
      if (load.phase === 'loading' || load.phase === 'refreshing') {
        body.push(h('p', { className: 'tmx-note', key: 'loading' }, t('footerLoading')))
      } else if (load.phase === 'failed') {
        body.push(h('p', { className: 'tmx-errorText', key: 'error' },
          `${t('hostError')}${t('colon')} ${describeError(load.message, t)}`))
      } else {
        body.push(h('p', { className: 'tmx-note', key: 'reason' }, `${t('footerReason')}${t('colon')} ${t(reasonKey)}`))
      }
      const panel = connectPanel(connect, t, { execute: () => { void executeConnect() }, cancel: () => setConnect({ phase: 'idle', lines: [] }) })
      if (panel !== null) body.push(panel)
      body.push(h('div', { className: 'tmx-seatActions', key: 'actions' }, actions))
      return h('div', { className: 'tmx-seatCard', 'data-tmax-footer': '1', key: 'card' }, body)
    }

    /** The model menu's tmax routing option: shown only when tmax provides the selected model. */
    function TmaxModelOption(props) {
      const t = props.t
      const projection = typeof props.useProjection === 'function' ? props.useProjection('modelSelection') : undefined
      const selection = projection === null || projection === undefined
        ? undefined
        : projection.next ?? projection.lastUsed
      const [load] = useSeatSnapshot(t)
      const match = load.phase === 'ready' ? tmaxSelection(load.state, selection) : undefined
      if (match === undefined) return null
      const modelId = typeof match.model.id === 'string' && match.model.id !== ''
        ? boundedText(match.model.id, 80)
        : boundedText(match.model.name, 80)
      return h('button', {
        type: 'button',
        role: 'menuitem',
        className: 'tmx-modelOption',
        'data-model-menu-pane': 'tmax-routing',
        disabled: props.locked === true,
        onClick: () => {
          if (typeof props.openPane === 'function') props.openPane({ key: 'tmax-routing', title: t('routingPaneTitle') })
        },
      }, [
        h('span', { className: 'tmx-modelOptionLabel', key: 'label' }, t('routingOption')),
        h('span', { className: 'tmx-chipKind', key: 'model' }, modelId),
        h('span', { className: 'tmx-chipKind', key: 'chevron', 'aria-hidden': 'true' }, '›'),
      ])
    }

    /** The routing detail pane: what the selected tmax model is, and what tmax would route to. */
    function TmaxRoutingPane(props) {
      const t = props.t
      const [load, snapshot] = useSeatSnapshot(t)
      const projection = typeof props.useProjection === 'function' ? props.useProjection('modelSelection') : undefined
      const selection = projection === null || projection === undefined
        ? undefined
        : projection.next ?? projection.lastUsed
      const state = load.phase === 'ready' ? load.state : undefined
      const match = state === undefined ? undefined : tmaxSelection(state, selection)
      const routing = state !== undefined && match !== undefined && Array.isArray(state.routing)
        ? state.routing.find(entry => entry !== null && typeof entry === 'object' && entry.model === match.model.id)
        : undefined
      const body = []
      if (load.phase === 'failed') {
        body.push(h('p', { className: 'tmx-errorText', key: 'error' },
          `${t('hostError')}${t('colon')} ${describeError(load.message, t)}`))
      } else if (state === undefined) {
        body.push(h('p', { className: 'tmx-note', key: 'loading' }, t('loading')))
      } else if (match === undefined) {
        body.push(h('p', { className: 'tmx-note', key: 'notTmax' }, t('routingNotTmax')))
      } else {
        const model = match.model
        const candidates = numeric(model.candidateCount)
        body.push(h('dl', { className: 'tmx-facts', key: 'facts' }, [
          fact(t('modelKind'), enumText(model.kind, KIND_STATES, t)),
          fact(t('modelContinuity'), enumText(model.continuity, CONTINUITY_STATES, t)),
          fact(t('modelCandidates'), candidates === undefined ? t('unknown') : String(candidates)),
          fact(t('modelAvailability'), enumText(model.availability, AVAILABILITY_STATES, t)),
        ]))
        body.push(h('dl', { className: 'tmx-facts', key: 'capabilities' },
          capabilityFacts(model.capabilities, t).map(item => fact(item.label, item.value))))
        body.push(routingBlock(routing, t))
      }
      const actions = [
        h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'reload',
          onClick: () => snapshot.reload(),
          disabled: load.phase === 'refreshing',
        }, t('routingReload')),
        typeof props.dismiss === 'function'
          ? h('button', {
            type: 'button',
            className: 'tmx-button',
            key: 'dismiss',
            onClick: () => props.dismiss(),
          }, t('dismiss'))
          : null,
      ]
      return h('div', { className: 'tmx-pane', 'data-tmax-pane': 'tmax-routing', key: 'pane' }, [
        h('h3', { className: 'tmx-paneTitle', key: 'title' }, t('routingPaneTitle')),
        ...body,
        h('div', { className: 'tmx-paneActions', key: 'actions' }, actions.filter(action => action !== null)),
      ])
    }

    /** The tmax_status tool card: the compact facts the tool already returned, never a new read. */
    function TmaxStatusCard(props) {
      const t = props.t
      if (props.phase !== 'result') {
        return h('div', { className: 'tmx-toolCard', 'data-tmax-tool': 'pending', key: 'tool' }, [
          h('p', { className: 'tmx-toolHead', key: 'head' }, t('toolTitle')),
          h('p', { className: 'tmx-note', key: 'note' }, t('toolPending')),
        ])
      }
      const block = props.block !== null && typeof props.block === 'object' ? props.block : {}
      const summary = toolStatusSummary(Array.isArray(block.content) ? block.content : [])
      const failed = block.isError === true || (summary !== undefined && summary.parsed === true && summary.ok !== true)
      const head = h('p', { className: 'tmx-toolHead', key: 'head' }, [
        t('toolTitle'),
        failed ? chip(t('toolFailed'), 'error') : null,
      ])
      if (summary === undefined || summary.parsed !== true) {
        const text = summary !== undefined && typeof summary.text === 'string' ? summary.text : ''
        return h('div', { className: 'tmx-toolCard', 'data-tmax-tool': 'unreadable', key: 'tool' }, [
          head,
          h('p', { className: 'tmx-errorText tmx-break', key: 'text' },
            text === '' ? t('toolUnreadable') : `${t('toolUnreadable')}${t('colon')} ${text}`),
        ])
      }
      // A failed or partial tool result supplies no counts: an unobserved list is
      // unknown, never a zero the model could read as "nothing configured".
      const healthy = summary.ok === true
      const routeCount = healthy ? summary.routeCount : undefined
      const modelCount = healthy ? summary.modelCount : undefined
      const providerCount = healthy ? summary.providerCount : undefined
      const agentProviderCount = healthy ? summary.agentProviderCount : undefined
      const routeText = routeCount === undefined
        ? t('unknown')
        : `${routeCount}${summary.routeNames.length === 0 ? '' : ` (${summary.routeNames.join(', ')})`}`
      const providerText = providerCount === undefined && agentProviderCount === undefined
        ? t('unknown')
        : `${providerCount ?? t('unknown')} tmax · ${agentProviderCount ?? t('unknown')} agent`
      const facts = [
        `${t('toolTarget')}${t('colon')} ${summary.targetId ?? t('unknown')}${summary.targetPath === undefined ? '' : ` (${summary.targetPath})`}`,
        `${t('toolRoutes')}${t('colon')} ${routeText}`,
        `${t('gatewayTitle')}${t('colon')} ${summary.gatewayReady === true ? t('gatewayReady') : summary.gatewayReady === false ? t('gatewayNotReady') : t('unknown')}`,
        `${t('toolProviders')}${t('colon')} ${providerText}`,
        `${t('toolModels')}${t('colon')} ${modelCount === undefined ? t('unknown') : modelCount}`,
      ]
      const body = [head, h('p', { className: 'tmx-seatFacts', key: 'facts' }, facts.join(' · '))]
      if (healthy) {
        if (summary.blockerCount !== undefined && summary.blockerCount > 0) {
          body.push(h('p', { className: 'tmx-errorText tmx-break', key: 'blockers' },
            `${t('toolBlockers')}${t('colon')} ${summary.blockerCount}${summary.blockers.length === 0 ? '' : ` · ${summary.blockers.join('; ')}`}`))
        }
      } else if (summary.blockers.length > 0) {
        body.push(h('p', { className: 'tmx-errorText tmx-break', key: 'blockers' },
          `${t('toolBlockers')}${t('colon')} ${summary.blockerCount ?? summary.blockers.length} · ${summary.blockers.join('; ')}`))
      } else {
        body.push(h('p', { className: 'tmx-note', key: 'blockers' },
          `${t('toolBlockers')}${t('colon')} ${t('unknown')}`))
      }
      return h('div', { className: 'tmx-toolCard', 'data-tmax-tool': 'status', key: 'tool' }, body)
    }

    function apply(context) {
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
      // Phase 3 seats. Each is one registration on a declared slot; `slots.inject`
      // simply waits when the declaring package is not in this composition.
      const seat = (component, extra) => props => h(component, { ...props, ...extra, t: props.t ?? t })
      context.effect(() => context.slots.inject('sidebar.panellist.trailing', () => context.slots.register({
        name: 'sidebar.panellist.trailing',
        id: 'tmax',
        order: 20,
        locale: NS,
      }, seat(SidebarTrailingMarker))), 'tmax: sidebar marker')
      context.effect(() => context.slots.inject('settings.models.footer', () => context.slots.register({
        name: 'settings.models.footer',
        id: 'tmax',
        order: 100,
        locale: NS,
      }, seat(ModelsFooter, {
        openPage: () => openTmaxPage(context),
        hasLayout: () => hasTmaxPage(context),
      }))), 'tmax: settings footer')
      context.effect(() => context.slots.inject('conversation.input.model.options', () => context.slots.register({
        name: 'conversation.input.model.options',
        id: 'tmax-routing',
        order: 100,
        locale: NS,
      }, seat(TmaxModelOption))), 'tmax: model option')
      context.effect(() => context.slots.inject('conversation.input.model.pane', () => context.slots.register({
        name: 'conversation.input.model.pane',
        key: 'tmax-routing',
        locale: NS,
      }, seat(TmaxRoutingPane))), 'tmax: routing pane')
      context.effect(() => context.slots.inject('tool.call.toolview', () => context.slots.register({
        name: 'tool.call.toolview',
        key: 'tmax_status',
        locale: NS,
      }, seat(TmaxStatusCard))), 'tmax: status tool card')
    }

    // The DSH loader ignores extra exports; tests use these pure helpers without
    // a DOM so the page's truth rules are asserted directly.
    const __pure = {
      boundedText,
      hostCodeText,
      enumText,
      numeric,
      shortRevision,
      clockOf,
      formatDate,
      compact,
      windowReading,
      disambiguate,
      toneFor,
      oldestReading,
      routeReadiness,
      routeActiveState,
      stateGate,
      sectionErrors,
      errorsForSection,
      projectionFailed,
      projectionErrors,
      sectionErrorText,
      ownershipMode,
      pendingReload,
      limitMessage,
      billingNoteTexts,
      resetQualified,
      resetUnqualified,
      resetAt,
      routeBlockerTexts,
      usableRoutes,
      modelCatalogOf,
      verdict,
      nextAction,
      defaultsMatch,
      safeDashboard,
      capabilityFacts,
      limitState,
      historyFact,
      lastErrorFact,
      connectLines,
      observeItems,
      failureIsActive,
      providerActions,
      gatewayActions,
      managementAction,
      actionPayload,
      actionSucceeded,
      actionResultLines,
      markerStatus,
      modelsAvailability,
      tmaxSelection,
      toolStatusSummary,
      historyCoverageLines,
      historyRows,
      historyRemaining,
      historyFailure,
      historyTone,
      locales: { en, zh },
    }

    return { inject: ['slots', 'locale'], apply, __pure }
  },
})
