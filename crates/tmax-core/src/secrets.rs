use crate::{
    Error, Request, Result,
    storage::{self, Store},
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
};
use zeroize::Zeroizing;

const VERSION: u32 = 1;
const MAX_INPUT: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    kind: String,
    identity: String,
    id: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    version: u32,
    reference: String,
    created_at: String,
    owner: Owner,
    kind: String,
    recovery: Option<Recovery>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recovery {
    config_file: String,
    original_revision: String,
    byte_count: u64,
    rejection_category: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    metadata: Metadata,
    algorithm: String,
    kdf: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

fn locked() -> Error {
    Error::new(
        "credential_unavailable",
        "Protected storage is locked or the supplied unlock value is incorrect; supply --unlock-stdin or a separate inherited pipe with --unlock-fd",
    )
}
fn unavailable() -> Error {
    Error::new(
        "credential_unavailable",
        "Protected storage or its ownership metadata is unavailable; preserve the affected entry and repair storage before using or deleting it",
    )
}
fn input_error() -> Error {
    Error::new(
        "unsupported_protected_input",
        "Use a readable inherited pipe or supported hidden terminal input; regular files, shared descriptors, and secret arguments are not supported",
    )
}

/// Resolve unlock input only for an operation which actually needs a credential.
pub fn unlock(request: &Request) -> Result<Option<Zeroizing<Vec<u8>>>> {
    input_selection(request, "unlock")?;
    check_input_separation(request)?;
    if request.dry_run {
        return Ok(None);
    }
    if has(request, "unlock-stdin") || has(request, "unlock-fd") {
        return Ok(Some(Zeroizing::new(read_protected_input(
            request, "unlock",
        )?)));
    }
    if has(request, "non-interactive") {
        return Ok(None);
    }
    match read_terminal("Unlock protected tmax storage: ") {
        Ok(bytes) => Ok(Some(Zeroizing::new(bytes))),
        Err(error) if error.code == "interactive_required" => Ok(None),
        Err(error) => Err(error),
    }
}

/// A single explicit UTF-8 line. No inspection or dry-run consumes input.
pub fn read_protected_input(request: &Request, prefix: &str) -> Result<Vec<u8>> {
    let selected = input_selection(request, prefix)?;
    check_input_separation(request)?;
    if request.dry_run {
        return Ok(Vec::new());
    }
    match selected {
        Some(fd) => read_descriptor(fd, fd == 0),
        None if !has(request, "non-interactive") => read_terminal("Enter protected tmax value: "),
        None => Err(Error::new(
            "interactive_required",
            "Supply explicit protected stdin or inherited pipe input in non-interactive mode",
        )),
    }
}

fn has(request: &Request, key: &str) -> bool {
    request.arguments.contains_key(key)
}
fn input_selection(request: &Request, prefix: &str) -> Result<Option<i32>> {
    let stdin = format!("{prefix}-stdin");
    let fd = format!("{prefix}-fd");
    if has(request, &stdin) && has(request, &fd) {
        return Err(Error::new(
            "invalid_arguments",
            "Protected stdin and descriptor input are mutually exclusive",
        )
        .field(prefix));
    }
    if has(request, &stdin) {
        return Ok(Some(0));
    }
    if let Some(values) = request.arguments.get(&fd) {
        if values.len() != 1 {
            return Err(Error::new(
                "invalid_arguments",
                "Expected exactly one nonnegative descriptor number",
            )
            .field(fd));
        }
        return values[0]
            .parse::<i32>()
            .ok()
            .filter(|fd| *fd >= 0)
            .map(Some)
            .ok_or_else(|| {
                Error::new(
                    "invalid_arguments",
                    "Expected a nonnegative descriptor number",
                )
                .field(fd)
            });
    }
    Ok(None)
}
fn check_input_separation(request: &Request) -> Result<()> {
    if let (Some(unlock), Some(key)) = (
        input_selection(request, "unlock")?,
        input_selection(request, "key")?,
    ) && (unlock == key || descriptors_alias(unlock, key)?)
    {
        return Err(Error::new(
            "invalid_arguments",
            "Key and unlock input must use separate protected streams",
        ));
    }
    Ok(())
}

pub fn put(store: &Store, owner: &str, value: &[u8], unlock: Option<&[u8]>) -> Result<String> {
    let unlock = unlock
        .filter(|value| !value.is_empty())
        .ok_or_else(locked)?;
    let _lock = store.lock("secrets")?;
    let owner = resolve_owner(store, owner)?;
    write_envelope(store, "secrets", owner, "secret", None, value, unlock)
}

pub fn get(store: &Store, reference: &str, unlock: Option<&[u8]>) -> Result<Zeroizing<Vec<u8>>> {
    let unlock = unlock
        .filter(|value| !value.is_empty())
        .ok_or_else(locked)?;
    let envelope = load(store, "secrets", reference)?;
    decrypt(&envelope, unlock)
}

pub fn remove(store: &Store, reference: &str) -> Result<()> {
    let _lock = store.lock("secrets")?;
    let _ = load(store, "secrets", reference)?;
    remove_envelope(store, "secrets", reference)
}

/// The inventory is safe while locked, including unreferenced entries left by a failed save.
pub fn inventory(store: &Store) -> Result<Value> {
    list_envelopes(store, "secrets", false)
}

pub fn protect_recovery(store: &Store, bytes: &[u8], unlock: Option<&[u8]>) -> Result<String> {
    use sha2::{Digest, Sha256};
    let unlock = unlock
        .filter(|value| !value.is_empty())
        .ok_or_else(locked)?;
    let _lock = store.lock("recovery")?;
    let recovery = Recovery {
        config_file: "tmax.toml".into(),
        original_revision: format!("{:x}", Sha256::digest(bytes)),
        byte_count: bytes.len() as u64,
        rejection_category: "invalid_configuration".into(),
    };
    write_envelope(
        store,
        "recovery",
        Owner {
            kind: "config".into(),
            identity: "tmax-config".into(),
            id: None,
        },
        "config-recovery",
        Some(recovery),
        bytes,
        unlock,
    )
}
pub fn recovery_list(store: &Store) -> Result<Value> {
    list_envelopes(store, "recovery", true)
}
pub fn recovery_inspect(store: &Store, reference: &str) -> Result<Value> {
    let envelope = load(store, "recovery", reference)?;
    Ok(recovery_metadata(&envelope))
}
pub fn recovery_discard(store: &Store, reference: &str) -> Result<()> {
    let _lock = store.lock("recovery")?;
    let _ = load(store, "recovery", reference)?;
    remove_envelope(store, "recovery", reference)
}

fn write_envelope(
    store: &Store,
    directory: &str,
    owner: Owner,
    kind: &str,
    recovery: Option<Recovery>,
    bytes: &[u8],
    unlock: &[u8],
) -> Result<String> {
    let reference = format!(
        "{}{}",
        if directory == "recovery" {
            "recovery:"
        } else {
            "secret:"
        },
        uuid::Uuid::new_v4()
    );
    let metadata = Metadata {
        version: VERSION,
        reference: reference.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        owner,
        kind: kind.into(),
        recovery,
    };
    let mut salt = [0u8; 32];
    let mut nonce = [0u8; 12];
    OsRng.try_fill_bytes(&mut salt).map_err(|_| unavailable())?;
    OsRng
        .try_fill_bytes(&mut nonce)
        .map_err(|_| unavailable())?;
    let key = derive_key(unlock, &salt)?;
    let aad = serde_json::to_vec(&metadata).map_err(|_| unavailable())?;
    let cipher = ChaCha20Poly1305::new_from_slice(key.as_slice()).map_err(|_| unavailable())?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: bytes,
                aad: &aad,
            },
        )
        .map_err(|_| unavailable())?;
    let envelope = Envelope {
        metadata,
        algorithm: "chacha20poly1305".into(),
        kdf: "argon2id-v19-m65536-t3-p1".into(),
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    };
    store.write_json(&entry_name(directory, &reference)?, &envelope)?;
    Ok(reference)
}
fn derive_key(unlock: &[u8], salt: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let mut key = Zeroizing::new(vec![0u8; 32]);
    let params = Params::new(65536, 3, 1, Some(32)).map_err(|_| unavailable())?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(unlock, salt, &mut key)
        .map_err(|_| unavailable())?;
    Ok(key)
}
fn decrypt(envelope: &Envelope, unlock: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let salt = STANDARD.decode(&envelope.salt).map_err(|_| unavailable())?;
    let nonce = STANDARD
        .decode(&envelope.nonce)
        .map_err(|_| unavailable())?;
    let ciphertext = STANDARD
        .decode(&envelope.ciphertext)
        .map_err(|_| unavailable())?;
    if salt.len() != 32 || nonce.len() != 12 {
        return Err(unavailable());
    }
    let key = derive_key(unlock, &salt)?;
    let cipher = ChaCha20Poly1305::new_from_slice(key.as_slice()).map_err(|_| unavailable())?;
    let aad = serde_json::to_vec(&envelope.metadata).map_err(|_| unavailable())?;
    cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| locked())
}
fn entry_name(directory: &str, reference: &str) -> Result<String> {
    let prefix = if directory == "recovery" {
        "recovery:"
    } else {
        "secret:"
    };
    let suffix = reference.strip_prefix(prefix).ok_or_else(|| {
        Error::new(
            "invalid_reference",
            "Expected an opaque reference owned by this protected store",
        )
    })?;
    let id = uuid::Uuid::parse_str(suffix)
        .map_err(|_| Error::new("invalid_reference", "Invalid protected-store reference"))?;
    if id.to_string() != suffix {
        return Err(Error::new(
            "invalid_reference",
            "Invalid protected-store reference",
        ));
    }
    Ok(format!("{directory}/{suffix}.json"))
}
fn load(store: &Store, directory: &str, reference: &str) -> Result<Envelope> {
    let path = store.path(&entry_name(directory, reference)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = fs::symlink_metadata(&path).map_err(|_| unavailable())?;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(unavailable());
        }
    }
    let bytes = storage::read_optional(&path)?.ok_or_else(unavailable)?;
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    validate_envelope(&envelope, directory, reference)?;
    Ok(envelope)
}
fn validate_envelope(envelope: &Envelope, directory: &str, reference: &str) -> Result<()> {
    let metadata = &envelope.metadata;
    if metadata.version != VERSION
        || metadata.reference != reference
        || envelope.algorithm != "chacha20poly1305"
        || envelope.kdf != "argon2id-v19-m65536-t3-p1"
        || chrono::DateTime::parse_from_rfc3339(&metadata.created_at).is_err()
    {
        return Err(unavailable());
    }
    let salt = STANDARD.decode(&envelope.salt).map_err(|_| unavailable())?;
    let nonce = STANDARD
        .decode(&envelope.nonce)
        .map_err(|_| unavailable())?;
    let ciphertext = STANDARD
        .decode(&envelope.ciphertext)
        .map_err(|_| unavailable())?;
    if salt.len() != 32 || nonce.len() != 12 || ciphertext.len() < 16 {
        return Err(unavailable());
    }
    let owner = &metadata.owner;
    if !safe_label(&owner.identity) || owner.id.as_deref().is_some_and(|id| !safe_label(id)) {
        return Err(unavailable());
    }
    if directory == "recovery" {
        if metadata.kind != "config-recovery"
            || owner.kind != "config"
            || owner.identity != "tmax-config"
            || metadata.recovery.is_none()
        {
            return Err(unavailable());
        }
    } else if metadata.kind != "secret"
        || !matches!(owner.kind.as_str(), "provider" | "agent" | "proxy")
        || (owner.kind == "proxy" && (owner.identity != "proxy" || owner.id.is_some()))
        || metadata.recovery.is_some()
    {
        return Err(unavailable());
    }
    Ok(())
}
fn remove_envelope(store: &Store, directory: &str, reference: &str) -> Result<()> {
    let path = store.path(&entry_name(directory, reference)?)?;
    fs::remove_file(&path).map_err(|_| unavailable())?;
    storage::sync_directory(path.parent().ok_or_else(unavailable)?)
}

fn list_envelopes(store: &Store, directory: &str, recovery: bool) -> Result<Value> {
    let path = store.path(directory)?;
    if storage::check_ancestors(&path).is_err() {
        return Ok(
            json!({"coverage":"unavailable", "unavailable_scopes":[directory], "entries":[]}),
        );
    }
    let iterator = match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({"coverage":"complete", "unavailable_scopes":[], "entries":[]}));
        }
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => fs::read_dir(&path),
        _ => {
            return Ok(
                json!({"coverage":"unavailable", "unavailable_scopes":[directory], "entries":[]}),
            );
        }
    };
    let iterator = match iterator {
        Ok(iterator) => iterator,
        Err(_) => {
            return Ok(
                json!({"coverage":"unavailable", "unavailable_scopes":[directory], "entries":[]}),
            );
        }
    };
    let mut entries = Vec::new();
    let mut unavailable_scopes = Vec::new();
    let owners = if recovery {
        Some(BTreeMap::new())
    } else {
        match read_owners(store) {
            Ok(owners) => Some(owners),
            Err(_) => {
                unavailable_scopes.push("secret-owners.json".into());
                None
            }
        }
    };
    for item in iterator {
        let item = match item {
            Ok(item) => item,
            Err(_) => {
                unavailable_scopes.push(directory.to_owned());
                continue;
            }
        };
        let filename = item.file_name();
        let Some(filename) = filename.to_str() else {
            unavailable_scopes.push(directory.to_owned());
            continue;
        };
        if filename.starts_with(".tmax-write-") {
            continue;
        }
        let Some(id) = filename.strip_suffix(".json") else {
            unavailable_scopes.push(directory.to_owned());
            continue;
        };
        let reference = format!("{}{id}", if recovery { "recovery:" } else { "secret:" });
        if entry_name(directory, &reference).is_err() {
            unavailable_scopes.push(directory.to_owned());
            continue;
        }
        match load(store, directory, &reference) {
            Ok(mut envelope) => {
                if !recovery {
                    if let Some(owners) = &owners {
                        if let Some(current) = owners.values().find(|owner| {
                            owner.kind == envelope.metadata.owner.kind
                                && owner.identity == envelope.metadata.owner.identity
                        }) {
                            envelope.metadata.owner.id = current.id.clone();
                        }
                    } else {
                        envelope.metadata.owner.id = None;
                    }
                }
                entries.push(if recovery { recovery_metadata(&envelope) } else { json!({"reference":reference,"created_at":envelope.metadata.created_at,"metadata_state":"known","owner":envelope.metadata.owner}) });
            }
            Err(_) => {
                unavailable_scopes.push(reference.clone());
                entries.push(json!({"reference":reference,"created_at":null,"metadata_state":"unavailable","owner":null}));
            }
        }
    }
    entries.sort_by(|a, b| a["reference"].as_str().cmp(&b["reference"].as_str()));
    unavailable_scopes.sort();
    unavailable_scopes.dedup();
    Ok(
        json!({"coverage":if unavailable_scopes.is_empty() {"complete"} else {"partial"},"unavailable_scopes":unavailable_scopes,"entries":entries}),
    )
}
fn recovery_metadata(envelope: &Envelope) -> Value {
    let recovery = envelope.metadata.recovery.as_ref();
    json!({"reference":envelope.metadata.reference,"created_at":envelope.metadata.created_at,"metadata_state":"known","owner":envelope.metadata.owner,"config_file":recovery.map(|r| &r.config_file),"original_revision":recovery.map(|r| &r.original_revision),"byte_count":recovery.map(|r| r.byte_count),"rejection_category":recovery.map(|r| &r.rejection_category),"protection":{"state":"encrypted","unlock_required":true,"algorithm":"chacha20poly1305","kdf":"argon2id"},"capture_state":"retained","discard_supported":true})
}
fn safe_label(label: &str) -> bool {
    !label.is_empty() && label.len() <= 512 && !label.chars().any(char::is_control)
}
fn read_owners(store: &Store) -> Result<BTreeMap<String, Owner>> {
    let owners: BTreeMap<String, Owner> = store.read_json("secret-owners.json")?;
    for (key, owner) in &owners {
        let id = owner.id.as_deref().ok_or_else(unavailable)?;
        if !matches!(owner.kind.as_str(), "provider" | "agent")
            || !safe_label(&owner.identity)
            || !safe_label(id)
            || *key != format!("{}:{id}", owner.kind)
        {
            return Err(unavailable());
        }
    }
    Ok(owners)
}

pub fn owner_identity(store: &Store, kind: &str, id: &str) -> Result<Option<String>> {
    if !matches!(kind, "provider" | "agent") || !safe_label(id) {
        return Err(Error::new(
            "invalid_arguments",
            "Invalid local secret owner",
        ));
    }
    Ok(read_owners(store)?
        .get(&format!("{kind}:{id}"))
        .map(|owner| owner.identity.clone()))
}

/// Resolve a stable local owner tag without interpreting delimiters in its display ID.
/// This is a provisioning operation, not an inspection or dry-run helper.
pub fn owner_tag(store: &Store, kind: &str, id: &str) -> Result<String> {
    if !matches!(kind, "provider" | "agent") || !safe_label(id) {
        return Err(Error::new(
            "invalid_arguments",
            "Invalid local secret owner",
        ));
    }
    let _lock = store.lock("secrets")?;
    let mut owners = read_owners(store)?;
    let key = format!("{kind}:{id}");
    if let Some(owner) = owners.get(&key) {
        return Ok(format!("{kind}:{}:{id}", owner.identity));
    }
    let identity = uuid::Uuid::new_v4().to_string();
    owners.insert(
        key,
        Owner {
            kind: kind.into(),
            identity: identity.clone(),
            id: Some(id.into()),
        },
    );
    store.write_json("secret-owners.json", &owners)?;
    Ok(format!("{kind}:{identity}:{id}"))
}
fn resolve_owner(store: &Store, text: &str) -> Result<Owner> {
    if text == "proxy" {
        return Ok(Owner {
            kind: "proxy".into(),
            identity: "proxy".into(),
            id: None,
        });
    }
    let mut parts = text.splitn(3, ':');
    let kind = parts.next().unwrap_or("");
    let identity = parts.next().unwrap_or("");
    let id = parts.next();
    if !matches!(kind, "provider" | "agent")
        || !safe_label(identity)
        || id.is_some_and(|id| !safe_label(id))
    {
        return Err(Error::new(
            "invalid_arguments",
            "Secret owner must identify a provider, agent target, or sole proxy",
        ));
    }
    let mut owners = read_owners(store)?;
    let key = format!("{kind}:{}", id.unwrap_or(identity));
    let owner = if let Some(id) = id {
        Owner {
            kind: kind.into(),
            identity: identity.into(),
            id: Some(id.into()),
        }
    } else {
        owners.get(&key).cloned().unwrap_or_else(|| Owner {
            kind: kind.into(),
            identity: uuid::Uuid::new_v4().to_string(),
            id: Some(identity.into()),
        })
    };
    owners.retain(|_, existing| existing.kind != owner.kind || existing.identity != owner.identity);
    owners.insert(key, owner.clone());
    store.write_json("secret-owners.json", &owners)?;
    Ok(owner)
}

/// Update safe labels without decrypting or changing a stable local owner identity.
/// Immutable capture metadata authenticates ciphertext; the label registry supplies current display IDs.
pub fn rename_owner(store: &Store, kind: &str, old_id: &str, new_id: &str) -> Result<()> {
    if !matches!(kind, "provider" | "agent") || !safe_label(old_id) || !safe_label(new_id) {
        return Err(Error::new(
            "invalid_arguments",
            "Invalid local secret owner rename",
        ));
    }
    let _lock = store.lock("secrets")?;
    let mut owners = read_owners(store)?;
    let old_key = format!("{kind}:{old_id}");
    let new_key = format!("{kind}:{new_id}");
    if owners.contains_key(&new_key) {
        return Err(Error::new(
            "inputs_changed",
            "The new secret-owner label already identifies another local owner",
        ));
    }
    if let Some(mut owner) = owners.remove(&old_key) {
        owner.id = Some(new_id.into());
        owners.insert(new_key, owner);
    }
    // Display labels are maintained separately so a rename never invalidates ciphertext authentication.
    store.write_json("secret-owners.json", &owners)
}

/// Detach a removed resource's current label without deleting its retained encrypted entries.
/// Reusing the label then receives a new stable local identity.
pub fn detach_owner(store: &Store, kind: &str, id: &str) -> Result<()> {
    if !matches!(kind, "provider" | "agent") || !safe_label(id) {
        return Err(Error::new(
            "invalid_arguments",
            "Invalid local secret owner",
        ));
    }
    let _lock = store.lock("secrets")?;
    let mut owners = read_owners(store)?;
    if owners.remove(&format!("{kind}:{id}")).is_some() {
        store.write_json("secret-owners.json", &owners)?;
    }
    Ok(())
}

/// Qualified secret delivery channel. Construction never retrieves credentials.
pub struct ProtectedOutput {
    file: fs::File,
    terminal: bool,
    descriptor: Option<i32>,
}
impl ProtectedOutput {
    pub fn channel(&self) -> &'static str {
        if self.terminal { "terminal" } else { "pipe" }
    }
    pub fn descriptor(&self) -> Option<i32> {
        self.descriptor
    }
    pub fn deliver(&mut self, bytes: &[u8]) -> Result<()> {
        deliver(self, bytes)
    }
}
pub fn qualify_output(request: &Request) -> Result<ProtectedOutput> {
    if let Some(values) = request.arguments.get("output-fd") {
        if values.len() != 1 {
            return Err(output_error());
        }
        let fd = values[0]
            .parse::<i32>()
            .ok()
            .filter(|fd| *fd >= 3)
            .ok_or_else(output_error)?;
        qualify_pipe(fd, false).map_err(|_| output_error())?;
        if descriptors_alias(fd, 1).map_err(|_| output_error())?
            || descriptors_alias(fd, 2).map_err(|_| output_error())?
        {
            return Err(output_error());
        }
        if let Some(unlock) = input_selection(request, "unlock")?
            && (fd == unlock || descriptors_alias(fd, unlock).map_err(|_| output_error())?)
        {
            return Err(output_error());
        }
        return Ok(ProtectedOutput {
            file: duplicate_fd(fd).map_err(|_| output_error())?,
            terminal: false,
            descriptor: Some(fd),
        });
    }
    if has(request, "non-interactive") {
        return Err(Error::new(
            "interactive_required",
            "Secret delivery requires a qualified separate inherited pipe through --output-fd",
        ));
    }
    let file = open_terminal().map_err(|_| {
        Error::new(
            "interactive_required",
            "Secret delivery requires a controlling terminal or qualified separate inherited pipe",
        )
    })?;
    Ok(ProtectedOutput {
        file,
        terminal: true,
        descriptor: None,
    })
}
pub fn deliver(output: &mut ProtectedOutput, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty()
        || std::str::from_utf8(bytes).is_err()
        || bytes.iter().any(|b| *b == b'\n' || *b == b'\r')
    {
        return Err(unavailable());
    }
    output
        .file
        .write_all(bytes)
        .and_then(|_| output.file.write_all(b"\n"))
        .and_then(|_| output.file.flush())
        .map_err(|_| {
            Error::new(
                "secret_delivery_failed",
                "Protected secret delivery failed; delivery may have been partial",
            )
        })
}
fn output_error() -> Error {
    Error::new(
        "unsupported_protected_output",
        "Use a writable inherited pipe numbered at least 3, separate from stdout, stderr, and unlock input",
    )
}

#[cfg(unix)]
fn descriptors_alias(a: i32, b: i32) -> Result<bool> {
    let a = descriptor_stat(a)?;
    let b = descriptor_stat(b)?;
    Ok(a.st_dev == b.st_dev && a.st_ino == b.st_ino)
}
#[cfg(unix)]
fn descriptor_stat(fd: i32) -> Result<libc::stat> {
    let mut stat = std::mem::MaybeUninit::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
        return Err(input_error());
    }
    Ok(unsafe { stat.assume_init() })
}
#[cfg(unix)]
fn qualify_pipe(fd: i32, reading: bool) -> Result<()> {
    let stat = descriptor_stat(fd)?;
    if stat.st_mode & libc::S_IFMT != libc::S_IFIFO {
        return Err(input_error());
    }
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0
        || (reading && flags & libc::O_ACCMODE == libc::O_WRONLY)
        || (!reading && flags & libc::O_ACCMODE == libc::O_RDONLY)
    {
        return Err(input_error());
    }
    Ok(())
}
#[cfg(unix)]
fn duplicate_fd(fd: i32) -> Result<fs::File> {
    use std::os::fd::FromRawFd;
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if copy < 0 {
        return Err(input_error());
    }
    Ok(unsafe { fs::File::from_raw_fd(copy) })
}
#[cfg(unix)]
fn read_descriptor(fd: i32, permit_terminal: bool) -> Result<Vec<u8>> {
    if permit_terminal && unsafe { libc::isatty(fd) } == 1 {
        let mut file = duplicate_fd(fd)?;
        return read_hidden(&mut file);
    }
    qualify_pipe(fd, true)?;
    read_line(&mut duplicate_fd(fd)?)
}
fn read_line(reader: &mut impl Read) -> Result<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::new());
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => {
                if bytes.len() >= MAX_INPUT {
                    return Err(input_error());
                }
                bytes.push(byte[0]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(input_error()),
        }
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    if bytes.is_empty()
        || std::str::from_utf8(&bytes).is_err()
        || bytes.iter().any(|b| *b == 0 || *b == b'\r')
    {
        return Err(Error::new(
            "invalid_arguments",
            "Protected input must be one nonempty UTF-8 line",
        ));
    }
    Ok(std::mem::take(&mut *bytes))
}
#[cfg(unix)]
fn open_terminal() -> Result<fs::File> {
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open("/dev/tty")
        .map_err(|_| {
            Error::new(
                "interactive_required",
                "No supported controlling terminal is available",
            )
        })?;
    if unsafe { libc::isatty(file.as_raw_fd()) } != 1 {
        return Err(input_error());
    }
    Ok(file)
}
#[cfg(unix)]
fn read_hidden(file: &mut fs::File) -> Result<Vec<u8>> {
    use std::os::fd::AsRawFd;
    struct Restore {
        fd: i32,
        original: libc::termios,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
            }
        }
    }
    let fd = file.as_raw_fd();
    let mut original = std::mem::MaybeUninit::uninit();
    if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
        return Err(input_error());
    }
    let original = unsafe { original.assume_init() };
    let mut hidden = original;
    hidden.c_lflag &= !(libc::ECHO | libc::ECHONL);
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &hidden) } != 0 {
        return Err(input_error());
    }
    let restore = Restore { fd, original };
    let result = read_line(file);
    drop(restore);
    result
}
fn read_terminal(prompt: &str) -> Result<Vec<u8>> {
    let mut terminal = open_terminal()?;
    terminal
        .write_all(prompt.as_bytes())
        .and_then(|_| terminal.flush())
        .map_err(|_| input_error())?;
    let result = read_hidden(&mut terminal);
    let _ = terminal.write_all(b"\n");
    result
}

#[cfg(not(unix))]
fn descriptors_alias(_a: i32, _b: i32) -> Result<bool> {
    Err(input_error())
}
#[cfg(not(unix))]
fn qualify_pipe(_fd: i32, _reading: bool) -> Result<()> {
    Err(input_error())
}
#[cfg(not(unix))]
fn duplicate_fd(_fd: i32) -> Result<fs::File> {
    Err(input_error())
}
#[cfg(not(unix))]
fn read_descriptor(_fd: i32, _permit_terminal: bool) -> Result<Vec<u8>> {
    Err(input_error())
}
#[cfg(not(unix))]
fn open_terminal() -> Result<fs::File> {
    Err(Error::new(
        "interactive_required",
        "This platform requires a supported protected-input channel",
    ))
}
#[cfg(not(unix))]
fn read_hidden(_file: &mut fs::File) -> Result<Vec<u8>> {
    Err(input_error())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            command: vec![],
            operands: vec![],
            arguments: BTreeMap::new(),
            dry_run: false,
            if_revision: None,
        }
    }
    #[test]
    fn protected_input_rejects_ambiguity_without_consumption() {
        let mut request = request();
        request.dry_run = true;
        request
            .arguments
            .insert("key-stdin".into(), vec!["true".into()]);
        request
            .arguments
            .insert("unlock-stdin".into(), vec!["true".into()]);
        assert!(unlock(&request).is_err());
        request.arguments.remove("key-stdin");
        assert!(unlock(&request).unwrap().is_none());
        assert!(read_protected_input(&request, "unlock").unwrap().is_empty());
    }
    #[test]
    fn locked_creation_has_no_store_effects() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        assert!(put(&store, "proxy", b"test credential", None).is_err());
        assert!(!store.root.exists());
        assert_eq!(inventory(&store).unwrap()["coverage"], "complete");
        assert!(!store.root.exists());
    }
    #[test]
    fn ciphertext_and_metadata_commit_together_and_orphans_remain_discoverable() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        let reference = put(
            &store,
            "provider:stable-local-id:example",
            b"unmistakable-private-test-value",
            Some(b"test unlock"),
        )
        .unwrap();
        let bytes = fs::read(
            store
                .path(&entry_name("secrets", &reference).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("unmistakable-private-test-value"));
        let inventory = inventory(&store).unwrap();
        assert_eq!(inventory["entries"][0]["reference"], reference);
        assert_eq!(
            inventory["entries"][0]["owner"]["identity"],
            "stable-local-id"
        );
        rename_owner(&store, "provider", "example", "renamed").unwrap();
        assert_eq!(
            super::inventory(&store).unwrap()["entries"][0]["owner"]["id"],
            "renamed"
        );
        assert_eq!(
            owner_identity(&store, "provider", "renamed")
                .unwrap()
                .as_deref(),
            Some("stable-local-id")
        );
        assert_eq!(
            get(&store, &reference, Some(b"test unlock"))
                .unwrap()
                .as_slice(),
            b"unmistakable-private-test-value"
        );
        assert!(get(&store, &reference, Some(b"wrong unlock")).is_err());
        assert!(get(&store, &reference, None).is_err());
        let mut envelope = load(&store, "secrets", &reference).unwrap();
        let original_identity = envelope.metadata.owner.identity.clone();
        envelope.metadata.owner.identity = "different-local-identity".into();
        store
            .write_json(&entry_name("secrets", &reference).unwrap(), &envelope)
            .unwrap();
        assert!(get(&store, &reference, Some(b"test unlock")).is_err());
        envelope.metadata.owner.identity = original_identity;
        store
            .write_json(&entry_name("secrets", &reference).unwrap(), &envelope)
            .unwrap();
        remove(&store, &reference).unwrap();
        assert_eq!(
            super::inventory(&store).unwrap()["entries"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn malformed_metadata_is_partial_not_empty() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        let reference = format!("secret:{}", uuid::Uuid::new_v4());
        store
            .write_raw(
                &entry_name("secrets", &reference).unwrap(),
                b"broken private contents",
            )
            .unwrap();
        let inventory = inventory(&store).unwrap();
        assert_eq!(inventory["coverage"], "partial");
        assert_eq!(inventory["entries"][0]["reference"], reference);
        assert!(inventory["entries"][0]["owner"].is_null());
        assert!(!inventory.to_string().contains("private contents"));
        assert!(remove(&store, &reference).is_err());
    }
    #[test]
    fn recovery_retains_exact_non_utf8_bytes_but_inspection_never_decrypts() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        let bytes = b"invalid\xffsecret\0configuration";
        let reference = protect_recovery(&store, bytes, Some(b"test unlock")).unwrap();
        let envelope = load(&store, "recovery", &reference).unwrap();
        assert_eq!(
            decrypt(&envelope, b"test unlock").unwrap().as_slice(),
            bytes
        );
        let inspected = recovery_inspect(&store, &reference).unwrap();
        assert_eq!(inspected["byte_count"], bytes.len());
        assert!(!inspected.to_string().contains("secret"));
        assert_eq!(
            recovery_list(&store).unwrap()["entries"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        recovery_discard(&store, &reference).unwrap();
        assert!(recovery_inspect(&store, &reference).is_err());
    }

    #[test]
    fn line_input_preserves_utf8_and_rejects_invalid_values() {
        assert_eq!(
            read_line(&mut &b"protected-test\r\n"[..]).unwrap(),
            b"protected-test"
        );
        assert!(read_line(&mut &b"\n"[..]).is_err());
        assert!(read_line(&mut &b"bad\xff\n"[..]).is_err());
        assert!(read_line(&mut &b"bad\0value\n"[..]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn inherited_pipes_are_qualified_but_regular_files_are_not() {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut fds = [-1; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let mut reader = unsafe { fs::File::from_raw_fd(fds[0]) };
        let writer = unsafe { fs::File::from_raw_fd(fds[1]) };
        let mut request = request();
        request
            .arguments
            .insert("non-interactive".into(), vec!["true".into()]);
        request
            .arguments
            .insert("output-fd".into(), vec![writer.as_raw_fd().to_string()]);
        let mut output = qualify_output(&request).unwrap();
        assert_eq!(output.channel(), "pipe");
        assert_eq!(output.descriptor(), Some(writer.as_raw_fd()));
        output.deliver(b"synthetic-proxy-key").unwrap();
        drop(output);
        drop(writer);
        let mut received = Vec::new();
        reader.read_to_end(&mut received).unwrap();
        assert_eq!(received, b"synthetic-proxy-key\n");
        let file = tempfile::tempfile().unwrap();
        request
            .arguments
            .insert("output-fd".into(), vec![file.as_raw_fd().to_string()]);
        assert!(qualify_output(&request).is_err());
        request
            .arguments
            .insert("key-fd".into(), vec![file.as_raw_fd().to_string()]);
        assert!(read_protected_input(&request, "key").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn input_and_output_cannot_share_the_same_pipe() {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut fds = [-1; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let reader = unsafe { fs::File::from_raw_fd(fds[0]) };
        let writer = unsafe { fs::File::from_raw_fd(fds[1]) };
        let mut request = request();
        request.dry_run = true;
        request
            .arguments
            .insert("unlock-fd".into(), vec![reader.as_raw_fd().to_string()]);
        request
            .arguments
            .insert("key-fd".into(), vec![reader.as_raw_fd().to_string()]);
        assert!(unlock(&request).is_err());
        request.arguments.remove("key-fd");
        request
            .arguments
            .insert("output-fd".into(), vec![writer.as_raw_fd().to_string()]);
        assert!(qualify_output(&request).is_err());
    }

    #[test]
    fn detached_labels_do_not_reuse_a_removed_targets_identity() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        let first = owner_tag(&store, "agent", "target:with-colons").unwrap();
        assert_eq!(
            owner_tag(&store, "agent", "target:with-colons").unwrap(),
            first
        );
        let first_identity = owner_identity(&store, "agent", "target:with-colons")
            .unwrap()
            .unwrap();
        detach_owner(&store, "agent", "target:with-colons").unwrap();
        assert!(
            owner_identity(&store, "agent", "target:with-colons")
                .unwrap()
                .is_none()
        );
        let second = owner_tag(&store, "agent", "target:with-colons").unwrap();
        assert_ne!(first, second);
        assert_ne!(
            owner_identity(&store, "agent", "target:with-colons")
                .unwrap()
                .unwrap(),
            first_identity
        );
    }
}
