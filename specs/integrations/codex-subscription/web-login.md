# Codex browser sign-in

The local Web interface owns the sign-in interaction; the official Codex app
server owns OAuth, credential storage, and refresh. This extends the native
source selection in [the Codex contract](spec.md), not token import or a new
tmax OAuth client.

```text
Configure authorization → choose independent Codex scope or existing Codex target
  → start → open official sign-in page → waiting → authenticated → save reference
                ↘ cancel / timeout / failure → explain outcome and recovery
```

- An independent scope requires no pre-existing agent configuration. The explicit
  action creates a private Codex directory; its target and provider reference are
  saved only after successful authentication. An interrupted or failed attempt can
  explicitly reuse its retained private scope. Existing Codex installations keep
  their authorization. The resulting target can subsequently be configured for
  direct Codex use.
- Reauthentication through an existing target discloses its path and shared
  consumers. Before OAuth changes that scope, invalidate managed authorization
  and persist uncertain native-effect metadata. Failure is not rollback.
- An existing ambient login is never adopted. Known differing account email is
  rejected. Email and plan reported by Codex are metadata, not verified workspace
  identity, model entitlement, quota, or inference evidence.
- Polling returns starting, waiting, cancelling, succeeded, cancelled, expired,
  failed, interrupted, or incomplete after a saved result needs runtime recovery. Closing the dialog pauses observation; reopening resumes the pending
  attempt. Explicit cancellation stops it. Service shutdown interrupts it; a
  retained write-ahead record blocks uncertain native use after restart.
- A bounded attempt lasts at most ten minutes. Only one native authentication
  operation may run at a time. Duplicate starts for the same provider return the
  pending attempt. Cancellation racing completion either precedes commit or
  reports the committed result; it cannot undo a completed save.
- The browser receives only an official authorization URL, an optional device
  code, status and account metadata. No token or raw process output is returned.
  Credentials remain in the selected Codex store, with one refresh owner.
- A changed configuration revision prevents commit. A completed native sign-in
  followed by a failed save must report the retained native scope and the need
  to recover; it must not report success or overwrite concurrent edits.
- Browser and device-code flows share the same lifecycle. Device-code sign-in
  is an explicit alternative when a local callback cannot be used; its account
  settings and availability are determined by OpenAI.

## Design and reference

Magpie's `StartSignIn` exposes a pending attempt, URL/code, cancellation and final
result. Its Codex implementation exchanges OAuth codes itself, saves credentials
and coordinates refreshes. tmax adopts the interaction pattern while using the
official managed Codex authentication protocol so it does not become a second
refresh owner for native credentials.

- [Magpie sign-in implementation](https://github.com/yetone/magpie/blob/ed91aa04ef1f79bdc7edd28de33e26dca8e1a49e/internal/provider/signin.go)
- [Magpie refresh coordination](https://github.com/yetone/magpie/blob/ed91aa04ef1f79bdc7edd28de33e26dca8e1a49e/internal/provider/account.go)
- [Codex app-server authentication](https://learn.chatgpt.com/docs/app-server#authentication)

Protocol qualification is independent of native model-configuration qualification:
Codex CLI 0.159.2 exposes the required managed login/account protocol in its
generated JSON schema. Other versions require explicit qualification. No shell
wrapper, terminal parsing, credential export, or externally managed token mode
is used. A successful login alone does not qualify gateway forwarding.
