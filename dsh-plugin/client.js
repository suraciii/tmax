// Client half: tmax's own page in DSH, reached from the sidebar panel list.
//
// The page is a first-level entry beside Plugins and Schedules, so it owns a
// full content column instead of competing for room in a settings footer. It
// reads one host read model (`GET /_tmax/state`); it never runs a CLI itself,
// never observes a provider on mount or on visibility, and never receives a
// key. A deliberate Refresh posts `/_tmax/observe`.
//
// Four sections answer four questions: 模型 lists the catalog with one inline
// Harness control per model, Harness lists one row per declared tmax target,
// 网关 shows the gateway, and 额度 shows quota windows. A Harness control saves
// desired configuration directly (`add_model` / `remove_model`) and never
// applies it; only an explicit Harness Apply previews one target with
// `apply_target { dryRun: true }` before it writes, confirms, and adopts the
// returned state. Every action is one exact operation with a typed result, and
// a failure is never drawn as success.
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
      colon: ':',
      unknown: 'unknown',
      none: 'none',
      notReported: 'not reported',
      loading: 'Reading tmax state…',
      checking: 'Checking…',
      refresh: 'Refresh',
      refreshing: 'Refreshing…',
      retry: 'Retry',
      close: 'Close',
      yes: 'yes',
      no: 'no',
      updated: 'updated',
      schema: 'schema',
      // The state contract this page consumes.
      stateSchemaUnsupported: 'Unsupported state schema',
      // Status and target evidence.
      target: 'Target',
      ownerSettings: 'applied settings',
      ownerUnapplied: 'not applied yet',
      ownerNone: 'none',
      scopeProfileScoped: 'profile-scoped',
      conflict: 'Conflict',
      applied: 'Applied',
      routeLive: 'live',
      routeNotApplied: 'not applied',
      routeBlockers: 'Route blockers',
      // Gateway detail.
      gatewayTitle: 'Gateway',
      gatewayReady: 'Ready',
      gatewayNotReady: 'Not ready',
      gatewayListen: 'Address',
      gatewayDaemon: 'Daemon',
      gatewayReason: 'reason',
      daemonIdentified: 'identified',
      daemonUnidentified: 'unidentified',
      supervisionAvailable: 'available',
      supervisionUnavailable: 'unavailable',
      keyLoaded: 'loaded',
      keyAbsent: 'absent',
      registrationRegistered: 'registered',
      registrationUnavailable: 'unavailable',
      // Config.
      configValid: 'valid',
      configInvalid: 'invalid',
      configUnavailable: 'unavailable',
      // Models and routing.
      accountsTitle: 'Accounts',
      noAccounts: 'No account reports a quota window yet.',
      providerFailing: 'failing',
      kindAlias: 'alias',
      kindConcrete: 'concrete',
      continuityStateless: 'stateless',
      continuityStateful: 'stateful',
      availabilityUnverified: 'unverified',
      availabilityVerified: 'verified',
      // Providers and limits.
      providerDisabled: 'disabled',
      providerSuspended: 'suspended',
      blocker: 'Blocker',
      billingPlanAllowance: 'plan allowance',
      billingUnknown: 'unknown',
      freshnessFresh: 'fresh',
      freshnessStale: 'stale',
      freshnessMissing: 'missing',
      freshnessUnqualified: 'unqualified',
      failureActive: 'active',
      usedShare: 'used',
      reset: 'resets',
      resetApprox: '~',
      resetApproxNote: 'interpreted as Unix milliseconds; unconfirmed',
      today: 'today',
      tomorrow: 'tomorrow',
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
      noteCountersUnitUnqualified: 'counter units unqualified',
      noteUsageMayOverlap: 'usage and limits may overlap',
      noteStatusEnumsUnqualified: 'status enums unqualified',
      notePercentFloored: 'consumed percent floored',
      noteCreditNotTokens: 'credit is not tokens',
      noteFieldsUnqualified: 'fields unqualified',
      models: 'models',
      // Diagnostics.
      historyAvailable: 'available',
      historyUnavailable: 'unavailable',
      reasonDaemonIdentity: 'daemon identity unavailable',
      reasonServiceManager: 'service manager unavailable',
      noLastError: 'none reported',
      // Observation results.
      observeRunning: 'Observing provider sources…',
      observeFailed: 'Observation failed',
      connect: 'Apply',
      connectEffects: 'effects',
      connectBlockers: 'blockers',
      connectNextActions: 'next actions',
      // Explicit repair actions. Each button names one fixed tmax operation.
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
      // Seats: one marker, one settings card, one tool card.
      // Page tabs, catalog search, provider detail, and target-selection add.
      tabsLabel: 'Page sections',
      tabModels: 'Models',
      // The Harness: one row per declared tmax target, and the inline selection
      // control beside each catalog model. A status is target-level only.
      tabHarness: 'Harness',
      harnessNone: 'No Harness is declared.',
      harnessPending: 'To apply',
      harnessApplied: 'Applied',
      harnessUnknown: 'Unknown',
      harnessReloadPending: 'Reload pending',
      harnessApply: 'Apply',
      harnessReapply: 'Reapply',
      harnessApplyTo: 'Apply to',
      harnessAddModel: 'Add model',
      harnessDefault: 'default',
      harnessScope: 'this Harness only',
      harnessApplyNote: 'Overwrites tmax-managed configuration; reopen the Harness after applying.',
      harnessWritten: 'Written',
      harnessReopenPrefix: 'reopen',
      harnessReopenSuffix: 'to use it',
      harnessPickerSearch: 'Search Harness',
      harnessPickerConfigured: 'Configured',
      harnessCountUnit: 'Harnesses',
      harnessCountOne: 'Harness',
      harnessPickerDone: 'Done',
      selectHarness: 'Choose a Harness',
      pendingHarnesses: 'Harnesses to apply',
      pendingHarnessOne: 'Harness to apply',
      goApply: 'Review',
      searchModelsPlaceholder: 'Search models or providers',
      fileOne: 'file',
      fileUnit: 'files',
      configProblem: 'Configuration problem',
      selectionSaved: 'Harness selection saved',
      resultTitle: 'Result',
      actionAddModel: 'Add model',
      actionRemoveModel: 'Remove model',
      actionApplyHarness: 'Apply Harness',
      resultDetails: 'Details',
      actionResult: 'Completed',
      resultDismiss: 'Dismiss result',
      selectionFailed: 'Harness selection failed',
      applyPreviewTitle: 'Apply preview',
      applyPreviewing: 'Checking the write plan…',
      applyApplying: 'Applying…',
      applyConfirm: 'Confirm apply',
      applyCancel: 'Cancel',
      applyApplied: 'Applied; tmax state refreshed.',
      applyFailed: 'Apply failed',
      modelOne: 'model',
      modelUnit: 'models',
      noProviders: 'No providers are configured.',
      tabGateway: 'Gateway',
      tabQuota: 'Quota',
      clearSearch: 'Clear search',
      searchNoMatch: 'No matching models.',
      gatewayKey: 'Key',
      gatewayConfig: 'configuration',
      gatewayConfigured: 'configured',
      gatewayUnconfigured: 'not configured',
      catalogUnavailable: 'The model catalog is unavailable. Fix the failed provider or model read and refresh.',
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
      modelsUnknown: 'The tmax state could not be read.',
      footerOpenPage: 'Open tmax',
      footerPageUnavailable: 'The tmax page is not available in this composition.',
      footerReload: 'Reload',
      footerLoading: 'Reading tmax state…',
      footerHarness: 'DSH Harness',
      footerUnapplied: 'unapplied',
      footerOpenHint: 'Apply is in the tmax Harness tab',
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
      // Failures.
      readFailed: 'Read failed',
      hostError: 'Host error',
      hostStatus: 'The tmax host answered',
      hostUnreachable: 'Cannot reach the tmax host',
      staleNotice: 'showing the last successful read',
      modelsCredential: 'The tmax route has no usable credential reference.',
      modelsCredentialUnknown: 'The tmax route credential readiness is unknown.',
      modelsInactive: 'The tmax route is configured but not active in DSH.',
      modelsNone: 'No tmax model is projected for this profile.',
      modelsReload: 'A DSH reload is pending before the tmax route can be used.',
      effectLocksCreated: 'locks created',
      effectNativeWrites: 'file writes',
      effectProcessesStarted: 'processes started',
      effectSecretReads: 'secret reads',
      observeDone: 'Observation finished',
    }
    const zh = {
      // Shared chrome.
      panel: 'tmax',
      title: 'tmax',
      colon: '：',
      unknown: '未知',
      none: '无',
      notReported: '未报告',
      loading: '正在读取 tmax 状态…',
      checking: '检查中…',
      refresh: '刷新',
      refreshing: '刷新中…',
      retry: '重试',
      close: '关闭',
      yes: '是',
      no: '否',
      updated: '更新于',
      schema: 'schema',
      // 本页消费的状态契约。
      stateSchemaUnsupported: '状态 schema 不受支持',
      // 状态与目标证据。
      target: '目标',
      ownerSettings: '已应用的设置',
      ownerUnapplied: '尚未应用',
      ownerNone: '无',
      scopeProfileScoped: '仅当前 profile',
      conflict: '冲突',
      applied: '已应用',
      routeLive: '已生效',
      routeNotApplied: '未应用',
      routeBlockers: '路由阻塞项',
      // Gateway detail.
      gatewayTitle: '网关',
      gatewayReady: '就绪',
      gatewayNotReady: '未就绪',
      gatewayListen: '监听地址',
      gatewayDaemon: '守护进程',
      gatewayReason: '原因',
      daemonIdentified: '已识别',
      daemonUnidentified: '未识别',
      supervisionAvailable: '可用',
      supervisionUnavailable: '不可用',
      keyLoaded: '已加载',
      keyAbsent: '未加载',
      registrationRegistered: '已注册',
      registrationUnavailable: '不可用',
      // Config.
      configValid: '有效',
      configInvalid: '无效',
      configUnavailable: '不可用',
      // Models and routing.
      accountsTitle: '账号额度',
      noAccounts: '还没有账号上报额度窗口。',
      providerFailing: '故障',
      kindAlias: '别名',
      kindConcrete: '具体模型',
      continuityStateless: '无状态',
      continuityStateful: '有状态',
      availabilityUnverified: '未验证',
      availabilityVerified: '已验证',
      // Providers and limits.
      providerDisabled: '已停用',
      providerSuspended: '已暂停',
      blocker: '阻塞项',
      billingPlanAllowance: '套餐额度',
      billingUnknown: '未知',
      freshnessFresh: '新鲜',
      freshnessStale: '过期',
      freshnessMissing: '缺失',
      freshnessUnqualified: '未定级',
      failureActive: '生效中',
      usedShare: '已用',
      reset: '重置',
      resetApprox: '约',
      resetApproxNote: '按 Unix 毫秒解释，含义待确认',
      today: '今天',
      tomorrow: '明天',
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
      noteCountersUnitUnqualified: '计数单位未定级',
      noteUsageMayOverlap: '用量与额度可能重叠',
      noteStatusEnumsUnqualified: '状态枚举未定级',
      notePercentFloored: '已用百分比已取整',
      noteCreditNotTokens: '额度不是 token 计量',
      noteFieldsUnqualified: '字段未定级',
      models: '模型',
      // Diagnostics.
      historyAvailable: '可用',
      historyUnavailable: '不可用',
      reasonDaemonIdentity: '守护进程身份不可用',
      reasonServiceManager: '服务管理器不可用',
      noLastError: '未报告',
      // Observation results.
      observeRunning: '正在观测供应商来源…',
      observeFailed: '观测失败',
      connect: '应用',
      connectEffects: '影响',
      connectBlockers: '阻塞项',
      connectNextActions: '下一步',
      // 显式修复操作：每个按钮对应一项固定的 tmax 操作。
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
      // Page tabs, catalog search, provider detail, and target-selection add.
      tabsLabel: '页面分区',
      tabModels: '模型',
      // Harness：每个已声明 tmax 目标一行，以及每个目录模型旁的行内选择控件。
      // 状态只到目标一级，不为单个模型发明已应用状态。
      tabHarness: 'Harness',
      harnessNone: '尚未声明任何 Harness。',
      harnessPending: '待应用',
      harnessApplied: '已应用',
      harnessUnknown: '未知',
      harnessReloadPending: '待重载',
      harnessApply: '应用',
      harnessReapply: '重新应用',
      harnessApplyTo: '应用到',
      harnessAddModel: '添加模型',
      harnessDefault: '默认',
      harnessScope: '仅此 Harness',
      harnessApplyNote: '会覆盖 tmax 管理的配置；完成后重新打开该 Harness 生效。',
      harnessWritten: '已写入',
      harnessReopenPrefix: '重新打开',
      harnessReopenSuffix: '后生效',
      harnessPickerSearch: '搜索 Harness',
      harnessPickerConfigured: '已配置',
      harnessCountUnit: '个 Harness',
      harnessCountOne: '个 Harness',
      harnessPickerDone: '完成',
      selectHarness: '选择 Harness',
      pendingHarnesses: '个 Harness 待应用',
      pendingHarnessOne: '个 Harness 待应用',
      goApply: '去应用',
      searchModelsPlaceholder: '搜索模型或供应商',
      fileOne: '个文件',
      fileUnit: '个文件',
      configProblem: '配置问题',
      selectionSaved: 'Harness 配置已保存',
      resultTitle: '操作结果',
      actionAddModel: '添加模型',
      actionRemoveModel: '移除模型',
      actionApplyHarness: '应用 Harness',
      resultDetails: '详情',
      actionResult: '已完成',
      resultDismiss: '关闭结果',
      selectionFailed: 'Harness 配置更新失败',
      applyPreviewTitle: '应用预览',
      applyPreviewing: '正在核对写入计划…',
      applyApplying: '正在应用…',
      applyConfirm: '确认应用',
      applyCancel: '取消',
      applyApplied: '已应用；tmax 状态已刷新。',
      applyFailed: '应用失败',
      modelOne: '个模型',
      modelUnit: '个模型',
      noProviders: '没有配置供应商。',
      tabGateway: '网关',
      tabQuota: '额度',
      clearSearch: '清除搜索',
      searchNoMatch: '没有匹配的模型。',
      gatewayKey: '密钥',
      gatewayConfig: '配置',
      gatewayConfigured: '已配置',
      gatewayUnconfigured: '未配置',
      catalogUnavailable: '模型目录不可用；先修复供应商或模型读取失败并刷新。',
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
      modelsUnknown: '无法读取 tmax 状态。',
      footerOpenPage: '打开 tmax',
      footerPageUnavailable: '当前组合中不可打开 tmax 页面。',
      footerReload: '重新读取',
      footerLoading: '正在读取 tmax 状态…',
      footerHarness: 'DSH Harness',
      footerUnapplied: '待应用',
      footerOpenHint: '在 tmax 的 Harness 页应用',
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
      // Failures.
      readFailed: '读取失败',
      hostError: 'Host 错误',
      hostStatus: 'tmax Host 返回',
      hostUnreachable: '无法连接到 tmax Host',
      staleNotice: '显示上次成功读取的结果',
      modelsCredential: 'tmax 路由没有可用的凭据引用。',
      modelsCredentialUnknown: '无法确定 tmax 路由的凭据就绪状态。',
      modelsInactive: 'tmax 路由已配置，但在 DSH 中未生效。',
      modelsNone: '此配置未投影任何 tmax 模型。',
      modelsReload: '需要重新加载 DSH 后 tmax 路由才能使用。',
      effectLocksCreated: '创建锁',
      effectNativeWrites: '实际配置写入',
      effectProcessesStarted: '启动进程',
      effectSecretReads: '密钥读取',
      observeDone: '观测完成',
    }

    // Enum tables: the host reports machine values; the page translates the
    // ones it knows and shows any other value verbatim (bounded) rather than
    // inventing a word for it.
    const OWNER_STATES = { settings: 'ownerSettings', unapplied: 'ownerUnapplied', none: 'ownerNone' }
    const ROUTE_STATES = { active: 'routeLive', inactive: 'routeNotApplied' }
    const SCOPES = { profile_scoped: 'scopeProfileScoped' }
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
    const REASONS = { daemon_identity_unavailable: 'reasonDaemonIdentity', service_manager_unavailable: 'reasonServiceManager' }
    const NOTE_KEYS = {
      status_enums_unqualified: 'noteStatusEnumsUnqualified',
      percent_consumed_floored: 'notePercentFloored',
      credit_not_tokens: 'noteCreditNotTokens',
      fields_unqualified: 'noteFieldsUnqualified',
      counters_unit_unqualified: 'noteCountersUnitUnqualified',
      usage_and_limits_may_overlap: 'noteUsageMayOverlap',
    }
    // Host section names are its own; the page recognizes the spellings it can
    // route and shows anything else in Diagnostics only.
    const SECTION_ALIASES = {
      projection: ['projection', 'projection_read', 'routes'],
      providers: ['providers', 'provider_list', 'provider_list_read', 'providers_read'],
      target: ['target', 'target_read', 'agent', 'agent_inspect', 'agent_inspect_read'],
      gateway: ['gateway', 'gateway_read', 'gateway_status'],
      config: ['config', 'config_read', 'config_validate'],
    }
    // The host read-model revision this page and its seats consume together.
    const STATE_SCHEMA_VERSION = 5
    // Only an explicit state names the web listener's lifecycle; a null record
    // is not evidence that it is stopped.
    const WEB_STATES = { running: 'webRunning', stopped: 'webStopped' }
    const CONNECT_EFFECTS = [
      ['native_writes', 'effectNativeWrites'],
      ['secret_reads', 'effectSecretReads'],
      ['processes_started', 'effectProcessesStarted'],
      ['locks_created', 'effectLocksCreated'],
    ]
    // Explicit actions: one fixed tmax operation or fixed selection write per
    // action and one exact operand set. The page never composes an argv and
    // never retries one on its own.
    const GATEWAY_ACTIONS = ['gateway_start', 'gateway_stop', 'gateway_restart']
    const ACTION_KINDS = ['resume_quota', 'reset_failure', 'observe_provider', 'open_management', 'add_model', 'remove_model', 'apply_target', ...GATEWAY_ACTIONS]
    const ACTION_LABELS = {
      resume_quota: 'actionResumeQuota',
      reset_failure: 'actionResetFailure',
      observe_provider: 'actionObserveQuota',
      open_management: 'actionOpenManagement',
      gateway_start: 'actionStartGateway',
      gateway_stop: 'actionStopGateway',
      gateway_restart: 'actionRestartGateway',
      add_model: 'actionAddModel',
      remove_model: 'actionRemoveModel',
      apply_target: 'actionApplyHarness',
    }
    const ACTION_SCOPES = ['quota', 'auth', 'models', 'all']
    const ADD_PROTOCOLS = ['chat-completions', 'responses', 'messages']
    const ACTION_FAILURE_STATUSES = ['failed', 'blocked', 'refused', 'rejected', 'error']
    const LABEL_TYPES = { TOKENS_LIMIT: 'windowTokens', TIME_LIMIT: 'windowTime' }
    const LABEL_KEYS = { rolling: 'windowRolling', interval: 'windowRolling', weekly: 'windowWeekly', monthly: 'windowMonthly', weekly_allowance: 'windowWeekly' }

    const STYLE = `
.tmx-page { display: flex; width: 100%; height: 100%; min-width: 0; min-height: 0; overflow: hidden; color: var(--dsw-alias-label-primary); background: var(--dsw-alias-bg-base); font-size: 14px; line-height: 1.5; font-variant-numeric: tabular-nums; }
.tmx-scroll { flex: 1; min-height: 0; overflow: auto; scrollbar-gutter: stable; --dsh-scrollbar-width: 9px; --dsh-scrollbar-thumb-border: 2px; }
.tmx-content { max-width: 1160px; margin: 0 auto; padding: 28px clamp(20px, 4vw, 48px) 48px; padding-left: max(clamp(20px, 4vw, 48px), env(safe-area-inset-left)); padding-right: max(clamp(20px, 4vw, 48px), env(safe-area-inset-right)); }
.tmx-titleRow { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px 12px; }
.tmx-title { margin: 0; font-size: 18px; line-height: 26px; font-weight: 600; letter-spacing: -0.01em; }
.tmx-tools { display: flex; align-items: center; gap: 12px; }
.tmx-meta { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.tmx-dot { box-sizing: border-box; display: inline-block; flex: none; width: 8px; height: 8px; border-radius: 50%; corner-shape: round; }
.tmx-dot[data-tone='ok'] { background: var(--dsw-alias-state-success-primary); }
.tmx-dot[data-tone='warn'] { background: var(--dsw-alias-state-warn-primary); }
.tmx-dot[data-tone='error'] { background: var(--dsw-alias-state-error-primary); }
.tmx-dot[data-tone='idle'] { background: var(--dsw-alias-label-tertiary); }
/* A first-level page's header action is the sibling pages' 32px capsule; on a touch screen every control keeps a 44px target. */
.tmx-button { box-sizing: border-box; display: inline-flex; align-items: center; justify-content: center; gap: 6px; min-height: 32px; padding: 0 12px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: 16px; background: transparent; color: var(--dsw-alias-label-primary); font: inherit; font-size: 13px; line-height: 20px; cursor: pointer; }
.tmx-button[data-kind='primary'] { border-color: transparent; background: var(--dsw-alias-button-primary-fill); color: var(--dsw-alias-label-primary-foreground); }
@media (hover: hover) {
  .tmx-button:hover:not(:disabled) { background: var(--dsw-alias-interactive-bg-hover-solid); }
  .tmx-button[data-kind='primary']:hover:not(:disabled) { background: var(--dsw-alias-button-primary-hover); }
}
.tmx-button:disabled { opacity: 0.4; cursor: default; }
.tmx-button[data-busy='true']:disabled { opacity: 1; background: var(--dsw-alias-interactive-bg-hover); }
@media (pointer: coarse) { .tmx-button { min-height: 44px; padding: 0 18px; font-size: 15px; } }
.tmx-section { margin-top: 28px; }
.tmx-sectionTitle { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px; margin: 0 0 10px; font-size: 14px; line-height: 22px; font-weight: 500; }
/*
 * An account is one object a person checks one at a time, so it gets a wide
 * card. Every window it reports keeps its own bar, named, filled with what it
 * has used and showing what it used of what, and when it resets. Two columns on
 * a desktop, one on a phone: the same card.
 */
.tmx-accounts { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 440px), 1fr)); gap: 12px; margin: 0; padding: 0; list-style: none; }
.tmx-account { display: flex; flex-direction: column; gap: 8px; padding: 12px 14px 14px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-xl); }
.tmx-accountHead { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; min-height: 20px; }
.tmx-accountName { font-size: 14px; line-height: 20px; font-weight: 500; overflow-wrap: anywhere; }
.tmx-tag { display: inline-flex; align-items: center; padding: 0 8px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: 12px; font-size: 12px; line-height: 20px; color: var(--dsw-alias-label-secondary); }
.tmx-tag[data-tone='error'] { border-color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 55%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
/* Every window in a card shares one column geometry, so a name, a bar, a value,
   and a reset time line up from row to row. */
.tmx-windows { display: grid; grid-template-columns: minmax(78px, max-content) minmax(48px, 1fr) minmax(80px, max-content) minmax(64px, max-content); align-items: center; column-gap: 10px; row-gap: 6px; margin: 0; padding: 0; list-style: none; }
.tmx-window { display: grid; grid-column: 1 / -1; grid-template-columns: subgrid; align-items: center; column-gap: 10px; min-height: 18px; }
.tmx-windowText { display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px 10px; }
.tmx-windowName { min-width: 78px; max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-bar { position: relative; box-sizing: border-box; height: 8px; border-radius: 99px; background: var(--dsw-alias-border-l2); overflow: hidden; }
/* The bar fills with what the window has used, the same quantity its value states. */
.tmx-barFill { display: block; height: 100%; border-radius: 99px; background: var(--dsw-alias-state-success-primary); transition: width var(--ds-transition-duration, 200ms) var(--ds-ease-in-out, ease), background-color var(--ds-transition-duration, 200ms) var(--ds-ease-in-out, ease); }
.tmx-barFill[data-tone='warn'] { background: var(--dsw-alias-state-warn-primary); }
.tmx-barFill[data-tone='error'] { background: var(--dsw-alias-state-error-primary); }
.tmx-windowValue { min-width: 0; overflow: hidden; text-overflow: ellipsis; font-size: 14px; line-height: 18px; font-weight: 600; text-align: start; white-space: nowrap; font-variant-numeric: tabular-nums; }
.tmx-windowQuota { font-size: 13px; font-weight: 400; color: var(--dsw-alias-label-secondary); }
.tmx-windowValue[data-tone='warn'] { color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
.tmx-windowValue[data-tone='error'] { color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-windowReset { min-width: 0; overflow: hidden; text-overflow: ellipsis; font-size: 13px; line-height: 18px; text-align: end; color: var(--dsw-alias-label-secondary); white-space: nowrap; }
.tmx-windowReset[data-empty='true'] { visibility: hidden; }
.tmx-resetWord { display: none; }
.tmx-windowWide { min-width: 0; overflow-wrap: anywhere; font-size: 13px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-accountActions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 2px; }
.tmx-modelId { font-family: var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace); font-size: 13.5px; line-height: 20px; overflow-wrap: anywhere; }
.tmx-skeletonBar { display: block; width: 60%; height: 10px; border-radius: 4px; background: var(--dsw-alias-interactive-bg-hover); }
.tmx-skeletonName { width: 30%; margin-bottom: 4px; }
.tmx-empty { margin: 0; font-size: 13px; line-height: 20px; color: var(--dsw-alias-label-secondary); }
.tmx-note { margin: 8px 0 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-errorText { font-size: 12px; line-height: 18px; color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); overflow-wrap: anywhere; }
.tmx-break { overflow-wrap: anywhere; }
.tmx-sr { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip-path: inset(50%); white-space: nowrap; border: 0; }
/* Reads that failed, and the preview, confirmation, and result of one action. */
.tmx-card { border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-xl); padding: 12px 14px; }
.tmx-cardNote { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; margin-top: 12px; }
.tmx-banner { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; margin: 12px 0 0; padding: 10px 12px; border: 0.5px solid color-mix(in srgb, var(--dsw-alias-state-error-primary) 45%, var(--dsw-alias-border-l3)); border-radius: var(--dsw-radius-md); background: var(--dsw-alias-settings-card-fill); }
.tmx-panel { display: flex; flex-direction: column; gap: 8px; margin: 12px 0 0; padding: 14px 16px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); border-radius: var(--dsw-radius-xl); background: var(--dsw-alias-settings-card-fill); }
.tmx-panelTitle { display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px; margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-panelNote { margin: 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-panelActions { display: flex; flex-wrap: wrap; gap: 12px; }
.tmx-connectLines { display: flex; flex-direction: column; gap: 6px; margin: 0; }
.tmx-connectGroup dt { font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-connectGroup dd { margin: 0 0 4px; }
.tmx-inlineList { margin: 0; padding: 0 0 0 14px; font-size: 12px; line-height: 18px; }
.tmx-chip { display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: var(--dsw-radius-md); font-size: 12px; line-height: 18px; }
.tmx-chip[data-tone='error'] { border-color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 55%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-chip[data-tone='warn'] { border-color: color-mix(in srgb, var(--dsw-alias-state-warn-primary) 55%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
.tmx-chip[data-tone='ok'] { border-color: color-mix(in srgb, var(--dsw-alias-state-success-primary) 50%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-success-primary) 75%, var(--dsw-alias-label-primary)); }
/* Phone: one column, and a window becomes two lines so its bar spans the whole card. */
@media (max-width: 720px) {
  .tmx-page { font-size: 15px; }
  .tmx-account { padding: 14px; }
  .tmx-windows { display: flex; flex-direction: column; align-items: stretch; gap: 10px; }
  .tmx-window { display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px 10px; }
  .tmx-windowName { order: 1; min-width: 0; max-width: 100%; overflow: visible; overflow-wrap: anywhere; white-space: normal; font-size: 14px; line-height: 22px; }
  .tmx-windowReset { order: 2; font-size: 14px; line-height: 22px; text-align: start; }
  .tmx-windowReset[data-empty='true'] { display: none; }
  .tmx-resetWord { display: inline; }
  .tmx-windowValue { order: 3; margin-left: auto; font-size: 15px; line-height: 22px; }
  .tmx-bar { order: 4; flex-basis: 100%; height: 10px; align-self: center; }
  .tmx-windowWide { order: 2; flex-basis: 100%; font-size: 14px; }
  .tmx-modelId { font-size: 14px; }
}
@media (max-width: 420px) { .tmx-meta { display: none; } }
@media (prefers-reduced-motion: reduce) { .tmx-barFill { transition: none; } }
/* Seats: navigation marker, Settings > Models card, tool card. */
/* The page's read-failure evidence: one compact line each, never a paragraph. */
.tmx-errors { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 12px; margin: 10px 0 0; }
.tmx-errors .tmx-errorText { flex: 1 1 100%; margin: 0; }
.tmx-toolbar { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px 16px; margin-top: 16px; }
.tmx-pending { display: flex; align-items: center; gap: 8px; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); white-space: nowrap; }
.tmx-linkButton { border: 0; background: transparent; padding: 2px 0; color: var(--dsw-alias-label-primary); font: inherit; font-size: 12px; line-height: 18px; font-weight: 600; cursor: pointer; }
.tmx-linkButton:hover { color: color-mix(in srgb, var(--dsw-alias-state-warn-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-tabs { display: flex; justify-content: flex-start; gap: 2px; width: max-content; max-width: 100%; margin: 16px 0 0; padding: 2px; border-radius: 999px; background: var(--dsw-alias-interactive-bg-hover); }
.tmx-tab { box-sizing: border-box; display: inline-flex; align-items: center; min-height: 28px; padding: 0 14px; border: 0; border-radius: 999px; background: transparent; color: var(--dsw-alias-label-secondary); font: inherit; font-size: 13px; line-height: 20px; cursor: pointer; }
.tmx-tab:hover { color: var(--dsw-alias-label-primary); }
.tmx-tab[data-active='true'] { background: var(--dsw-alias-settings-card-fill); color: var(--dsw-alias-label-primary); font-weight: 500; }
.tmx-tabBadge { display: inline-flex; align-items: center; justify-content: center; min-width: 16px; height: 16px; margin-left: 5px; padding: 0 4px; border-radius: 999px; background: color-mix(in srgb, var(--dsw-alias-state-warn-primary) 18%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); font-size: 10px; line-height: 1; font-weight: 600; vertical-align: 1px; }
/* The models tab: one provider-grouped list, one inline Harness control per model. */
.tmx-search { box-sizing: border-box; display: flex; flex: 1 1 240px; align-items: center; gap: 8px; width: 100%; max-width: 360px; height: 34px; padding: 0 10px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: 999px; background: var(--dsw-alias-settings-card-fill); color: var(--dsw-alias-label-secondary); }
.tmx-search:focus-within { border-color: var(--dsw-alias-label-tertiary); }
.tmx-searchGlyph { flex: none; font-size: 14px; line-height: 1; }
.tmx-searchInput { flex: 1; min-width: 0; border: 0; outline: none; background: transparent; color: var(--dsw-alias-label-primary); font: inherit; font-size: 13px; }
.tmx-searchInput::placeholder { color: var(--dsw-alias-label-tertiary); }
.tmx-searchInput::-webkit-search-cancel-button { display: none; }
.tmx-searchClear { flex: none; width: 18px; height: 18px; padding: 0; border: 0; border-radius: 999px; background: var(--dsw-alias-interactive-bg-hover); color: var(--dsw-alias-label-secondary); font: inherit; font-size: 14px; line-height: 1; cursor: pointer; }
.tmx-catalog { margin-top: 12px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); border-radius: var(--dsw-radius-xl); background: var(--dsw-alias-settings-card-fill); }
.tmx-catalogEmpty { display: flex; flex-direction: column; align-items: flex-start; gap: 10px; padding: 24px 16px; }
.tmx-catalogError { margin: 8px 0 0; }
.tmx-providerGroup { border-bottom: 0.5px solid var(--dsw-alias-border-l2); }
.tmx-providerGroup:last-child { border-bottom: 0; }
.tmx-providerHead { display: flex; align-items: center; gap: 9px; min-height: 40px; padding: 8px 14px; border-bottom: 0.5px solid var(--dsw-alias-border-l2); }
.tmx-providerName { color: var(--dsw-alias-label-secondary); font-size: 12px; line-height: 18px; font-weight: 600; letter-spacing: 0.03em; text-transform: uppercase; overflow-wrap: anywhere; }
.tmx-providerCount { color: var(--dsw-alias-label-tertiary); font-size: 11.5px; line-height: 18px; }
.tmx-providerProtocols { margin-left: auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--dsw-alias-label-tertiary); font-size: 11px; line-height: 18px; }
.tmx-mark { flex: none; display: inline-grid; place-items: center; width: 22px; height: 22px; border: 0.5px solid var(--dsw-alias-border-l3); border-radius: 6px; background: var(--dsw-alias-settings-card-fill); color: var(--dsw-alias-label-secondary); font-size: 10px; font-weight: 600; line-height: 1; }
.tmx-mark[data-gateway='true'] { border-color: transparent; background: var(--dsw-alias-button-primary-fill); color: var(--dsw-alias-label-primary-foreground); }
.tmx-modelRow { box-sizing: border-box; display: flex; align-items: center; gap: 12px; min-height: 52px; padding: 8px 14px; border-bottom: 0.5px solid var(--dsw-alias-border-l2); }
.tmx-modelRows .tmx-modelRow:last-child { border-bottom: 0; }
.tmx-modelRow:hover { background: var(--dsw-alias-interactive-bg-hover); }
.tmx-modelMain { display: flex; flex: 1; align-items: center; gap: 10px; min-width: 0; }
.tmx-modelId { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace); font-size: 13.5px; line-height: 20px; }
.tmx-protocol { display: inline-flex; flex: none; align-items: center; height: 20px; padding: 0 6px; border-radius: 5px; background: var(--dsw-alias-interactive-bg-hover); color: var(--dsw-alias-label-secondary); font-size: 10.5px; line-height: 18px; white-space: nowrap; }
.tmx-harnessWrap { position: relative; display: flex; flex: none; }
.tmx-harnessControl { box-sizing: border-box; display: inline-flex; align-items: center; justify-content: flex-start; gap: 7px; min-width: 128px; max-width: 210px; height: 30px; padding: 0 8px; border: 0.5px solid transparent; border-radius: 8px; background: var(--dsw-alias-interactive-bg-hover); color: var(--dsw-alias-label-primary); font: inherit; font-size: 12px; line-height: 18px; text-align: left; cursor: pointer; }
.tmx-harnessControl:hover { border-color: var(--dsw-alias-border-l3); }
.tmx-harnessControl[aria-expanded='true'] { border-color: var(--dsw-alias-border-l3); }
.tmx-harnessControl .tmx-dot { width: 6px; height: 6px; }
.tmx-harnessEmpty { justify-content: center; min-width: 112px; border: 0.5px dashed var(--dsw-alias-border-l3); background: transparent; color: var(--dsw-alias-label-secondary); }
.tmx-harnessEmpty:hover { background: var(--dsw-alias-interactive-bg-hover); color: var(--dsw-alias-label-primary); }
.tmx-harnessLabel { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tmx-harnessEmpty .tmx-harnessLabel { flex: none; }
.tmx-plus { flex: none; color: var(--dsw-alias-label-tertiary); font-size: 13px; line-height: 1; }
.tmx-chevron { flex: none; color: var(--dsw-alias-label-tertiary); font-size: 12px; line-height: 1; margin-left: auto; }
/* The anchored picker: declared Harnesses only, no create action. */
.tmx-popover { position: absolute; top: calc(100% + 6px); right: 0; z-index: 30; box-sizing: border-box; width: min(332px, calc(100vw - 24px)); max-height: min(70vh, 460px); overflow: auto; padding: 6px; border: 0.5px solid var(--dsw-alias-border-l2); border-radius: var(--dsw-radius-xl); background: var(--dsw-alias-settings-card-fill); box-shadow: 0 16px 42px color-mix(in srgb, var(--dsw-alias-label-primary) 18%, transparent); animation: tmxPopIn var(--ds-transition-duration, 160ms) var(--ds-ease-in-out, ease-out); }
.tmx-popover[data-flip='true'] { top: auto; bottom: calc(100% + 6px); }
@keyframes tmxPopIn { from { opacity: 0; transform: translateY(-3px); } to { opacity: 1; transform: none; } }
.tmx-popoverSearch { display: flex; align-items: center; gap: 7px; height: 32px; padding: 0 8px; border-radius: 8px; background: var(--dsw-alias-interactive-bg-hover); color: var(--dsw-alias-label-tertiary); }
.tmx-popoverSearchInput { flex: 1; min-width: 0; border: 0; outline: none; background: transparent; color: var(--dsw-alias-label-primary); font: inherit; font-size: 12px; }
.tmx-popoverSearchInput::placeholder { color: var(--dsw-alias-label-tertiary); }
.tmx-popoverGroup { display: flex; align-items: center; gap: 8px; margin: 7px 5px 2px; color: var(--dsw-alias-label-tertiary); font-size: 10.5px; line-height: 16px; }
.tmx-popoverGroup::after { content: ''; height: 1px; flex: 1; background: var(--dsw-alias-border-l2); }
.tmx-popoverItem { display: flex; align-items: center; gap: 8px; min-height: 34px; padding: 3px 6px; border-radius: 7px; cursor: pointer; }
.tmx-popoverItem:hover { background: var(--dsw-alias-interactive-bg-hover); }
.tmx-popoverItem input { flex: none; width: 14px; height: 14px; margin: 0; accent-color: var(--dsw-alias-button-primary-fill); }
.tmx-popoverName { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; line-height: 18px; }
.tmx-popoverMeta { flex: none; color: var(--dsw-alias-label-tertiary); font-size: 10.5px; line-height: 16px; white-space: nowrap; }
.tmx-popoverStatus { flex: none; font-size: 10px; line-height: 16px; white-space: nowrap; }
.tmx-popoverStatus[data-tone='warn'] { color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
.tmx-popoverStatus[data-tone='ok'] { color: color-mix(in srgb, var(--dsw-alias-state-success-primary) 75%, var(--dsw-alias-label-primary)); }
.tmx-popoverStatus[data-tone='idle'] { color: var(--dsw-alias-label-tertiary); }
.tmx-popoverEmpty { margin: 4px 6px; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); }
.tmx-popoverError { margin: 4px 6px; }
.tmx-popoverFooter { display: flex; align-items: center; justify-content: space-between; margin-top: 4px; padding: 6px 7px 3px; border-top: 0.5px solid var(--dsw-alias-border-l2); }
.tmx-popoverFooter span { color: var(--dsw-alias-label-tertiary); font-size: 10.5px; line-height: 16px; }
.tmx-popoverDone { height: 26px; padding: 0 10px; border: 0; border-radius: 6px; background: var(--dsw-alias-button-primary-fill); color: var(--dsw-alias-label-primary-foreground); font: inherit; font-size: 11px; line-height: 18px; font-weight: 500; cursor: pointer; }
.tmx-harnessMark { flex: none; display: inline-grid; place-items: center; width: 28px; height: 28px; border-radius: 8px; background: var(--dsw-alias-interactive-bg-hover); color: var(--dsw-alias-label-secondary); }
.tmx-popoverItem .tmx-harnessMark { width: 20px; height: 20px; border-radius: 6px; }
.tmx-harnessInitial { font-size: 9px; line-height: 1; font-weight: 600; }
/* The Harness tab: one row per declared target, one target-level status. */
.tmx-harnessSection { margin-top: 16px; }
.tmx-harnessList { border: 0.5px solid var(--dsw-alias-settings-card-stroke); border-radius: var(--dsw-radius-xl); background: var(--dsw-alias-settings-card-fill); }
.tmx-harnessGroup { border-bottom: 0.5px solid var(--dsw-alias-border-l2); }
.tmx-harnessGroup:last-child { border-bottom: 0; }
.tmx-harnessRow { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 11px; padding: 13px 15px 6px; }
.tmx-harnessInfo { flex: 1; min-width: 0; }
.tmx-harnessName { display: block; font-size: 14px; line-height: 20px; font-weight: 600; letter-spacing: -0.01em; overflow-wrap: anywhere; }
.tmx-harnessMeta { display: block; margin-top: 1px; color: var(--dsw-alias-label-secondary); font-size: 11.5px; line-height: 17px; overflow-wrap: anywhere; }
.tmx-harnessActions { display: flex; flex-wrap: wrap; align-items: center; gap: 7px; }
.tmx-pill { display: inline-flex; align-items: center; gap: 5px; height: 22px; padding: 0 7px; border-radius: 6px; background: var(--dsw-alias-interactive-bg-hover); color: var(--dsw-alias-label-secondary); font-size: 10.5px; line-height: 16px; white-space: nowrap; }
.tmx-pill .tmx-dot { width: 5px; height: 5px; }
.tmx-pill[data-tone='warn'] { background: color-mix(in srgb, var(--dsw-alias-state-warn-primary) 15%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); }
.tmx-pill[data-tone='ok'] { background: color-mix(in srgb, var(--dsw-alias-state-success-primary) 14%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-success-primary) 75%, var(--dsw-alias-label-primary)); }
.tmx-applyButton { min-height: 28px; padding: 0 11px; border-radius: 8px; font-size: 12px; }
.tmx-modelChips { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 0 15px 12px 52px; }
.tmx-modelChip { display: inline-flex; align-items: center; gap: 5px; max-width: 220px; height: 23px; padding: 0 7px; border: 0.5px solid var(--dsw-alias-border-l2); border-radius: 6px; background: var(--dsw-alias-interactive-bg-hover); color: var(--dsw-alias-label-secondary); font-size: 10.5px; line-height: 17px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tmx-modelChip .tmx-dot { width: 5px; height: 5px; }
.tmx-addModel { display: inline-flex; align-items: center; height: 23px; padding: 0 4px; border: 0; background: transparent; color: var(--dsw-alias-label-secondary); font: inherit; font-size: 10.5px; line-height: 17px; cursor: pointer; }
.tmx-addModel:hover { color: var(--dsw-alias-label-primary); }
.tmx-harnessNote { margin: 0; padding: 0 15px 12px 52px; font-size: 11px; line-height: 17px; color: var(--dsw-alias-label-secondary); }
.tmx-harnessWritten { color: color-mix(in srgb, var(--dsw-alias-state-success-primary) 75%, var(--dsw-alias-label-primary)); font-weight: 600; }
/* One Harness apply: dry-run preview, explicit confirmation, typed result. */
.tmx-backdrop { position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; padding: 22px; background: color-mix(in srgb, var(--dsw-alias-label-primary) 32%, transparent); }
.tmx-dialog { box-sizing: border-box; display: flex; flex-direction: column; width: min(480px, 100%); max-height: calc(100vh - 44px); border: 0.5px solid var(--dsw-alias-border-l2); border-radius: var(--dsw-radius-xl); background: var(--dsw-alias-bg-base); box-shadow: 0 24px 64px color-mix(in srgb, var(--dsw-alias-label-primary) 24%, transparent); overflow: hidden; }
.tmx-dialogHead { display: flex; align-items: flex-start; gap: 12px; padding: 16px 18px 0; }
.tmx-dialogTitle { flex: 1; margin: 0; font-size: 15px; line-height: 22px; font-weight: 600; overflow-wrap: anywhere; }
.tmx-dialogClose { display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 26px; border: 0; border-radius: 6px; background: transparent; color: var(--dsw-alias-label-secondary); font: inherit; font-size: 16px; line-height: 1; cursor: pointer; }
.tmx-dialogClose:hover { background: var(--dsw-alias-interactive-bg-hover); }
.tmx-dialogBody { display: flex; flex-direction: column; gap: 9px; padding: 6px 18px 0; overflow: auto; }
.tmx-dialogLead { margin: 0; color: var(--dsw-alias-label-secondary); font-size: 12.5px; line-height: 19px; }
.tmx-dialogMeta { margin: 0; color: var(--dsw-alias-label-tertiary); font-family: var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace); font-size: 10.5px; line-height: 16px; overflow-wrap: anywhere; }
.tmx-applyFiles { display: flex; flex-direction: column; padding: 1px 9px; border: 0.5px solid var(--dsw-alias-border-l2); border-radius: 9px; background: var(--dsw-alias-interactive-bg-hover); }
.tmx-applyFile { display: flex; align-items: flex-start; gap: 8px; padding: 7px 0; border-bottom: 0.5px solid var(--dsw-alias-border-l2); color: var(--dsw-alias-label-secondary); font-size: 10.5px; line-height: 16px; }
.tmx-applyFile:last-child { border-bottom: 0; }
.tmx-applyFileGlyph { flex: none; color: var(--dsw-alias-label-tertiary); }
.tmx-applyFilePath { min-width: 0; overflow-wrap: anywhere; font-family: var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace); }
.tmx-applyFileFormat { flex: none; color: var(--dsw-alias-label-tertiary); }
.tmx-dialogBlockers { display: flex; flex-direction: column; gap: 4px; margin: 0; padding: 0 0 0 14px; }
.tmx-dialogNote { margin: 0; padding: 8px 9px; border-radius: 7px; background: color-mix(in srgb, var(--dsw-alias-state-warn-primary) 14%, transparent); color: color-mix(in srgb, var(--dsw-alias-state-warn-label) 70%, var(--dsw-alias-label-primary)); font-size: 10.5px; line-height: 16px; }
.tmx-dialogActions { display: flex; justify-content: flex-end; gap: 7px; padding: 14px 18px 16px; }
.tmx-toast { position: fixed; left: 50%; bottom: 20px; z-index: 60; box-sizing: border-box; max-width: min(440px, calc(100vw - 32px)); padding: 8px 11px; border: 0.5px solid var(--dsw-alias-border-l2); border-radius: 9px; background: var(--dsw-alias-settings-card-fill); box-shadow: 0 10px 28px color-mix(in srgb, var(--dsw-alias-label-primary) 16%, transparent); color: var(--dsw-alias-label-secondary); font-size: 11.5px; line-height: 17px; overflow-wrap: anywhere; transform: translateX(-50%); }
.tmx-toast[data-tone='error'] { color: color-mix(in srgb, var(--dsw-alias-state-error-primary) 80%, var(--dsw-alias-label-primary)); }
.tmx-result { margin-top: 12px; padding: 9px 12px; border: 0.5px solid var(--dsw-alias-border-l2); border-radius: 9px; background: var(--dsw-alias-settings-card-fill); }
.tmx-resultHead { display: flex; align-items: center; gap: 8px; }
.tmx-resultTitle { font-size: 12px; line-height: 18px; font-weight: 600; }
.tmx-resultMeta { flex: 1; min-width: 0; overflow-wrap: anywhere; color: var(--dsw-alias-label-secondary); font-size: 11px; line-height: 17px; }
.tmx-resultClose { flex: none; width: 22px; height: 22px; border: 0; border-radius: 5px; background: transparent; color: var(--dsw-alias-label-secondary); font: inherit; font-size: 14px; line-height: 1; cursor: pointer; }
.tmx-resultClose:hover { background: var(--dsw-alias-interactive-bg-hover); }
.tmx-resultDetails { margin-top: 4px; color: var(--dsw-alias-label-secondary); font-size: 11px; line-height: 17px; }
.tmx-resultDetails summary { cursor: pointer; color: var(--dsw-alias-label-secondary); }
.tmx-resultLines { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 3px 10px; margin: 6px 0 0; }
.tmx-resultLines dt { color: var(--dsw-alias-label-tertiary); }
.tmx-resultLines dd { margin: 0; overflow-wrap: anywhere; }
.tmx-previewFacts { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 3px 10px; margin: 0; font-size: 11px; line-height: 17px; color: var(--dsw-alias-label-secondary); }
.tmx-previewFacts dt { color: var(--dsw-alias-label-tertiary); }
.tmx-previewFacts dd { margin: 0; overflow-wrap: anywhere; }
.tmx-modelsSection { margin-top: 16px; }
.tmx-modelsSection .tmx-toolbar { margin-top: 0; }
/* The gateway tab keeps its compact facts and one lifecycle control. */
.tmx-facts { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 6px 16px; margin: 10px 0 0; font-size: 12.5px; line-height: 20px; }
.tmx-fact { display: contents; }
.tmx-fact dt { color: var(--dsw-alias-label-secondary); }
.tmx-fact dd { margin: 0; overflow-wrap: anywhere; }
.tmx-gatewayCard { max-width: 560px; }
.tmx-gatewayHead { display: flex; align-items: center; gap: 8px; }
.tmx-gatewayTitle { margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-gatewayStatus { font-size: 13px; line-height: 20px; color: var(--dsw-alias-label-secondary); }
.tmx-gatewayReason { margin-top: 8px; }
.tmx-gatewayActions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
.tmx-tab:focus-visible, .tmx-harnessControl:focus-visible, .tmx-searchClear:focus-visible, .tmx-linkButton:focus-visible, .tmx-addModel:focus-visible, .tmx-popoverDone:focus-visible, .tmx-dialogClose:focus-visible, .tmx-resultClose:focus-visible { outline: 2px solid var(--dsw-alias-label-primary); outline-offset: 2px; }
@media (prefers-reduced-motion: reduce) { .tmx-popover { animation: none; } }
@media (max-width: 720px) {
  .tmx-tabs { justify-content: flex-start; width: 100%; overflow-x: auto; scrollbar-width: none; }
  .tmx-tabs::-webkit-scrollbar { display: none; }
  .tmx-tab { flex: none; white-space: nowrap; }
  .tmx-search { max-width: none; }
  .tmx-toolbar { align-items: stretch; flex-direction: column; }
  .tmx-modelRow { align-items: flex-start; flex-wrap: wrap; gap: 8px 12px; padding: 10px 12px; }
  .tmx-modelMain { flex: 1 0 100%; }
  .tmx-harnessWrap { flex: 1; }
  .tmx-harnessControl { max-width: none; flex: 1; }
  .tmx-popover { position: fixed; top: auto; bottom: 12px; left: 12px; right: 12px; width: auto; max-height: 60vh; }
  .tmx-modelChips, .tmx-harnessNote { padding-left: 12px; }
  .tmx-providerHead, .tmx-harnessRow { padding-left: 12px; padding-right: 12px; }
}
@media (pointer: coarse) {
  .tmx-tab { min-height: 44px; padding: 0 20px; }
  .tmx-search { min-height: 48px; }
  .tmx-searchClear { width: 44px; height: 44px; }
  .tmx-harnessControl { min-height: 44px; }
  .tmx-popoverItem { min-height: 44px; }
  .tmx-popoverDone { height: 44px; }
}
.tmx-navMark { display: inline-flex; align-items: center; gap: 4px; }
.tmx-navMark[data-tone='warn'] .tmx-dot { background: var(--dsw-alias-state-warn-primary); }
.tmx-navMark[data-tone='error'] .tmx-dot { background: var(--dsw-alias-state-error-primary); }
.tmx-navMark[data-wide='false'] { gap: 0; }
.tmx-seatCard { display: flex; flex-direction: column; gap: 10px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-xl); padding: 12px 14px; }
.tmx-seatHead { display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px; margin: 0; font-size: 14px; line-height: 22px; font-weight: 500; }
.tmx-seatFacts { margin: 0; font-size: 12px; line-height: 18px; color: var(--dsw-alias-label-secondary); overflow-wrap: anywhere; }
.tmx-seatActions { display: flex; flex-wrap: wrap; gap: 8px; }
.tmx-toolCard { display: flex; flex-direction: column; gap: 6px; border: 0.5px solid var(--dsw-alias-settings-card-stroke); background: var(--dsw-alias-settings-card-fill); border-radius: var(--dsw-radius-md); padding: 10px 12px; }
.tmx-toolHead { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0; font-size: 13px; line-height: 20px; font-weight: 500; }
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

    /**
     * A reset reads the same way on every row: today and tomorrow keep their
     * clock time, a later day keeps its date, and the year appears only when it
     * is not this one. `now` is injectable so tests are stable.
     */
    function formatReset(date, t, now) {
      if (!(date instanceof Date) || Number.isNaN(date.getTime())) return ''
      const reference = new Date(typeof now === 'number' ? now : Date.now())
      const midnight = value => Date.UTC(value.getFullYear(), value.getMonth(), value.getDate())
      const days = Math.round((midnight(date) - midnight(reference)) / 86400000)
      try {
        if (days === 0 || days === 1) {
          const clock = new Intl.DateTimeFormat(dateLocale, { hour: '2-digit', minute: '2-digit' }).format(date)
          return `${days === 0 ? t('today') : t('tomorrow')} ${clock}`
        }
        return new Intl.DateTimeFormat(dateLocale,
          date.getFullYear() === reference.getFullYear()
            ? { month: 'short', day: 'numeric' }
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
     * An unqualified epoch carries no proven meaning, but it is still the only
     * reset evidence the provider reported. The page reads it as Unix
     * milliseconds and says so instead of hiding the time.
     */
    function unqualifiedResetAt(reset) {
      if (reset === null || typeof reset !== 'object' || resetQualified(reset)) return undefined
      // Both discriminators must name the same native form: an unqualified epoch
      // in some other unit is not a millisecond timestamp the page can print.
      if (reset.kind !== 'unqualified_epoch_millis' || reset.encoding !== 'epoch_milliseconds_unqualified') return undefined
      const raw = numeric(reset.raw)
      if (raw === undefined) return undefined
      const date = new Date(raw)
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

    /**
     * One window as the share of its quota already used. tmax reports some
     * windows as consumed and others as remaining; both become the single
     * consumed fraction the bar fills with. When no share is computable there
     * is no meter at all: a bar is a claim about proportion.
     *
     * `value` (and `quota`, when the account reports a quantity) is what the
     * row prints, and `spoken` says the same thing to a screen reader, so the
     * visible number, the bar, and the spoken label cannot disagree.
     */
    function windowReading(row, index, t) {
      if (row === null || typeof row !== 'object') return undefined
      const name = windowName(row.label, index, t)
      const raw = boundedText(JSON.stringify(row.label ?? {}), 120)
      const resetIsUnqualified = resetUnqualified(row.reset)
      const reset = resetIsUnqualified ? unqualifiedResetAt(row.reset) : resetAt(row.reset)
      const percent = numeric(row.percent)
      // A share is computable only on the two bases that name it. A basis the
      // contract does not define is not read as consumed by default: that would
      // draw a bar nobody measured.
      if (percent !== undefined && (row.basis === 'remaining_percent' || row.basis === 'consumed_percent')) {
        const used = Math.round(Math.max(0, Math.min(100, row.basis === 'remaining_percent' ? 100 - percent : percent)))
        return {
          name,
          used,
          value: `${used}%`,
          // A percentage is a share of its own limit, so the quota it is used
          // of is 100%: printed beside it like every other window's quota.
          quota: '100%',
          spoken: `${t('usedShare')} ${used}% / 100%`,
          raw,
          reset,
          resetUnqualified: resetIsUnqualified,
        }
      }
      const counters = row.counters !== null && typeof row.counters === 'object' ? row.counters : undefined
      const total = numeric(counters?.total ?? counters?.limit)
      const left = numeric(counters?.remaining)
      // Counters are a share only on the two bases that define them, and a
      // counter is an independent field: a reported used amount measures the
      // share without what is left, and the subtraction only covers accounts
      // that report what is left and nothing else.
      const spent = numeric(counters?.used)
        ?? (total === undefined || left === undefined ? undefined : total - left)
      if ((row.basis === 'counters' || row.basis === 'credit') && total !== undefined && total > 0 && spent !== undefined) {
        const used = Math.round(Math.max(0, Math.min(100, (spent / total) * 100)))
        return {
          name,
          used,
          value: compact(spent),
          quota: compact(total),
          spoken: `${t('usedShare')} ${compact(spent)} / ${compact(total)}`,
          raw,
          reset,
          resetUnqualified: resetIsUnqualified,
        }
      }
      // A row that exists but yields no share still names itself: a text line
      // with no meter, never a bar standing in for a measurement we cannot make.
      return { name, used: undefined, text: t('quotaUnreadable'), unreadable: true, raw, reset, resetUnqualified: resetIsUnqualified }
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

    // ------------------------------------------------------------- state reads

    /**
     * A route is usable only when DSH serves it and its credential is
     * configured. A registered-but-inactive route is configured, not connected.
     */
    function usableRoutes(state) {
      const routes = Array.isArray(state?.routes) ? state.routes : []
      return routes.filter(route => route !== null && typeof route === 'object'
        && route.active === true && route.credentialConfigured === true)
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

    // ------------------------------------------------------------- Harnesses
    //
    // A Harness is one declared tmax target (`state.harnesses`). The host
    // reports each target as a whole: its configured selections, the desired
    // configuration revision, and whether that revision is what the Agent has
    // loaded. It does not report which individual model is live, so the page
    // never draws a per-model applied state: a Harness is applied or pending,
    // and every chip of that Harness carries the same tone.

    const AGENT_NAMES = {
      dsh: 'DeepSeek Harness',
      pi: 'Pi',
      codex: 'Codex',
      opencode: 'OpenCode',
      hermes: 'Hermes',
      omp: 'OMP',
    }
    const AGENT_SHORTS = {
      dsh: 'DSH',
      pi: 'Pi',
      codex: 'Codex',
      opencode: 'OpenCode',
      hermes: 'Hermes',
      omp: 'OMP',
    }
    // Small marks are drawn inline: no image asset ships with the page. A known
    // Agent gets its own glyph; anything else is a letter tile.
    const AGENT_GLYPHS = {
      dsh: { viewBox: '0 0 16 16', stroke: true, paths: ['M1.8 10.4c1.6-3.5 4.3-3.5 5.9 0 1.6 3.5 4.3 3.5 5.9 0', 'M12.9 4.2h.01'] },
      pi: { viewBox: '0 0 24 24', paths: ['M1 1h16.5v11H12v5.5H6.5V23H1V1zm5.5 5.5V12H12V6.5H6.5z', 'M17.5 12H23v11h-5.5V12z'] },
      opencode: { viewBox: '0 0 24 24', paths: ['M16 6H8v12h8V6zm4 16H4V2h16v20z'] },
      hermes: { viewBox: '12 16 40 40', paths: ['M21 20h7v14h8V20h7v32h-7V40h-8v12h-7zM13 21l7 3v6l-7-3zM51 21l-7 3v6l7-3z'] },
      omp: { viewBox: '12 16 40 40', paths: ['M14 16h36v8H40v32h-8V24h-6v22h-8V24h-4z'] },
    }

    /** The Agent's product name, or its bounded host token when unknown. */
    function agentName(agent) {
      if (typeof agent !== 'string' || agent === '') return ''
      return AGENT_NAMES[agent] ?? boundedText(agent, 40)
    }

    /** The short label used beside a model: DSH, Pi, Codex. */
    function agentShort(agent) {
      if (typeof agent !== 'string' || agent === '') return ''
      if (AGENT_SHORTS[agent] !== undefined) return AGENT_SHORTS[agent]
      const text = boundedText(agent, 24)
      return text === '' ? '' : text.toUpperCase()
    }

    function countSelections(harness) {
      let count = 0
      for (const provider of Array.isArray(harness?.providers) ? harness.providers : []) {
        count += Array.isArray(provider?.models) ? provider.models.length : 0
      }
      return count
    }

    /**
     * The declared Harnesses, normalized. A row the host did not finish is
     * skipped rather than guessed at: an unreadable entry is not a Harness the
     * page can draw.
     */
    function harnessesOf(state) {
      const raw = Array.isArray(state?.harnesses) ? state.harnesses : []
      const list = []
      for (const item of raw) {
        if (item === null || typeof item !== 'object') continue
        const id = boundedText(item.id, 80)
        if (id === '') continue
        const agent = boundedText(item.agent, 40)
        const providers = []
        for (const provider of Array.isArray(item.providers) ? item.providers : []) {
          if (provider === null || typeof provider !== 'object') continue
          const name = boundedText(provider.name, 80)
          const sourceProvider = boundedText(provider.sourceProvider, 160)
          if (name === '' && sourceProvider === '') continue
          const models = []
          for (const model of Array.isArray(provider.models) ? provider.models : []) {
            if (model === null || typeof model !== 'object') continue
            const modelId = boundedText(model.id, 160)
            if (modelId === '') continue
            models.push({ id: modelId, protocol: boundedText(model.protocol, 48) })
          }
          providers.push({ name, sourceProvider, relayed: provider.relayed === true, models })
        }
        const blockers = (Array.isArray(item.blockers) ? item.blockers : []).slice(0, 12)
          .map(blocker => blocker !== null && typeof blocker === 'object'
            ? { code: boundedText(blocker.code, 48), message: boundedText(blocker.message, 240) }
            : { code: '', message: boundedText(blocker, 240) })
          .filter(blocker => blocker.code !== '' || blocker.message !== '')
        const modelCount = numeric(item.modelCount)
        const normalized = {
          id,
          agent,
          name: agentName(agent) || id,
          short: agentShort(agent) || id,
          path: boundedText(item.path, 200),
          providerCount: numeric(item.providerCount),
          defaultConfigured: item.defaultConfigured === true,
          appliedState: boundedText(item.appliedState, 64),
          unapplied: item.unapplied === true ? true : item.unapplied === false ? false : null,
          blockers,
          default: item.default !== null && typeof item.default === 'object'
            ? {
                provider: boundedText(item.default.provider, 80),
                id: boundedText(item.default.id, 160),
                protocol: boundedText(item.default.protocol, 48),
              }
            : null,
          providers,
        }
        normalized.modelCount = modelCount ?? countSelections(normalized)
        list.push(normalized)
      }
      return list
    }

    /** One target-level word, never a per-model claim. */
    function harnessTone(unapplied) {
      return unapplied === false ? 'ok' : unapplied === true ? 'warn' : 'idle'
    }

    function harnessStatusKey(unapplied) {
      return unapplied === false ? 'harnessApplied' : unapplied === true ? 'harnessPending' : 'harnessUnknown'
    }

    /**
     * How two Harnesses of the same Agent are told apart. The short label is
     * the Agent, and a duplicated Agent carries its target ID beside it.
     */
    function harnessLabelMap(harnesses) {
      const counts = new Map()
      for (const harness of harnesses) counts.set(harness.short, (counts.get(harness.short) ?? 0) + 1)
      const labels = new Map()
      for (const harness of harnesses) {
        labels.set(harness.id, (counts.get(harness.short) ?? 0) > 1 ? `${harness.short} ${harness.id}` : harness.short)
      }
      return labels
    }

    /**
     * Which model a Harness selects, keyed by the catalog's provider ID and
     * model ID. A target entry names its `sourceProvider`, so a relayed model
     * and a direct model of the same ID stay distinct.
     */
    function harnessMembershipOf(harnesses) {
      const membership = new Map()
      for (const harness of harnesses) {
        for (const provider of harness.providers) {
          if (provider.sourceProvider === '') continue
          for (const model of provider.models) {
            const key = `${provider.sourceProvider}\u0000${model.id}`
            const member = { harness, protocol: model.protocol }
            const found = membership.get(key)
            if (found === undefined) membership.set(key, [member])
            else found.push(member)
          }
        }
      }
      return membership
    }

    /** Harnesses whose desired configuration the Agent has not loaded. */
    function unappliedHarnesses(harnesses) {
      return harnesses.filter(harness => harness.unapplied === true)
    }

    function harnessStatusText(harness, t) {
      return t(harnessStatusKey(harness.unapplied))
    }

    function defaultSelectionText(harness) {
      const selection = harness?.default
      if (selection === null || selection === undefined) return ''
      return [selection.provider, selection.id].filter(part => part !== '').join('/')
    }

    function harnessMark(harness) {
      const glyph = AGENT_GLYPHS[harness?.agent]
      const initial = boundedText(harness?.short, 4) || '?'
      const body = glyph === undefined
        ? h('span', { className: 'tmx-harnessInitial', key: 'initial' }, initial)
        : h('svg', {
            width: 18,
            height: 18,
            viewBox: glyph.viewBox,
            fill: glyph.stroke === true ? 'none' : 'currentColor',
            stroke: glyph.stroke === true ? 'currentColor' : 'none',
            strokeWidth: glyph.stroke === true ? 1.6 : undefined,
            strokeLinecap: glyph.stroke === true ? 'round' : undefined,
            'aria-hidden': 'true',
            key: 'glyph',
          }, glyph.paths.map((d, index) => h('path', { d, key: index })))
      return h('span', {
        className: 'tmx-harnessMark',
        key: 'mark',
        'data-agent': harness?.agent === '' ? undefined : harness?.agent,
        'aria-hidden': 'true',
      }, body)
    }

    /**
     * The one explicit next step, only when one exists and the page can offer
     * it. A broken config blocks every write; a failed projection is a retry;
     * a home integration that is installed but not loaded is a reload.
     */
    function nextAction(load) {
      // A failed read never offers a write. Even when a last-known state is
      // still on screen (labeled last-known), only a fresh read can change the
      // evidence, so the one honest next step is that read — never Apply or any
      // other operation that would act on stale facts.
      if (load?.phase === 'failed') return { kind: 'retry' }
      const state = load?.value
      if (state === undefined || state === null || state.ok !== true) return undefined
      const routes = Array.isArray(state.routes) ? state.routes : []
      const ownership = state.ownership ?? {}
      const config = state.config ?? {}
      const conflict = ownership.conflict
      const usable = usableRoutes(state)
      const active = routes.filter(route => route !== null && typeof route === 'object' && route.active === true)
      const missing = routes.filter(route => route !== null && typeof route === 'object' && route.credentialConfigured === false)
      const readFailures = sectionErrors(state)
      const sectionRetry = readFailures.length > 0 ? { kind: 'retry', reason: 'section' } : undefined

      // A config that is invalid or unreadable blocks every write, so it is
      // named before Apply can be offered.
      if (config.state === 'invalid') return { kind: 'config', reason: 'invalid' }
      if (config.state === 'unavailable') return { kind: 'config', reason: 'unavailable' }
      // A failed projection read leaves routes unknown; only a retry is honest.
      if (projectionFailed(state)) return { kind: 'retry', reason: 'projection' }
      if (usable.length === 0) {
        if (conflict !== null && conflict !== undefined) {
          return { kind: 'conflict', code: conflictCode(conflict), message: conflictMessage(conflict) }
        }
        // Whether the routes are applied is unknown when the settings could
        // not be read; applying on a guess could write over a working home.
        if (ownership.owner === 'unknown') return { kind: 'retry', reason: 'settings' }

        // Nothing is declared: applying is the step that gives DSH something.
        if (routes.length === 0) return { kind: 'connect' }
        // A credential tmax knows is missing is the one thing Apply repairs;
        // an unknown readiness never justifies a write.
        if (missing.length > 0) return { kind: 'connect' }
        // Declared but not loaded by DSH yet: a reload is the non-writing step.
        if (active.length === 0) return { kind: 'reload' }
        return sectionRetry
      }

      const gateway = state.gateway ?? {}
      // A readiness that is unknown because its own read failed is not evidence
      // of anything: read again before acting on it.
      if (gateway.ready !== true && sectionErrors(state).some(error => error.section === 'gateway')) {
        return { kind: 'retry', reason: 'section' }
      }
      // Unknown readiness is not readiness: the page offers Start for a
      // configured gateway that has not reported ready, and the preview itself
      // reports the blocker instead of performing anything.
      if (gateway.configured === true && gateway.ready !== true) return { kind: 'gateway', code: gateway.reasonCode }
      const blockers = Array.isArray(state.target?.blockers) ? state.target.blockers : []
      if (blockers.length > 0) {
        const blocker = blockers[0]
        // The blocker itself asks for the start, so the page offers it.
        if (blocker?.code === 'gateway_not_ready' && state.gateway?.configured === true) {
          return { kind: 'gateway', code: 'gateway_not_ready' }
        }
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
     * Gateway lifecycle actions only where they can change the reported state:
     * start when it is not ready, stop/restart only when a daemon is actually
     * identified or the gateway reports ready. An unknown readiness never
     * offers an operation that may have nothing to act on.
     */
    /** Whether the reported daemon discovery token names an identified daemon. */
    function daemonIdentified(gateway) {
      return gateway?.daemonDiscoveryState === 'identified' || gateway?.daemonDiscoveryState === 'compatible'
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
      const preview = dryRun === true
      if (action === 'open_management' || GATEWAY_ACTIONS.includes(action)) {
        return { action, dryRun: preview }
      }
      // A Harness operation names one declared target; the host validates the ID
      // against its own state, so the page only bounds the string it received.
      if (action === 'apply_target') {
        const target = boundedText(descriptor.target, 80)
        if (target === '') return undefined
        return { action, target, dryRun: preview }
      }
      if (action === 'add_model' || action === 'remove_model') {
        const target = boundedText(descriptor.target, 80)
        const provider = boundedText(descriptor.provider, 80)
        const model = boundedText(descriptor.model, 160)
        const protocol = descriptor.protocol
        if (target === '' || provider === '' || model === '') return undefined
        if (typeof protocol !== 'string' || !ADD_PROTOCOLS.includes(protocol)) return undefined
        return { action, target, provider, model, protocol, dryRun: preview }
      }
      const provider = descriptor.provider
      if (typeof provider !== 'string' || provider === '') return undefined
      if (action === 'observe_provider') {
        const scope = typeof descriptor.scope === 'string' && ACTION_SCOPES.includes(descriptor.scope) ? descriptor.scope : 'quota'
        return { action, provider, scope, dryRun: preview }
      }
      return { action, provider, dryRun: preview }
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
          code,
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
     * seat. Only an object carrying the current `schemaVersion` and `ok === true` is
     * a state; an unsupported schema is a typed failure, and a refused model is
     * an error. Neither is ever adopted as evidence.
     * @returns {{ object: boolean, state?: object, schemaUnsupported?: boolean, message?: string }}
     */
    function stateGate(value, response) {
      if (value === null || typeof value !== 'object') return { object: false }
      if (value.schemaVersion !== STATE_SCHEMA_VERSION) {
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
      // One explicit repair action at a time, in one panel: preview, then an
      // explicit confirmation, then a typed result. Never retried on its own.
      const [repair, setRepair] = React.useState({ phase: 'idle' })
      // The page's own browsing state. A tab switch only changes which read
      // model section is shown; it never starts a read or an action.
      const [tab, setTab] = React.useState('models')
      const [query, setQuery] = React.useState('')
      // One model's Harness popover: the model it answers for, its own search
      // text, and the viewport anchor it was opened from.
      const [picker, setPicker] = React.useState(null)
      // A selection write runs without a preview: the host saves desired
      // configuration and returns the state the control is redrawn from.
      const [selection, setSelection] = React.useState({ phase: 'idle' })
      // One Harness apply at a time: preview, confirmation, then the returned
      // state. 'applied' keeps its announcement until the next action.
      const [apply, setApply] = React.useState({ phase: 'idle' })
      // Monotonic fences: a late response from a cancelled dialog or an older
      // selection write must not reopen or overwrite the newer interaction.
      const [applySeq] = React.useState({ current: 0 })
      const [selectionSeq] = React.useState({ current: 0 })
      const [toast, setToast] = React.useState(null)
      // The typed result of the last save or apply, kept until dismissed so the
      // page can disclose code/status/steps/files rather than only announce success.
      const [lastResult, setLastResult] = React.useState(null)

      const loadState = React.useCallback(async () => {
        setLoad(previous => ({ phase: previous.value === undefined ? 'loading' : 'refreshing', value: previous.value }))
        try {
          const response = await fetch(new URL('/_tmax/state', document.baseURI), { credentials: 'same-origin' })
          const value = await readBody(response)
          if (!response.ok) throw new Error(stateError(value, response.status))
          if (value === null || typeof value !== 'object') throw new Error(`HTTP ${response.status}`)
          // The state schema is a hard consumer gate: an unknown version is a
          // failure, never a payload this page guesses at. A last-known state
          // stays on screen, labeled, instead of being cleared.
          if (value.schemaVersion !== STATE_SCHEMA_VERSION) {
            setLoad(previous => ({
              phase: 'failed',
              value: previous.value,
              schemaUnsupported: true,
              message: `${t('schema')} ${boundedText(value.schemaVersion, 24)}`,
            }))
            return
          }
          if (value.ok !== true) {
            // A refused read model is a failed read, not a reason to drop the
            // last good table. Its section failures stay on the load so the
            // page names them beside the last-known data.
            const errors = Array.isArray(value.errors) ? value.errors : []
            setLoad(previous => ({
              phase: 'failed',
              value: previous.value,
              message: stateError(value, response.status),
              errors,
            }))
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

      // A toast is a confirmation, not a log: it clears itself after a moment.
      // The handle is unref'd where the runtime offers that, so a pending
      // confirmation never holds a non-browser process open.
      React.useEffect(() => {
        if (toast === null) return undefined
        const handle = setTimeout(() => setToast(null), 3200)
        if (handle !== null && typeof handle === 'object' && typeof handle.unref === 'function') handle.unref()
        return () => clearTimeout(handle)
      }, [toast])

      const refresh = React.useCallback(async () => {
        setObserve({ phase: 'running', items: [] })
        try {
          const { response, value } = await postJson('/_tmax/observe', {})
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

      const changeTab = React.useCallback(next => {
        if (apply.phase === 'executing') return
        applySeq.current += 1
        setPicker(null)
        setApply({ phase: 'idle' })
        setTab(next)
      }, [apply.phase, applySeq])

      const togglePicker = React.useCallback((key, event) => {
        setSelection({ phase: 'idle' })
        setPicker(previous => {
          if (previous !== null && previous.key === key) return null
          return { key, query: '', anchor: pickerAnchor(event) }
        })
      }, [])

      const changePickerQuery = React.useCallback(value => {
        setPicker(previous => previous === null ? null : { ...previous, query: String(value) })
      }, [])

      const closePicker = React.useCallback(() => setPicker(null), [])

      /**
       * One desired-configuration write: add sends the protocol the model
       * publishes, remove names the exact protocol selection the Harness holds.
       * There is no preview; the typed result reports success or the reason.
       */
      const runSelection = React.useCallback(async (harness, provider, model, protocol, action) => {
        const payload = actionPayload({ action, target: harness.id, provider: provider.id, model: model.id, protocol }, false)
        if (payload === undefined) return false
        const key = `${provider.id}\u0000${model.id}`
        const seq = ++selectionSeq.current
        setLastResult(null)
        setSelection({ phase: 'running', key, action })
        try {
          const { response, value } = await postJson('/_tmax/action', payload)
          const result = value !== null && typeof value === 'object' && value.result !== null && typeof value.result === 'object'
            ? value.result
            : undefined
          const succeeded = actionSucceeded(value, response.ok)
          // A stale response's snapshot is older than a newer completed write.
          // Never let it overwrite the visible state: read the authoritative
          // state instead, then stop updating this action's UI.
          if (seq !== selectionSeq.current) {
            await loadState()
            return false
          }
          const nextState = value?.state !== null && typeof value?.state === 'object' ? value.state : undefined
          const adopted = nextState !== undefined && adoptState(setLoad, nextState, response, t)
          if (!adopted) await loadState()
          if (result !== undefined) setLastResult({ action, result })
          if (succeeded) {
            setSelection({ phase: 'saved', key, result })
            setToast({ tone: 'ok', text: `${boundedText(model.id, 160)} · ${t('selectionSaved')}` })
            return true
          }
          const detail = hostCodeText(result?.code, result?.message) || stateError(value, response.status)
          setSelection({ phase: 'failed', key, result, message: detail })
          setToast({ tone: 'error', text: `${t('selectionFailed')} · ${boundedText(detail, 160)}` })
          return false
        } catch (error) {
          if (seq !== selectionSeq.current) return false
          const message = plainError(error)
          setSelection({ phase: 'failed', key, message })
          setToast({ tone: 'error', text: `${t('selectionFailed')} · ${boundedText(message, 160)}` })
          return false
        }
      }, [loadState, t])

      /**
       * A checkbox toggles one Harness selection for one model. Adding names
       * the model's preferred protocol; removing names each protocol selection
       * the Harness already holds, one exact host action each.
       */
      const toggleSelection = React.useCallback((harness, provider, model, protocols, checked) => {
        setSelection({ phase: 'idle' })
        if (checked === true) {
          const protocol = preferredProtocol(model.protocols)
          if (protocol === undefined || protocols.includes(protocol)) return
          void runSelection(harness, provider, model, protocol, 'add_model')
          return
        }
        // The host removes every protocol selection for this provider/model in
        // one fixed sequence; the requested protocol only proves the selector.
        const selected = protocols.length > 0
          ? protocols[0]
          : preferredProtocol(model.protocols)
        if (selected === undefined) return
        void runSelection(harness, provider, model, selected, 'remove_model')
      }, [runSelection])

      const previewApply = React.useCallback(async harness => {
        const payload = actionPayload({ action: 'apply_target', target: harness.id }, true)
        if (payload === undefined) return
        const seq = ++applySeq.current
        setLastResult(null)
        setApply({ phase: 'previewing', harness })
        try {
          const { response, value } = await postJson('/_tmax/action', payload)
          if (seq !== applySeq.current) return
          const result = value !== null && typeof value === 'object' && value.result !== null && typeof value.result === 'object'
            ? value.result
            : undefined
          const succeeded = actionSucceeded(value, response.ok)
          const nextState = value?.state !== null && typeof value?.state === 'object' ? value.state : undefined
          if (nextState !== undefined) adoptState(setLoad, nextState, response, t)
          if (!succeeded) {
            setApply({ phase: 'failed', harness, result, message: hostCodeText(result?.code, result?.message) || stateError(value, response.status) })
            return
          }
          const data = result?.data !== null && typeof result?.data === 'object' ? result.data : {}
          setApply({ phase: 'preview', harness, data, result })
        } catch (error) {
          if (seq !== applySeq.current) return
          setApply({ phase: 'failed', harness, message: plainError(error) })
        }
      }, [t])

      const confirmApply = React.useCallback(async () => {
        if (apply.phase !== 'preview' || apply.harness === null || typeof apply.harness !== 'object') return
        const harness = apply.harness
        const payload = actionPayload({ action: 'apply_target', target: harness.id }, false)
        if (payload === undefined) return
        const seq = applySeq.current
        setApply(previous => ({ ...previous, phase: 'executing' }))
        try {
          const { response, value } = await postJson('/_tmax/action', payload)
          if (seq !== applySeq.current) return
          const result = value !== null && typeof value === 'object' && value.result !== null && typeof value.result === 'object'
            ? value.result
            : undefined
          const succeeded = actionSucceeded(value, response.ok)
          const nextState = value?.state !== null && typeof value?.state === 'object' ? value.state : undefined
          const adopted = nextState !== undefined && adoptState(setLoad, nextState, response, t)
          if (!adopted) await loadState()
          if (seq !== applySeq.current) return
          if (!succeeded) {
            setApply({ phase: 'failed', harness, result, message: hostCodeText(result?.code, result?.message) || stateError(value, response.status) })
            return
          }
          setApply({ phase: 'applied', harness, result })
          setLastResult({ action: 'apply_target', result })
          setToast({ tone: 'ok', text: `${boundedText(harness.name, 120)} · ${t('applyApplied')}` })
        } catch (error) {
          if (seq !== applySeq.current) return
          setApply(previous => ({ ...previous, phase: 'failed', message: plainError(error) }))
        }
      }, [apply, loadState, t])

      const cancelApply = React.useCallback(() => {
        // An executing write cannot be cancelled at the host; keep the dialog
        // (and its result) authoritative instead of discarding the completion.
        if (apply.phase === 'executing') return
        applySeq.current += 1
        setApply({ phase: 'idle' })
      }, [apply.phase, applySeq])

      // An aria-modal dialog owns focus while it is open: focus the safe close
      // control, keep Tab inside it, close on Escape, and return focus after.
      const applyOpen = apply !== null && apply !== undefined && apply.phase !== 'idle' && apply.phase !== 'applied'
      React.useEffect(() => {
        if (typeof document === 'undefined' || applyOpen !== true) return undefined
        if (typeof document.querySelector !== 'function' || typeof document.addEventListener !== 'function') return undefined
        const dialog = document.querySelector('.tmx-dialog')
        if (dialog === null || typeof dialog.querySelectorAll !== 'function') return undefined
        const previous = typeof document.activeElement === 'object' ? document.activeElement : null
        const focusable = () => Array.from(dialog.querySelectorAll('button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'))
        const initial = dialog.querySelector('[data-autofocus]') ?? focusable()[0]
        if (initial !== null && typeof initial.focus === 'function') initial.focus()
        const onKeyDown = event => {
          if (event.key === 'Escape') {
            event.preventDefault?.()
            cancelApply()
            return
          }
          if (event.key !== 'Tab') return
          const items = focusable()
          if (items.length === 0) {
            event.preventDefault?.()
            return
          }
          const first = items[0]
          const last = items[items.length - 1]
          if (event.shiftKey === true && document.activeElement === first) {
            event.preventDefault?.()
            last.focus?.()
          } else if (event.shiftKey !== true && document.activeElement === last) {
            event.preventDefault?.()
            first.focus?.()
          }
        }
        document.addEventListener('keydown', onKeyDown)
        return () => {
          document.removeEventListener('keydown', onKeyDown)
          if (previous !== null && typeof previous.focus === 'function') previous.focus()
        }
      }, [applyOpen, cancelApply])

      const state = load.value !== null && typeof load.value === 'object' && load.value.ok === true ? load.value : undefined
      const harnesses = state === undefined ? [] : harnessesOf(state)
      const labels = harnessLabelMap(harnesses)
      const membership = harnessMembershipOf(harnesses)
      const unapplied = unappliedHarnesses(harnesses)
      const config = state?.config !== null && typeof state?.config === 'object' ? state.config : {}
      const ownership = state?.ownership !== null && typeof state?.ownership === 'object' ? state.ownership : {}
      const conflict = ownership.conflict
      // A failed read, an invalid configuration, or a reported ownership
      // conflict is not evidence a write may act on. Desired configuration
      // stays editable only while the read model is current and valid.
      const readBlocked = load.phase === 'failed'
        || config.state === 'invalid' || config.state === 'unavailable'
        || (conflict !== null && conflict !== undefined)
      const busy = observe.phase === 'running'
        || repair.phase === 'previewing' || repair.phase === 'executing'
        || selection.phase === 'running'
        || apply.phase === 'previewing' || apply.phase === 'executing'
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
            pageHeader(state, state === undefined && load.phase === 'failed' ? retryButton : refreshButton, t),
            state === undefined ? null : tabsBar(tab, changeTab, t, unapplied.length),
            h('p', { className: 'tmx-sr', key: 'live', role: 'status', 'aria-live': 'polite' },
              announce({ load, observe, repair, selection, apply }, t)),
            pageErrors(state, t, loadState),
            resultPanel(lastResult, t, () => setLastResult(null)),
            state === undefined ? null : routeBlockerLine(state, t),
            staleBanner(load, t, loadState),
            state === undefined && load.phase === 'failed' ? initialError(load, t, loadState) : null,
            repairPanel(repair, t, { confirm: confirmRepair, cancel: cancelRepair, disabled: busy }),
            observePanel(observe, t, load.phase === 'failed'),
            state === undefined && load.phase === 'loading' ? skeleton(t) : null,
            state === undefined ? null : tab === 'models'
              ? modelsSection(state, t, {
                query,
                harnesses,
                labels,
                membership,
                picker,
                selection,
                onQuery: setQuery,
                onTogglePicker: togglePicker,
                onPickerQuery: changePickerQuery,
                onClosePicker: closePicker,
                onToggleSelection: toggleSelection,
                onGoApply: () => changeTab('harness'),
                gatewayConfigured: state.gateway?.configured === true,
                busy,
                readBlocked,
              })
              : tab === 'harness'
                ? harnessSection(state, t, harnesses, {
                  onApply: harness => { void previewApply(harness) },
                  onAddModel: () => changeTab('models'),
                  busy,
                  readBlocked,
                })
                : tab === 'gateway'
                  ? gatewaySection(state, t, repairHandlers, busy)
                  : accountsSection(state, t, repairHandlers),
            applyDialog(apply, t, { confirm: () => { void confirmApply() }, cancel: cancelApply }),
          ])),
        toastLine(toast))
    }

    /**
     * The typed result of the last desired-configuration save or Harness apply.
     * The summary stays visible; the full code/status/data/steps/blockers/effects
     * are behind one disclosure so the page remains sparse without dropping the
     * result the host returned.
     */
    function resultPanel(last, t, onClose) {
      if (last === null || last === undefined) return null
      const actionKey = ACTION_LABELS[last.action] ?? 'resultTitle'
      const lines = actionResultLines(last.result, t, last.action)
      const status = last.result?.status === 'success'
        ? t('actionDone')
        : last.result?.status === 'dry_run' ? t('actionPreviewTitle') : t('actionResult')
      const code = typeof last.result?.code === 'string' && last.result.code !== ''
        ? boundedText(last.result.code, 48)
        : t('unknown')
      return h('section', { className: 'tmx-result', key: 'result', role: 'status' }, [
        h('div', { className: 'tmx-resultHead', key: 'head' }, [
          h('span', { className: 'tmx-resultTitle', key: 'title' }, t(actionKey)),
          h('span', { className: 'tmx-resultMeta', key: 'meta' }, `${status} · ${code}`),
          h('button', {
            type: 'button',
            className: 'tmx-resultClose',
            key: 'close',
            'aria-label': t('resultDismiss'),
            onClick: onClose,
          }, '×'),
        ]),
        lines.length === 0
          ? null
          : h('details', { className: 'tmx-resultDetails', key: 'details' }, [
            h('summary', { key: 'summary' }, t('resultDetails')),
            h('dl', { className: 'tmx-resultLines', key: 'lines' },
              lines.flatMap((line, index) => [
                h('dt', { key: `label:${index}` }, line.label),
                h('dd', { key: `value:${index}` }, line.items.join(' · ')),
              ])),
          ]),
      ])
    }

    /** The name, when the state was generated, and the one control that reads again. */
    function pageHeader(state, action, t) {
      const updated = typeof state?.generatedAt === 'string'
        ? h('span', { className: 'tmx-meta', key: 'meta' }, `${t('updated')} ${clockOf(state.generatedAt) || t('unknown')}`)
        : null
      return h('div', { className: 'tmx-titleRow', key: 'titleRow' }, [
        h('h1', { className: 'tmx-title', key: 'title' }, t('title')),
        h('div', { className: 'tmx-tools', key: 'tools' }, [updated, action]),
      ])
    }

    /**
     * Page-level evidence that something is wrong outside one section: a failed
     * section read, an invalid configuration, or a reported ownership conflict.
     * One compact line each, with the read retry beside them: the page states
     * the fact and never explains at length.
     */
    function pageErrors(state, t, retry) {
      if (state === undefined || state === null) return null
      const failures = sectionErrors(state)
      const config = configErrors(state)
      const ownership = state.ownership !== null && typeof state.ownership === 'object' ? state.ownership : {}
      const conflict = ownership.conflict
      const lines = []
      for (const failure of failures) lines.push(`${t('readFailed')}${t('colon')} ${failure.text}`)
      for (const failure of config) lines.push(`${t('configProblem')}${t('colon')} ${failure.text}`)
      if (conflict !== null && conflict !== undefined) {
        const text = hostCodeText(conflictCode(conflict), conflictMessage(conflict)) || entryText(conflict)
        if (text !== '') lines.push(`${t('conflict')}${t('colon')} ${text}`)
      }
      if (lines.length === 0) return null
      return h('div', { className: 'tmx-errors', key: 'pageErrors' }, [
        ...lines.map((line, index) => h('p', { className: 'tmx-errorText tmx-break', key: `line:${index}` }, line)),
        failures.length === 0 ? null : h('button', { type: 'button', className: 'tmx-button', key: 'retry', onClick: retry }, t('retry')),
      ])
    }

    /**
     * Decide whether the popover opens below its control or flips above it.
     * The popover is absolutely positioned inside the row wrapper, so its
     * horizontal alignment is CSS; only the vertical fit needs a measurement.
     * A narrow viewport renders it as a bottom sheet from CSS.
     */
    function pickerAnchor(event) {
      const rect = event?.currentTarget?.getBoundingClientRect?.()
      if (rect === undefined || rect === null || typeof rect.bottom !== 'number') return null
      const viewportHeight = typeof window !== 'undefined' && Number.isFinite(window.innerHeight) ? window.innerHeight : 0
      const viewportWidth = typeof window !== 'undefined' && Number.isFinite(window.innerWidth) ? window.innerWidth : 0
      if (viewportWidth > 0 && viewportWidth <= 720) return null
      // Match the stylesheet cap (min(70vh, 460px)). Prefer below, but flip only
      // when below cannot hold it and above has more room.
      const maxHeight = viewportHeight > 0 ? Math.min(460, Math.max(240, Math.round(viewportHeight * 0.7))) : 460
      const below = viewportHeight > 0 ? viewportHeight - rect.bottom - 12 : maxHeight
      const above = typeof rect.top === 'number' ? rect.top - 12 : 0
      if (below < maxHeight + 16 && above > below) return { flip: true }
      return { flip: false }
    }

    /**
     * A toast repeats what the live region says, visually; it is not a second
     * announcement for assistive technology, and it clears itself.
     */
    function toastLine(toast) {
      if (toast === null || toast === undefined) return null
      return h('div', {
        className: 'tmx-toast',
        'data-tone': toast.tone,
        key: 'toast',
        'aria-hidden': 'true',
      }, toast.text)
    }
    /**
     * A route the host could not build names why: an unsupported protocol, a
     * credential reference DSH cannot use, a missing base URL. Each blocker is
     * said once, beside the route it belongs to.
     */
    function routeBlockerLine(state, t) {
      const lines = []
      for (const route of Array.isArray(state.routes) ? state.routes : []) {
        if (route === null || typeof route !== 'object') continue
        for (const blocker of Array.isArray(route.blockers) ? route.blockers : []) {
          const text = hostCodeText(blocker?.code, blocker?.message) || entryText(blocker)
          if (text !== '') lines.push(`${boundedText(route.provider ?? route.name, 80)}${t('colon')} ${text}`)
        }
      }
      if (lines.length === 0) return null
      return h('p', { className: 'tmx-errorText tmx-break', key: 'routeBlockers' }, lines.join(' · '))
    }

    /** A configuration's own validation errors, bounded and code first. */
    function configErrors(state) {
      const config = state.config !== null && typeof state.config === 'object' ? state.config : {}
      return (Array.isArray(config.errors) ? config.errors : []).slice(0, 12)
        .map(error => hostCodeText(error?.code, error?.message))
        .filter(text => text !== '')
        .map(text => ({ section: 'config', code: '', message: '', text }))
    }

    /** Colour is spent on what needs attention; a lightly used window stays green. */
    function windowTone(used) {
      if (used === undefined) return ''
      return used >= 90 ? 'error' : used >= 75 ? 'warn' : 'ok'
    }

    /**
     * Every tmax account that reports a quota window, closest to its limit
     * first. An account without any window takes no card: it would only say
     * "unknown".
     */
    function accountsOf(state, t) {
      const providers = state.providers !== null && typeof state.providers === 'object' ? state.providers : {}
      const list = []
      for (const provider of Array.isArray(providers.tmax) ? providers.tmax : []) {
        if (provider === null || typeof provider !== 'object') continue
        const readings = windowsFor(provider, t)
        if (readings.length === 0) continue
        const measured = readings.filter(item => item.used !== undefined)
        const mostUsed = measured.length === 0 ? undefined : Math.max(...measured.map(item => item.used))
        list.push({ provider, readings, mostUsed })
      }
      // An account whose windows cannot be measured follows the measured ones.
      return list.sort((a, b) => (b.mostUsed ?? -1) - (a.mostUsed ?? -1))
    }

    function quotaRow(item, index, t) {
      const name = boundedText(item.name, 48)
      const key = `window:${index}`
      if (item.used === undefined) {
        // A row that yields no share names itself and takes no bar: a bar would
        // claim a proportion nobody measured.
        return h('li', { className: 'tmx-window tmx-windowText', key }, [
          h('span', { className: 'tmx-windowName', key: 'name' }, name),
          h('span', { className: 'tmx-windowWide', key: 'value' }, boundedText(item.text, 80)),
        ])
      }
      const tone = windowTone(item.used)
      const reset = item.reset === undefined ? '' : formatReset(item.reset, t)
      const approx = reset !== '' && item.resetUnqualified === true
      return h('li', { className: 'tmx-window', key }, [
        h('span', { className: 'tmx-windowName', key: 'name' }, name),
        h('span', {
          className: 'tmx-bar',
          key: 'bar',
          role: 'img',
          'aria-label': `${name} ${item.spoken}`,
        }, h('span', { className: 'tmx-barFill', 'data-tone': tone, style: { width: `${item.used}%` }, key: 'fill' })),
        h('span', { className: 'tmx-windowValue', 'data-tone': tone, key: 'value' }, [
          h('span', { key: 'used' }, item.value),
          // The quota is context for the used amount, never a second headline;
          // an account that reports only a percentage has no quantity to print.
          item.quota === undefined
            ? null
            : h('span', { className: 'tmx-windowQuota', key: 'quota' }, ` / ${item.quota}`),
        ]),
        h('span', {
          className: 'tmx-windowReset',
          'data-empty': reset === '' ? 'true' : undefined,
          ...(approx ? { title: t('resetApproxNote') } : {}),
          key: 'reset',
        }, [
          h('span', { className: 'tmx-resetWord', key: 'word' }, `${t('reset')} `),
          approx ? `${t('resetApprox')} ${reset}` : reset,
        ]),
      ])
    }

    function accountCard({ provider, readings }, index, t, handlers) {
      const name = boundedText(
        typeof provider?.id === 'string' && provider.id !== '' ? provider.id : t('unknown'), 120)
      // A tag exists only for a state a person can act on or must know about.
      const tags = [
        provider?.enabled === false ? h('span', { className: 'tmx-tag', key: 'disabled' }, t('providerDisabled')) : null,
        provider?.suspension?.state === 'suspended'
          ? h('span', { className: 'tmx-tag', 'data-tone': 'error', key: 'suspended' }, t('providerSuspended'))
          : null,
        failureIsActive(provider?.failure)
          ? h('span', { className: 'tmx-tag', 'data-tone': 'error', key: 'failure' }, t('providerFailing'))
          : null,
      ]
      // Resuming and resetting sit beside the evidence they repair; observing
      // is the page's Refresh, so no card repeats it.
      const actions = handlers === undefined ? [] : providerActions(provider)
        .filter(descriptor => descriptor.action !== 'observe_provider')
        .map(descriptor => h('button', {
          type: 'button',
          className: 'tmx-button',
          key: `action:${descriptor.action}`,
          'data-action': descriptor.action,
          onClick: () => handlers.run(descriptor),
          disabled: handlers.disabled,
        }, t(ACTION_LABELS[descriptor.action])))
      return h('li', { className: 'tmx-account', key: `account:${provider?.id ?? index}` }, [
        h('div', { className: 'tmx-accountHead', key: 'head' },
          [h('span', { className: 'tmx-accountName', key: 'name' }, name), ...tags]),
        h('ul', { className: 'tmx-windows', key: 'windows' }, readings.map((item, windowIndex) => quotaRow(item, windowIndex, t))),
        actions.length === 0 ? null : h('div', { className: 'tmx-accountActions', key: 'actions' }, actions),
      ])
    }

    function accountsSection(state, t, handlers) {
      const accounts = accountsOf(state, t)
      // A failed account read is named where the accounts would be, so an empty
      // list caused by the failure never reads as "no account".
      const failures = sectionErrors(state).filter(error => error.section === 'providers')
      return h('section', { className: 'tmx-section', key: 'accounts' }, [
        h('h2', { className: 'tmx-sectionTitle', key: 'title' }, t('accountsTitle')),
        ...failures.map((error, index) =>
          h('p', { className: 'tmx-errorText', key: `failure:${index}` }, `${t('readFailed')}${t('colon')} ${error.text}`)),
        accounts.length === 0
          ? (failures.length > 0 ? null : h('p', { className: 'tmx-empty', key: 'empty' }, t('noAccounts')))
          : h('ul', { className: 'tmx-accounts', key: 'accounts' }, accounts.map((account, index) => accountCard(account, index, t, handlers))),
      ])
    }

    /** The model catalog, or null when this state predates/hides it. */
    function catalogOf(state) {
      const catalog = state !== null && typeof state === 'object' ? state.catalog : null
      return catalog !== null && typeof catalog === 'object' ? catalog : null
    }

    function catalogProviders(catalog) {
      if (catalog === null || !Array.isArray(catalog.providers)) return []
      const providers = catalog.providers.filter(provider => provider !== null && typeof provider === 'object')
      // The derived gateway provider first, then a stable name order. This is
      // an ordering only: no provider gets a status label.
      const rank = provider => provider.source === 'gateway' ? 0 : 1
      return providers.slice().sort((left, right) => {
        const byRank = rank(left) - rank(right)
        if (byRank !== 0) return byRank
        const leftName = String(left.name ?? left.id ?? '').toLowerCase()
        const rightName = String(right.name ?? right.id ?? '').toLowerCase()
        return leftName.localeCompare(rightName)
      })
    }

    function providerMark(provider) {
      const source = provider?.name || provider?.id || ''
      const letter = String(source).trim().charAt(0).toUpperCase() || '?'
      return h('span', {
        className: 'tmx-mark',
        key: 'mark',
        'aria-hidden': 'true',
        'data-gateway': provider?.source === 'gateway' ? 'true' : undefined,
      }, letter)
    }

    function modelCountText(count, t) {
      if (count === null || count === undefined) return t('unknown')
      if (count === 1) return `1 ${t('modelOne')}`
      return `${count} ${t('modelUnit')}`
    }

    function fileCountText(count, t) {
      if (count === null || count === undefined) return ''
      if (count === 1) return `1 ${t('fileOne')}`
      return `${count} ${t('fileUnit')}`
    }

    function harnessCountText(count, t) {
      if (count === null || count === undefined) return ''
      if (count === 1) return `1 ${t('harnessCountOne')}`
      return `${count} ${t('harnessCountUnit')}`
    }

    function pendingHarnessText(count, t) {
      if (count === 1) return `1 ${t('pendingHarnessOne')}`
      return `${count} ${t('pendingHarnesses')}`
    }

    function protocolText(protocols) {
      const values = Array.isArray(protocols) ? protocols.filter(protocol => typeof protocol === 'string' && protocol !== '') : []
      return values.length === 0 ? '' : values.join(' · ')
    }

    function preferredProtocol(protocols) {
      const values = Array.isArray(protocols) ? protocols.filter(protocol => typeof protocol === 'string' && protocol !== '') : []
      if (values.includes('responses')) return 'responses'
      if (values.includes('chat-completions')) return 'chat-completions'
      return values[0]
    }

    function modelMatches(model, needle) {
      const id = typeof model?.id === 'string' ? model.id.toLowerCase() : ''
      if (id.includes(needle)) return true
      return Array.isArray(model?.protocols) && model.protocols.some(protocol => typeof protocol === 'string' && protocol.toLowerCase().includes(needle))
    }

    /** The catalog grouped by provider, filtered by one search string. */
    function catalogGroups(catalog, query) {
      const needle = typeof query === 'string' ? query.trim().toLowerCase() : ''
      const groups = []
      for (const provider of catalogProviders(catalog)) {
        const models = Array.isArray(provider.models)
          ? provider.models.filter(model => model !== null && typeof model === 'object')
          : []
        if (models.length === 0) continue
        const providerMatched = needle !== '' && [provider.name, provider.id]
          .some(value => typeof value === 'string' && value.toLowerCase().includes(needle))
        const matched = needle === '' || providerMatched ? models : models.filter(model => modelMatches(model, needle))
        if (matched.length > 0) groups.push({ provider, models: matched })
      }
      return groups
    }

    function tabsBar(tab, setTab, t, unappliedCount) {
      const items = [
        ['models', 'tabModels'],
        ['harness', 'tabHarness'],
        ['gateway', 'tabGateway'],
        ['quota', 'tabQuota'],
      ]
      // Ordinary navigation buttons, not the ARIA tabs pattern: the views are
      // page sections, and incomplete tab semantics would be worse than plain
      // pressed-state buttons for keyboard and screen-reader users.
      return h('div', { className: 'tmx-tabs', role: 'group', 'aria-label': t('tabsLabel'), key: 'tabs' },
        items.map(([id, key]) => h('button', {
          type: 'button',
          className: 'tmx-tab',
          key: id,
          'data-tab': id,
          'data-active': tab === id ? 'true' : undefined,
          'aria-pressed': tab === id ? 'true' : 'false',
          onClick: () => setTab(id),
        }, [
          t(key),
          id === 'harness' && unappliedCount > 0
            ? h('span', { className: 'tmx-tabBadge', key: 'badge', title: t('harnessPending') }, String(unappliedCount))
            : null,
        ])))
    }

    // ------------------------------------------------------ models and picker

    function searchField(query, t, onChange) {
      return h('label', { className: 'tmx-search', key: 'search' }, [
        h('span', { className: 'tmx-searchGlyph', key: 'glyph', 'aria-hidden': 'true' }, '⌕'),
        h('input', {
          type: 'search',
          className: 'tmx-searchInput',
          value: query,
          placeholder: t('searchModelsPlaceholder'),
          'aria-label': t('searchModelsPlaceholder'),
          onChange: event => onChange(event?.target?.value ?? ''),
          onKeyDown: event => { if (event?.key === 'Escape') onChange('') },
        }),
        query === ''
          ? null
          : h('button', {
            type: 'button',
            className: 'tmx-searchClear',
            key: 'clear',
            'aria-label': t('clearSearch'),
            onClick: () => onChange(''),
          }, '×'),
      ])
    }

    /** One model in one Harness: a target-level status, never a guess at a model. */
    function harnessControl(provider, model, members, t, options) {
      // One model may select the same Harness under two protocols; the control
      // names each Harness once.
      const unique = []
      for (const member of members) {
        if (!unique.some(known => known.harness.id === member.harness.id)) unique.push(member)
      }
      const labels = unique
        .map(member => options.labels.get(member.harness.id) ?? member.harness.short)
        .filter(label => label !== '')
      const tones = unique.map(member => harnessTone(member.harness.unapplied))
      const tone = tones.includes('warn') ? 'warn' : tones.length > 0 && tones.every(value => value === 'ok') ? 'ok' : 'idle'
      const open = options.picker !== null && options.picker.key === options.modelKey
      const statuses = unique
        .map(member => `${member.harness.id}:${member.harness.unapplied === true ? 'pending' : member.harness.unapplied === false ? 'applied' : 'unknown'}`)
        .join(',')
      const common = {
        type: 'button',
        key: 'control',
        'aria-haspopup': 'dialog',
        'aria-expanded': open ? 'true' : 'false',
        'data-model': model.id,
        'data-provider': provider.id,
        'data-harnesses': unique.map(member => member.harness.id).join(','),
        'data-statuses': statuses,
        onClick: event => options.onTogglePicker(options.modelKey, event),
      }
      if (members.length === 0) {
        return h('button', {
          ...common,
          className: 'tmx-harnessControl tmx-harnessEmpty',
          title: t('selectHarness'),
        }, [
          h('span', { className: 'tmx-plus', key: 'plus', 'aria-hidden': 'true' }, '+'),
          h('span', { className: 'tmx-harnessLabel', key: 'label' }, t('selectHarness')),
        ])
      }
      return h('button', {
        ...common,
        className: 'tmx-harnessControl',
        title: labels.join(' · '),
      }, [
        h('span', { className: 'tmx-dot', 'data-tone': tone, key: 'dot', 'aria-hidden': 'true' }),
        h('span', { className: 'tmx-harnessLabel', key: 'label' }, labels.join(' · ')),
        h('span', { className: 'tmx-chevron', key: 'chevron', 'aria-hidden': 'true' }, '⌄'),
      ])
    }

    function harnessMatches(harness, needle) {
      if (needle === '') return true
      return [harness.name, harness.short, harness.id, harness.agent, harness.path]
        .some(value => typeof value === 'string' && value.toLowerCase().includes(needle))
    }

    function pickerRow(harness, provider, model, protocols, t, options) {
      const checked = protocols.length > 0
      const gatewayBlocked = provider.source === 'gateway' && options.gatewayConfigured !== true
      const disabled = options.busy || options.readBlocked
        || (checked === false && (gatewayBlocked || provider.enabled !== true || model.enabled !== true))
      return h('label', {
        className: 'tmx-popoverItem',
        key: `harness:${harness.id}`,
        'data-harness': harness.id,
        'data-checked': checked ? 'true' : 'false',
      }, [
        h('input', {
          type: 'checkbox',
          checked,
          disabled,
          onChange: event => options.onToggleSelection(harness, provider, model, protocols, event?.target?.checked === true),
        }),
        harnessMark(harness),
        h('span', { className: 'tmx-popoverName', key: 'name' }, harness.name),
        h('span', { className: 'tmx-popoverMeta', key: 'meta' }, harness.id),
        h('span', { className: 'tmx-popoverStatus', 'data-tone': harnessTone(harness.unapplied), key: 'status' }, harnessStatusText(harness, t)),
      ])
    }

    /** The anchored picker: only declared Harnesses, checkboxes, and 完成. */
    function harnessPicker(t, provider, model, members, options) {
      const picker = options.picker
      const needle = picker.query.trim().toLowerCase()
      const protocolsByHarness = new Map()
      for (const member of members) {
        const found = protocolsByHarness.get(member.harness.id)
        if (found === undefined) protocolsByHarness.set(member.harness.id, [member.protocol])
        else found.push(member.protocol)
      }
      const rows = []
      for (const harness of options.harnesses) {
        if (!harnessMatches(harness, needle)) continue
        rows.push(pickerRow(harness, provider, model, protocolsByHarness.get(harness.id) ?? [], t, options))
      }
      const checkedCount = protocolsByHarness.size
      const shell = {
        className: 'tmx-popover',
        role: 'dialog',
        'aria-label': t('selectHarness'),
        key: 'picker',
        'data-flip': picker.anchor?.flip === true ? 'true' : undefined,
      }
      return h('div', shell, [
        h('label', { className: 'tmx-popoverSearch', key: 'search' }, [
          h('span', { className: 'tmx-searchGlyph', key: 'glyph', 'aria-hidden': 'true' }, '⌕'),
          h('input', {
            type: 'search',
            className: 'tmx-popoverSearchInput',
            value: picker.query,
            placeholder: t('harnessPickerSearch'),
            'aria-label': t('harnessPickerSearch'),
            onChange: event => options.onPickerQuery(event?.target?.value ?? ''),
            onKeyDown: event => { if (event?.key === 'Escape') options.onClosePicker() },
          }),
        ]),
        h('div', { className: 'tmx-popoverGroup', key: 'group' }, t('harnessPickerConfigured')),
        rows.length === 0
          ? h('p', { className: 'tmx-popoverEmpty', key: 'empty' },
            picker.query.trim() === '' && options.harnesses.length === 0 ? t('harnessNone') : t('searchNoMatch'))
          : null,
        ...rows,
        options.selection !== null && options.selection !== undefined
          && options.selection.phase === 'failed' && options.selection.key === options.modelKey
          ? h('p', { className: 'tmx-errorText tmx-popoverError', key: 'error' },
            `${t('selectionFailed')}${t('colon')} ${boundedText(options.selection.message, 200)}`)
          : null,
        h('div', { className: 'tmx-popoverFooter', key: 'footer' }, [
          h('span', { key: 'count' }, harnessCountText(checkedCount, t)),
          h('button', { type: 'button', className: 'tmx-popoverDone', key: 'done', onClick: options.onClosePicker }, t('harnessPickerDone')),
        ]),
      ])
    }

    function modelRow(provider, model, t, options) {
      const key = `${provider.id}\u0000${model.id}`
      const members = options.membership.get(key) ?? []
      const open = options.picker !== null && options.picker.key === key
      return h('div', {
        className: 'tmx-modelRow',
        key: `model:${provider.id}:${model.id}`,
        'data-provider': provider.id,
        'data-model': model.id,
      }, [
        h('div', { className: 'tmx-modelMain', key: 'main' }, [
          h('span', { className: 'tmx-modelId', key: 'id' }, boundedText(model.id, 160)),
          protocolText(model.protocols) === ''
            ? null
            : h('span', { className: 'tmx-protocol', key: 'protocol' }, protocolText(model.protocols)),
          model.enabled === false
            ? h('span', { className: 'tmx-tag', key: 'disabled' }, t('providerDisabled'))
            : null,
        ]),
        h('div', { className: 'tmx-harnessWrap', key: 'wrap' }, [
          harnessControl(provider, model, members, t, { ...options, modelKey: key }),
          open ? harnessPicker(t, provider, model, members, { ...options, modelKey: key }) : null,
        ]),
      ])
    }

    function catalogGroup(provider, models, t, options) {
      const protocols = []
      for (const model of models) {
        if (!Array.isArray(model.protocols)) continue
        for (const protocol of model.protocols) {
          if (typeof protocol === 'string' && protocol !== '' && !protocols.includes(protocol)) protocols.push(protocol)
        }
      }
      return h('div', {
        className: 'tmx-providerGroup',
        key: `provider:${provider.id}`,
        'data-provider': provider.id,
      }, [
        h('div', { className: 'tmx-providerHead', key: 'head' }, [
          providerMark(provider),
          h('span', { className: 'tmx-providerName', key: 'name' }, boundedText(provider.name || provider.id || t('unknown'), 120)),
          h('span', { className: 'tmx-providerCount', key: 'count' }, modelCountText(models.length, t)),
          provider.enabled === false ? h('span', { className: 'tmx-tag', key: 'disabled' }, t('providerDisabled')) : null,
          h('span', { className: 'tmx-providerProtocols', key: 'protocols' }, protocols.slice(0, 3).join(' · ')),
        ]),
        h('div', { className: 'tmx-modelRows', key: 'rows' }, models.map(model => modelRow(provider, model, t, options))),
      ])
    }

    /**
     * The models tab: one search field, a compact pending count, and every
     * catalog model with one inline Harness control. A Harness is applied or
     * pending as a target; a model row never claims more than that.
     */
    function modelsSection(state, t, options) {
      const catalog = catalogOf(state)
      const failures = sectionErrors(state).filter(error => error.section === 'catalog' || error.section === 'harnesses')
      const failureLine = failures.length === 0
        ? null
        : h('p', { className: 'tmx-errorText tmx-catalogError', key: 'catalogError' },
          `${t('readFailed')}${t('colon')} ${failures.map(error => error.text).join(' · ')}`)
      const unappliedCount = options.harnesses.filter(harness => harness.unapplied === true).length
      const pending = unappliedCount === 0
        ? null
        : h('div', { className: 'tmx-pending', key: 'pending' }, [
          h('span', { className: 'tmx-dot', 'data-tone': 'warn', 'aria-hidden': 'true', key: 'dot' }),
          h('span', { key: 'text' }, pendingHarnessText(unappliedCount, t)),
          h('button', { type: 'button', className: 'tmx-linkButton', key: 'go', onClick: options.onGoApply }, t('goApply')),
        ])
      const toolbar = h('div', { className: 'tmx-toolbar', key: 'toolbar' }, [
        searchField(options.query, t, options.onQuery),
        pending,
      ])
      const catalogReady = catalog !== null && catalog.state === 'available'
      if (!catalogReady) {
        return h('section', { className: 'tmx-section tmx-modelsSection', key: 'models' }, [
          toolbar,
          failureLine,
          h('p', { className: 'tmx-empty tmx-catalogEmpty', key: 'unavailable' }, t('catalogUnavailable')),
        ])
      }
      const groups = catalogGroups(catalog, options.query)
      let body
      if (groups.length === 0) {
        body = h('div', { className: 'tmx-catalog tmx-catalogEmpty', key: 'empty' }, [
          h('p', { className: 'tmx-empty', key: 'text' }, options.query.trim() === '' ? t('noProviders') : t('searchNoMatch')),
          options.query.trim() === ''
            ? null
            : h('button', { type: 'button', className: 'tmx-button', key: 'clear', onClick: () => options.onQuery('') }, t('clearSearch')),
        ])
      } else {
        body = h('div', { className: 'tmx-catalog', key: 'catalog' }, groups.map(group => catalogGroup(group.provider, group.models, t, options)))
      }
      return h('section', { className: 'tmx-section tmx-modelsSection', key: 'models' }, [toolbar, failureLine, body])
    }

    // -------------------------------------------------------------- Harness tab

    function harnessRow(harness, t, options) {
      const name = harness.name
      const applied = harness.unapplied === false
      const tone = harnessTone(harness.unapplied)
      const reloadPending = harness.appliedState === 'applied_reload_pending'
      const pills = [
        h('span', { className: 'tmx-pill', 'data-tone': tone, key: 'status' }, [
          h('span', { className: 'tmx-dot', 'data-tone': tone, 'aria-hidden': 'true', key: 'dot' }),
          harnessStatusText(harness, t),
        ]),
      ]
      if (reloadPending) {
        pills.push(h('span', { className: 'tmx-pill', 'data-tone': 'warn', key: 'reload' }, t('harnessReloadPending')))
      }
      if (harness.defaultConfigured) {
        const selection = defaultSelectionText(harness)
        pills.push(h('span', {
          className: 'tmx-tag',
          key: 'default',
          title: selection === '' ? undefined : selection,
        }, t('harnessDefault')))
      }
      const chips = []
      for (const provider of harness.providers) {
        for (const model of provider.models) {
          chips.push(h('span', {
            className: 'tmx-modelChip',
            key: `chip:${provider.name}:${model.id}:${model.protocol}`,
            title: [provider.name || provider.sourceProvider, model.protocol].filter(part => part !== '').join(' · '),
          }, [
            h('span', { className: 'tmx-dot', 'data-tone': tone, 'aria-hidden': 'true', key: 'dot' }),
            model.id,
          ]))
        }
      }
      chips.push(h('button', {
        type: 'button',
        className: 'tmx-addModel',
        key: 'add',
        onClick: options.onAddModel,
      }, `+ ${t('harnessAddModel')}`))
      // Apply stays available on every row: an applied Harness may still need
      // a reapply to repair actual-file drift. A pending row keeps the primary
      // treatment; an applied row uses the secondary treatment.
      const applyButton = h('button', {
        type: 'button',
        className: 'tmx-button tmx-applyButton',
        'data-kind': applied ? undefined : 'primary',
        key: 'apply',
        disabled: options.busy || options.readBlocked,
        onClick: () => options.onApply(harness),
      }, applied ? t('harnessReapply') : t('harnessApply'))
      const meta = [harness.id, harness.agent === '' ? t('unknown') : harness.agent, modelCountText(harness.modelCount, t)]
        .filter(part => part !== '')
        .join(' · ')
      return h('div', {
        className: 'tmx-harnessGroup',
        key: `harness:${harness.id}`,
        'data-harness': harness.id,
        'data-unapplied': harness.unapplied === null ? 'unknown' : String(harness.unapplied),
      }, [
        h('div', { className: 'tmx-harnessRow', key: 'head' }, [
          harnessMark(harness),
          h('div', { className: 'tmx-harnessInfo', key: 'info' }, [
            h('span', { className: 'tmx-harnessName', key: 'name' }, name),
            h('span', { className: 'tmx-harnessMeta', key: 'meta' }, meta),
          ]),
          h('div', { className: 'tmx-harnessActions', key: 'actions' }, [
            ...pills,
            applyButton,
          ]),
        ]),
        h('div', { className: 'tmx-modelChips', key: 'chips' }, chips),
        harness.blockers.length === 0
          ? null
          : h('p', { className: 'tmx-errorText tmx-harnessNote', key: 'blockers' },
            harness.blockers.map(blocker => hostCodeText(blocker.code, blocker.message)).filter(text => text !== '').join(' · ')),
        applied && reloadPending
          ? h('p', { className: 'tmx-harnessNote', key: 'note' }, [
            h('strong', { className: 'tmx-harnessWritten', key: 'written' }, t('harnessWritten')),
            ` · ${t('harnessReopenPrefix')} ${name} ${t('harnessReopenSuffix')}`,
          ])
          : null,
      ])
    }

    /** One compact row per declared Harness; no invented Agent is offered. */
    function harnessSection(state, t, harnesses, options) {
      if (harnesses.length === 0) {
        // A failed Harness read is already named above; it is not an empty
        // declaration list, so the page draws no empty-state claim here.
        const failed = sectionErrors(state).some(error => error.section === 'harnesses')
        if (failed) return null
        return h('section', { className: 'tmx-section tmx-harnessSection', key: 'harness' },
          h('p', { className: 'tmx-empty', key: 'empty' }, t('harnessNone')))
      }
      return h('section', { className: 'tmx-section tmx-harnessSection', key: 'harness' },
        h('div', { className: 'tmx-harnessList', key: 'list' },
          harnesses.map(harness => harnessRow(harness, t, options))))
    }

    function applyPreviewFiles(data, t) {
      const files = Array.isArray(data?.files) ? data.files.filter(file => file !== null && typeof file === 'object') : []
      if (files.length === 0) return null
      return h('div', { className: 'tmx-applyFiles', key: 'files' },
        files.slice(0, 12).map((file, index) => h('div', { className: 'tmx-applyFile', key: index }, [
          h('span', { className: 'tmx-applyFileGlyph', key: 'glyph', 'aria-hidden': 'true' }, '▤'),
          h('span', { className: 'tmx-applyFilePath', key: 'path' }, boundedText(file.path ?? t('unknown'), 160)),
          typeof file.format === 'string' && file.format !== ''
            ? h('span', { className: 'tmx-applyFileFormat', key: 'format' }, boundedText(file.format, 32))
            : null,
        ])))
    }

    /**
     * One Harness apply: the dry-run preview states the files and the model
     * count, an explicit confirmation runs the same target, and the refreshed
     * state redraws the row.
     */
    function applyDialog(apply, t, handlers) {
      if (apply === null || apply === undefined) return null
      if (apply.phase === 'idle' || apply.phase === 'applied') return null
      const harness = apply.harness !== null && typeof apply.harness === 'object' ? apply.harness : {}
      const name = harness.name !== undefined && harness.name !== '' ? harness.name : t('unknown')
      const previewing = apply.phase === 'previewing'
      const executing = apply.phase === 'executing'
      const failed = apply.phase === 'failed'
      const data = apply.data !== null && typeof apply.data === 'object' ? apply.data : {}
      const files = Array.isArray(data.files) ? data.files.filter(file => file !== null && typeof file === 'object') : []
      const lead = [modelCountText(harness.modelCount, t)]
      if (files.length > 0) lead.push(fileCountText(files.length, t))
      lead.push(t('harnessScope'))
      const blockers = Array.isArray(apply.result?.blockers) ? apply.result.blockers.filter(blocker => blocker !== null) : []
      const actions = []
      if (apply.phase === 'preview') {
        actions.push(h('button', { type: 'button', className: 'tmx-button', key: 'cancel', onClick: handlers.cancel }, t('applyCancel')))
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          'data-kind': 'primary',
          key: 'confirm',
          onClick: handlers.confirm,
        }, t('applyConfirm')))
      } else if (previewing || executing) {
        actions.push(h('button', {
          type: 'button',
          className: 'tmx-button',
          'data-kind': 'primary',
          key: 'busy',
          disabled: true,
          'data-busy': 'true',
        }, executing ? t('applyApplying') : t('applyPreviewing')))
      } else {
        actions.push(h('button', { type: 'button', className: 'tmx-button', key: 'close', onClick: handlers.cancel }, t('close')))
      }
      return h('div', { className: 'tmx-backdrop', key: 'applyBackdrop' },
        h('div', {
          className: 'tmx-dialog',
          role: 'dialog',
          'aria-modal': 'true',
          'aria-label': `${t('harnessApplyTo')} ${name}`,
        }, [
          h('div', { className: 'tmx-dialogHead', key: 'head' }, [
            h('h2', { className: 'tmx-dialogTitle', key: 'title' }, `${t('harnessApplyTo')} ${name}`),
            h('button', {
              type: 'button',
              className: 'tmx-dialogClose',
              key: 'close',
              'aria-label': t('close'),
              'data-autofocus': 'true',
              disabled: executing,
              onClick: handlers.cancel,
            }, '×'),
          ]),
          h('div', { className: 'tmx-dialogBody', key: 'body' }, [
            h('p', { className: 'tmx-dialogLead', key: 'lead' }, lead.join(' · ')),
            typeof apply.result?.code === 'string' && apply.result.code !== ''
              ? h('p', { className: 'tmx-dialogMeta', key: 'meta' },
                `${boundedText(apply.result.code, 48)} · ${boundedText(apply.result.status ?? t('unknown'), 48)}`)
              : null,
            failed
              ? h('p', { className: 'tmx-errorText tmx-break', key: 'error' },
                `${t('applyFailed')}${t('colon')} ${boundedText(apply.message, 200)}`)
              : null,
            applyPreviewFiles(data, t),
            apply.phase === 'preview' && effectLines(data.effects, t).length > 0
              ? h('dl', { className: 'tmx-previewFacts', key: 'effects' }, [
                h('dt', { key: 'label' }, t('connectEffects')),
                h('dd', { key: 'value' }, effectLines(data.effects, t).join(' · ')),
              ])
              : null,
            blockers.length === 0
              ? null
              : h('ul', { className: 'tmx-dialogBlockers', key: 'blockers' },
                blockers.slice(0, 8).map((blocker, index) => h('li', { className: 'tmx-errorText tmx-break', key: index },
                  hostCodeText(blocker?.code, blocker?.message) || entryText(blocker)))),
            apply.phase === 'preview'
              ? h('p', { className: 'tmx-dialogNote', key: 'note' }, t('harnessApplyNote'))
              : null,
          ]),
          h('div', { className: 'tmx-dialogActions', key: 'actions' }, actions),
        ]))
    }

    function gatewayFacts(gateway, t) {
      const load = gateway?.sharedKey?.loadedState
      const key = load === 'loaded' ? t('keyLoaded') : load === 'absent' ? t('keyAbsent') : t('unknown')
      const configured = gateway?.configured === true ? t('gatewayConfigured') : gateway?.configured === false ? t('gatewayUnconfigured') : t('unknown')
      return h('dl', { className: 'tmx-facts', key: 'facts' }, [
        h('div', { className: 'tmx-fact', key: 'address' }, [
          h('dt', { key: 'label' }, t('gatewayListen')),
          h('dd', { key: 'value' }, boundedText(gateway?.listen ?? t('notReported'), 120)),
        ]),
        h('div', { className: 'tmx-fact', key: 'key' }, [
          h('dt', { key: 'label' }, t('gatewayKey')),
          h('dd', { key: 'value' }, key),
        ]),
        h('div', { className: 'tmx-fact', key: 'configured' }, [
          h('dt', { key: 'label' }, t('gatewayConfig')),
          h('dd', { key: 'value' }, configured),
        ]),
      ])
    }

    function gatewaySection(state, t, handlers, busy) {
      const gateway = state.gateway !== null && typeof state.gateway === 'object' ? state.gateway : null
      const ready = gateway?.ready
      const tone = ready === true ? 'ok' : ready === false ? 'warn' : 'idle'
      const status = gateway === null
        ? t('unknown')
        : gateway.configured === false
          ? t('gatewayUnconfigured')
          : ready === true
            ? t('gatewayReady')
            : ready === false
              ? t('gatewayNotReady')
              : t('unknown')
      const start = gateway !== null && gateway.configured === true && ready !== true
        ? h('button', {
          type: 'button',
          className: 'tmx-button',
          key: 'start',
          onClick: () => handlers.run({ action: 'gateway_start' }),
          disabled: busy,
        }, t('actionStartGateway'))
        : null
      const reason = gateway?.reasonCode !== null && gateway?.reasonCode !== undefined
        ? h('p', { className: 'tmx-note tmx-gatewayReason', key: 'reason' }, `${t('gatewayReason')}${t('colon')} ${boundedText(gateway.reasonCode, 80)}`)
        : null
      return h('section', { className: 'tmx-section', key: 'gateway' }, [
        h('div', { className: 'tmx-card tmx-gatewayCard', key: 'card' }, [
          h('div', { className: 'tmx-gatewayHead', key: 'head' }, [
            h('span', { className: 'tmx-dot', key: 'dot', 'data-tone': tone }),
            h('h2', { className: 'tmx-gatewayTitle', key: 'title' }, t('gatewayTitle')),
            h('span', { className: 'tmx-gatewayStatus', key: 'status' }, status),
          ]),
          gatewayFacts(gateway, t),
          reason,
          start === null ? null : h('div', { className: 'tmx-gatewayActions', key: 'actions' }, start),
        ]),
      ])
    }

    /** Skeleton cards while the first read is in flight: never a spinner in the content. */
    function skeleton(t) {
      return h('div', { className: 'tmx-accounts', key: 'skeleton', role: 'status', 'aria-label': t('checking') },
        [0, 1, 2, 3].map(index => h('div', { className: 'tmx-account', 'aria-hidden': 'true', key: index }, [
          h('span', { className: 'tmx-skeletonBar tmx-skeletonName', key: 'name' }),
          h('span', { className: 'tmx-skeletonBar', key: 'bar' }),
          h('span', { className: 'tmx-skeletonBar', key: 'second' }),
        ])))
    }

    function staleBanner(load, t, retry) {
      if (load.phase !== 'failed' || load.value === undefined) return null
      // A refused read model's own section failures are evidence too: name them
      // beside the last-known data instead of dropping them with the payload.
      const failures = sectionErrors(load)
      return h('div', { className: 'tmx-banner', key: 'stale' }, [
        h('span', { className: 'tmx-errorText', key: 'text' },
          `${t('hostError')}${t('colon')} ${describeError(load.message, t)} · ${t('staleNotice')}`),
        ...failures.map((error, index) => h('span', { className: 'tmx-errorText tmx-break', key: `failure:${index}` },
          `${t('readFailed')}${t('colon')} ${error.text}`)),
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

    function chip(text, tone) {
      return h('span', { className: 'tmx-chip', 'data-tone': tone }, text)
    }

    function word(value, t) {
      return value === true ? t('yes') : value === false ? t('no') : t('unknown')
    }

    function windowsFor(provider, t) {
      const readings = (Array.isArray(provider?.billing?.rows) ? provider.billing.rows : [])
        .map((row, index) => windowReading(row, index, t))
        .filter(Boolean)
      return disambiguate(readings)
    }


    // ---------------------------------------------------------- request history
    //
    // History is a retained, bounded, process-scoped record. The page reads it
    // only after an explicit click, says what it covers and what is missing, and
    // reports an unavailable read as unavailable rather than an empty list.

    function observePanel(observe, t, loadFailed) {
      if (observe === null || observe === undefined || observe.phase === 'idle' || observe.phase === 'running') return null
      // A completed or partially refused observation names only the accounts it
      // could not read; every account it did is already on its card.
      const missed = observe.items.filter(item => item.tone !== 'ok')
      if (missed.length > 0) {
        return h('p', { className: 'tmx-errorText', key: 'observe', role: 'status' }, [
          `${t('observeFailed')}${t('colon')} `,
          ...missed.map((item, index) => h('span', { className: 'tmx-break', key: item.key },
            `${index === 0 ? '' : ' · '}${boundedText(item.id === '' ? item.source : item.id, 80)}${item.detail === '' ? '' : ` (${item.detail})`}`)),
        ])
      }
      // A refused request failed the read as well, and the banner names that
      // failure once; repeating it here would be the same sentence twice.
      if (observe.phase !== 'failed' || loadFailed || observe.message === undefined) return null
      return h('p', { className: 'tmx-errorText', key: 'observe', role: 'status' }, `${t('observeFailed')}${t('colon')} ${describeError(observe.message, t)}`)
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

    /**
     * The one line a screen reader is told after each step. A failed step names
     * its typed detail; a completed step says so, because a reader who cannot
     * see the row change must still learn that it did.
     */
    function announce({ load, observe, repair, selection, apply }, t) {
      if (load.schemaUnsupported === true) return t('stateSchemaUnsupported')
      if (observe.phase === 'failed') {
        return observe.message === undefined
          ? t('observeFailed')
          : `${t('observeFailed')}${t('colon')} ${describeError(observe.message, t)}`
      }
      if (observe.phase === 'running') return t('observeRunning')
      if (selection !== null && selection !== undefined && selection.phase === 'failed') {
        return selection.message === undefined
          ? t('selectionFailed')
          : `${t('selectionFailed')}${t('colon')} ${boundedText(selection.message, 160)}`
      }
      if (apply !== null && apply !== undefined && apply.phase === 'failed') {
        return apply.message === undefined
          ? t('applyFailed')
          : `${t('applyFailed')}${t('colon')} ${boundedText(apply.message, 160)}`
      }
      if (repair !== null && repair !== undefined && repair.phase === 'failed') {
        return repair.message === undefined
          ? t('actionFailed')
          : `${t('actionFailed')}${t('colon')} ${describeError(repair.message, t)}`
      }
      if (load.phase === 'failed' && load.message !== undefined) {
        return `${t('hostError')}${t('colon')} ${describeError(load.message, t)}`
      }
      if (selection !== null && selection !== undefined && selection.phase === 'running') return t('actionWorking')
      if (apply !== null && apply !== undefined && (apply.phase === 'previewing' || apply.phase === 'executing')) {
        return t('actionWorking')
      }
      if (repair !== null && repair !== undefined && (repair.phase === 'previewing' || repair.phase === 'executing')) {
        return t('actionWorking')
      }
      if (apply !== null && apply !== undefined && apply.phase === 'preview') return t('applyPreviewTitle')
      if (apply !== null && apply !== undefined && apply.phase === 'applied') return t('applyApplied')
      if (selection !== null && selection !== undefined && selection.phase === 'saved') return t('selectionSaved')
      if (repair !== null && repair !== undefined && repair.phase === 'preview') return t('actionPreviewTitle')
      if (repair !== null && repair !== undefined && repair.phase === 'applied') {
        const code = typeof repair.result?.code === 'string' ? boundedText(repair.result.code, 48) : ''
        return code === '' ? t('actionDone') : `${t('actionDone')}${t('colon')} ${code}`
      }
      if (observe.phase === 'done') {
        const missed = observe.items.filter(item => item.tone !== 'ok').length
        return missed === 0 ? t('observeDone') : `${t('observeFailed')}${t('colon')} ${missed}`
      }
      return ''
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
    // The DSH-native seats (sidebar marker, Settings > Models card, tool result
    // card) read the same host read model as the
    // page. They share one short-lived GET and never post, never observe, and
    // never run an action on mount.

    /** How long one cached read model serves the seats before an explicit reload. */
    const SNAPSHOT_TTL_MS = 10_000
    /** Latest shared seats read: `{ at, result?, pending? }`. */
    let snapshotCache

    /** Record a read model the page or a seat just adopted, so the seats agree. */
    function rememberState(state) {
      if (state === null || typeof state !== 'object' || state.schemaVersion !== STATE_SCHEMA_VERSION || state.ok !== true) return
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
          if (value.schemaVersion !== STATE_SCHEMA_VERSION) return { schemaUnsupported: true, message: String(value.schemaVersion) }
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

    // ----------------------------------------------------------- Seat components

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

    /**
     * Settings > Models: whether tmax models are usable, which declared DSH
     * Harness owns them, and how many of its Harnesses are pending. Applying is
     * the page's Harness tab, so this card never offers a legacy Apply; the one
     * action that changes configuration is the link back to tmax.
     */
    function ModelsFooter(props) {
      const t = props.t
      const [load, snapshot] = useSeatSnapshot(t)
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
      // The settings page is DSH's, so it reports the DSH Harnesses. A failed
      // Harness section is unknown, not an empty declaration list or a zero
      // pending count.
      const harnessReadFailed = read !== undefined && errorsForSection(read, 'harnesses').length > 0
      const harnesses = read === undefined || harnessReadFailed ? undefined : harnessesOf(read)
      const dshHarnesses = harnesses === undefined ? [] : harnesses.filter(harness => harness.agent === 'dsh')
      const unappliedDsh = dshHarnesses.filter(harness => harness.unapplied === true)
      // One uninspectable DSH row makes the exact pending count unknown even
      // when the list itself read; only a fully known set can print a number.
      const unappliedDshText = harnesses === undefined || dshHarnesses.some(harness => harness.unapplied === null)
        ? t('unknown')
        : unappliedDsh.length
      const harnessText = harnesses === undefined
        ? t('unknown')
        : dshHarnesses.length === 0
          ? t('none')
          : dshHarnesses.map(harness => harness.id).join(', ')
      const reloadable = load.phase === 'failed'
        || (read !== undefined && (() => {
          const pageAction = nextAction({ phase: 'ready', value: read })
          return pageAction !== undefined && pageAction.kind === 'retry'
        })())

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
      facts.push(`${t('footerHarness')}${t('colon')} ${harnessText}`)
      facts.push(`${t('footerUnapplied')}${t('colon')} ${unappliedDshText}`)
      // Seat data is one shared GET; say when the read model was generated so a
      // mounted card cannot look newer than the evidence behind it.
      const generatedAt = read === undefined ? '' : clockOf(read.generatedAt)
      if (generatedAt !== '') facts.push(`${t('updated')} ${generatedAt}`)
      const pageAvailable = typeof props.hasLayout === 'function' ? props.hasLayout() === true : false
      const pageUnavailable = pageAvailable !== true || openFailed
      const actions = []
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
        title: pageUnavailable ? t('footerPageUnavailable') : t('footerOpenHint'),
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
      body.push(h('div', { className: 'tmx-seatActions', key: 'actions' }, actions))
      return h('div', { className: 'tmx-seatCard', 'data-tmax-footer': '1', key: 'card' }, body)
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
      const routeText = routeCount === undefined
        ? t('unknown')
        : `${routeCount}${summary.routeNames.length === 0 ? '' : ` (${summary.routeNames.join(', ')})`}`
      const providerText = providerCount === undefined ? t('unknown') : `${providerCount} tmax`
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
      formatReset,
      compact,
      windowReading,
      disambiguate,
      stateGate,
      sectionErrors,
      errorsForSection,
      projectionFailed,
      projectionErrors,
      sectionErrorText,
      resetQualified,
      resetUnqualified,
      unqualifiedResetAt,
      resetAt,
      usableRoutes,
      modelCatalogOf,
      accountsOf,
      windowTone,
      nextAction,
      safeDashboard,
      lastErrorFact,
      observeItems,
      failureIsActive,
      providerActions,
      actionPayload,
      actionSucceeded,
      actionResultLines,
      markerStatus,
      modelsAvailability,
      toolStatusSummary,
      harnessesOf,
      harnessTone,
      harnessStatusKey,
      harnessLabelMap,
      harnessMembershipOf,
      unappliedHarnesses,
      countSelections,
      agentName,
      agentShort,
      locales: { en, zh },
    }

    return { inject: ['slots', 'locale'], apply, __pure }
  },
})
