// One bounded, argv-only invocation path to the tmax CLI.
//
// Every host read or action goes through here: an argument array (never a
// shell string), a per-call timeout, a bounded stdout buffer, and exactly one
// JSON envelope. The tmax CLI prints its JSON envelope on stdout even when the
// exit code is nonzero, so a failed read still yields a typed code, human
// message, and blockers instead of a lost transcript.
import { execFile } from 'node:child_process'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

export const READ_TIMEOUT_MS = 15_000
export const INVENTORY_TIMEOUT_MS = 45_000
export const REFRESH_TIMEOUT_MS = 45_000
// `agent apply` writes native files through tmax's own recovery and lock
// sequence, so it gets the same deadline a persisted repair does.
export const CONNECT_TIMEOUT_MS = 45_000
export const MAX_OUTPUT_BYTES = 2 * 1024 * 1024

/** A tmax read or action failed with a typed code; the message is bounded and secret-free. */
export class TmaxError extends Error {
  constructor(message, code = 'tmax_failed') {
    super(message)
    this.name = 'TmaxError'
    this.code = code
  }
}

/** Resolved DSH home: explicit plugin config, environment, then the OS default. */
export function resolveHome(config = {}, env = process.env) {
  if (typeof config.home === 'string' && config.home !== '') return config.home
  // A whitespace-only DSH_HOME is unset, not a path made of spaces.
  const fromEnvironment = typeof env.DSH_HOME === 'string' ? env.DSH_HOME.trim() : ''
  if (fromEnvironment !== '') return fromEnvironment
  return join(homedir(), '.dsh')
}

/** The configured target id, or undefined when none is named. */
export function explicitTarget(config = {}) {
  if (typeof config.target === 'string') {
    const trimmed = config.target.trim()
    return trimmed === '' ? undefined : trimmed
  }
  if (config.target !== undefined && config.target !== null && config.target !== '') return String(config.target)
  return undefined
}

/** The first declared blocker's message, or the envelope's own message/code. */
export function envelopeMessage(envelope) {
  if (envelope === undefined || envelope === null) return 'tmax did not answer'
  const blocker = Array.isArray(envelope.blockers) ? envelope.blockers[0] : undefined
  if (blocker !== undefined && typeof blocker.message === 'string' && blocker.message !== '') return blocker.message
  if (typeof envelope.message === 'string' && envelope.message !== '') return envelope.message
  return `tmax returned ${envelope.code || 'an unknown error'}`
}

/** Whether an envelope reports a completed read (not merely a dry-run preview). */
export function isSuccess(envelope) {
  return envelope?.code === 'ok' && envelope?.status === 'success'
}

function parseEnvelope(text) {
  if (typeof text !== 'string' || text.trim() === '') return undefined
  try {
    const parsed = JSON.parse(text.trim())
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return undefined
    return parsed
  } catch {
    return undefined
  }
}

function boundedFailure(error) {
  if (error === undefined || error === null) return 'tmax did not answer'
  if (typeof error.code === 'string' && error.code === 'ETIMEDOUT') return 'tmax did not answer before the timeout'
  if (error.killed === true) return 'tmax was stopped after exceeding its timeout or output buffer'
  if (typeof error.message === 'string' && error.message !== '') {
    // execFile embeds the command and argv in the first line. Drop that line
    // entirely when it is the wrapper, keeping only tmax's own diagnostic text
    // so no binary/home path reaches an HTTP body or the command output.
    const lines = error.message.split('\n')
    const first = lines[0]
    const line = /^Command failed:/u.test(first)
      ? (lines.slice(1).find(entry => entry.trim() !== '') ?? 'tmax could not be executed')
      : first
    return line.length > 300 ? `${line.slice(0, 300)}…` : line
  }
  return 'tmax could not be executed'
}

/**
 * Run one tmax command and parse its envelope.
 * @returns {{envelope: object|undefined, exitCode: number|undefined, failure: string|undefined}}
 */
export async function runTmax(config, args, { timeoutMs = READ_TIMEOUT_MS, env = process.env } = {}) {
  const tmax = config.tmaxPath || 'tmax'
  try {
    const { stdout } = await execFileAsync(tmax, args, {
      timeout: timeoutMs,
      maxBuffer: MAX_OUTPUT_BYTES,
      env,
    })
    const envelope = parseEnvelope(stdout)
    if (envelope === undefined) {
      return { envelope: undefined, exitCode: 0, failure: 'tmax returned output that is not one JSON envelope' }
    }
    return { envelope, exitCode: 0, failure: undefined }
  } catch (error) {
    const stdout = typeof error?.stdout === 'string' ? error.stdout : ''
    const envelope = parseEnvelope(stdout)
    if (envelope !== undefined) {
      const exitCode = typeof error?.code === 'number' ? error.code : 1
      return { envelope, exitCode, failure: undefined }
    }
    const exitCode = typeof error?.code === 'number' ? error.code : undefined
    return { envelope: undefined, exitCode, failure: boundedFailure(error) }
  }
}

/**
 * Read one command as a typed result; failures are data, not exceptions.
 * @returns {{ok: boolean, code: string, status: string, message: string|undefined, data: any, envelope: object|undefined, exitCode: number|undefined}}
 */
export async function readEnvelope(config, args, options) {
  const result = await runTmax(config, args, options)
  if (result.envelope === undefined) {
    return {
      ok: false,
      code: 'tmax_unavailable',
      status: 'error',
      message: result.failure,
      data: undefined,
      envelope: undefined,
      exitCode: result.exitCode,
    }
  }
  return {
    ok: isSuccess(result.envelope),
    code: typeof result.envelope.code === 'string' ? result.envelope.code : 'unknown',
    status: typeof result.envelope.status === 'string' ? result.envelope.status : 'unknown',
    message: isSuccess(result.envelope) ? undefined : envelopeMessage(result.envelope),
    data: result.envelope.data,
    envelope: result.envelope,
    exitCode: result.exitCode,
  }
}

/** A route's declared credential reference, or undefined when it cannot be used. */
export function usableCredentialRef(route) {
  const ref = route?.credential_ref
  return typeof ref === 'string' && ref !== '' ? ref : undefined
}

/** A route's declared base URL, or undefined when it cannot be used. */
export function usableBaseUrl(route) {
  const baseUrl = route?.base_url
  return typeof baseUrl === 'string' && baseUrl !== '' ? baseUrl : undefined
}
