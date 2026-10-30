use crate::{Error, Result};
use fs2::FileExt;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

/// The one OS-user-owned tmax data directory. Discovery never consults HOME.
#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
    pub home: PathBuf,
}

impl Store {
    pub fn discover() -> Result<Self> {
        Ok(Self::at(os_home()?))
    }
    pub(crate) fn at(home: PathBuf) -> Self {
        Self {
            root: home.join(".tmax"),
            home,
        }
    }

    pub fn path(&self, name: &str) -> Result<PathBuf> {
        let path = Path::new(name);
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::new(
                "invalid_storage_path",
                "Expected a relative tmax-owned storage name",
            ));
        }
        Ok(self.root.join(path))
    }

    /// Only an absent file has a default. Corruption and access failures remain errors.
    pub fn read_json<T: DeserializeOwned + Default>(&self, name: &str) -> Result<T> {
        let path = self.path(name)?;
        match read_optional(&path)? {
            None => Ok(T::default()),
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
                Error::new(
                    "storage_unavailable",
                    "Stored metadata is malformed; preserve it and repair the affected store",
                )
            }),
        }
    }

    pub fn write_json<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        let bytes = serde_json::to_vec(value)
            .map_err(|_| Error::new("storage_unavailable", "Unable to encode stored metadata"))?;
        self.write_raw(name, &bytes)
    }

    pub fn write_raw(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path(name)?;
        self.ensure_root()?;
        atomic_write(&path, bytes)
    }

    pub fn lock(&self, name: &str) -> Result<LockGuard> {
        let file = self.lock_file(name)?;
        FileExt::lock_exclusive(&file).map_err(|_| storage_error())?;
        Ok(LockGuard(file))
    }

    pub fn try_lock(&self, name: &str) -> Result<Option<LockGuard>> {
        let file = self.lock_file(name)?;
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Some(LockGuard(file))),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                Ok(None)
            }
            Err(_) => Err(storage_error()),
        }
    }

    /// Share an existing publication lock without initializing or changing storage.
    /// Absence is reported explicitly; it is not a reservation against a first writer.
    pub fn read_lock(&self, name: &str) -> Result<Option<LockGuard>> {
        let Some(file) = self.existing_lock_file(name)? else {
            return Ok(None);
        };
        FileExt::lock_shared(&file).map_err(|_| storage_error())?;
        Ok(Some(LockGuard(file)))
    }

    /// Observe an existing exclusive lock without waiting, creating files, or changing permissions.
    /// The returned state is an observation, not a reservation for later work.
    pub fn is_locked(&self, name: &str) -> Result<Option<bool>> {
        let Some(file) = self.existing_lock_file(name)? else {
            return Ok(None);
        };
        match FileExt::try_lock_shared(&file) {
            Ok(()) => {
                FileExt::unlock(&file).map_err(|_| storage_error())?;
                Ok(Some(false))
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                Ok(Some(true))
            }
            Err(_) => Err(storage_error()),
        }
    }

    fn existing_lock_file(&self, name: &str) -> Result<Option<File>> {
        let path = self.path(&format!("locks/{name}.lock"))?;
        check_ancestors(&path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        nofollow(&mut options);
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(storage_error()),
        };
        let metadata = file.metadata().map_err(|_| storage_error())?;
        if !metadata.is_file() {
            return Err(storage_error());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != unsafe { libc::geteuid() } {
                return Err(storage_error());
            }
        }
        Ok(Some(file))
    }

    fn lock_file(&self, name: &str) -> Result<File> {
        let path = self.path(&format!("locks/{name}.lock"))?;
        self.ensure_root()?;
        ensure_directory(path.parent().ok_or_else(storage_error)?)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        secure_options(&mut options);
        let file = options.open(&path).map_err(|_| storage_error())?;
        if !file.metadata().map_err(|_| storage_error())?.is_file() {
            return Err(storage_error());
        }
        restrict_file(&file, &path)?;
        Ok(file)
    }

    fn ensure_root(&self) -> Result<()> {
        ensure_directory(&self.root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata = fs::symlink_metadata(&self.root).map_err(|_| storage_error())?;
            if metadata.uid() != unsafe { libc::geteuid() } {
                return Err(storage_error());
            }
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))
                .map_err(|_| storage_error())?;
        }
        #[cfg(windows)]
        restrict_windows(&self.root)?;
        Ok(())
    }
}

pub struct LockGuard(File);
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub(crate) fn storage_error() -> Error {
    Error::new(
        "storage_unavailable",
        "Local storage is unavailable; check ownership, permissions, and available disk space",
    )
}

/// Open a restricted regular file for append without reading or replacing its contents.
pub(crate) fn open_append(path: &Path) -> Result<File> {
    ensure_directory(path.parent().ok_or_else(storage_error)?)?;
    check_ancestors(path)?;
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    secure_options(&mut options);
    let file = options.open(path).map_err(|_| storage_error())?;
    if !file.metadata().map_err(|_| storage_error())?.is_file() {
        return Err(storage_error());
    }
    restrict_file(&file, path)?;
    Ok(file)
}

pub(crate) fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    read_optional_bounded(path, usize::MAX)
}

pub(crate) fn read_optional_bounded(path: &Path, limit: usize) -> Result<Option<Vec<u8>>> {
    check_ancestors(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    nofollow(&mut options);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(storage_error()),
    };
    let metadata = file.metadata().map_err(|_| storage_error())?;
    if !metadata.is_file() {
        return Err(storage_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(storage_error());
        }
    }
    if metadata.len() > limit as u64 {
        return Err(Error::new(
            "storage_limit_exceeded",
            "Stored metadata exceeds its supported size",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| storage_error())?;
    if bytes.len() > limit {
        return Err(Error::new(
            "storage_limit_exceeded",
            "Stored metadata exceeds its supported size",
        ));
    }
    Ok(Some(bytes))
}

/// Replace a file with a restricted, fully written same-directory temporary file.
/// Callers serialize related read/modify/write operations with Store::lock.
/// A failure after rename may mean the new bytes are visible but not durable.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(storage_error)?;
    ensure_directory(parent)?;
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => return Err(storage_error()),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(storage_error()),
    }
    let temporary = parent.join(format!(".tmax-write-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    secure_options(&mut options);
    let result = (|| {
        let mut file = options.open(&temporary).map_err(|_| storage_error())?;
        restrict_file(&file, &temporary)?;
        file.write_all(bytes).map_err(|_| storage_error())?;
        file.sync_all().map_err(|_| storage_error())?;
        drop(file);
        replace_file(&temporary, path)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Publish fully written bytes only if the destination is absent. Never replace it.
/// Returns false when another directory entry already occupies the destination.
pub(crate) fn atomic_create(path: &Path, bytes: &[u8]) -> Result<bool> {
    let parent = path.parent().ok_or_else(storage_error)?;
    ensure_directory(parent)?;
    check_ancestors(path)?;
    let temporary = parent.join(format!(".tmax-write-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    secure_options(&mut options);
    let mut file = options.open(&temporary).map_err(|_| storage_error())?;
    let result = (|| {
        restrict_file(&file, &temporary)?;
        file.write_all(bytes).map_err(|_| storage_error())?;
        file.sync_all().map_err(|_| storage_error())?;
        drop(file);
        match fs::hard_link(&temporary, path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(_) => Err(storage_error()),
        }
    })();
    let cleanup = fs::remove_file(&temporary).map_err(|_| storage_error());
    let created = result?;
    cleanup?;
    sync_directory(parent)?;
    Ok(created)
}

#[cfg(not(windows))]
pub(crate) fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).map_err(|_| storage_error())
}

#[cfg(windows)]
pub(crate) fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Replace the old directory entry and request a durable move on this same volume.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) } == 0 {
        return Err(storage_error());
    }
    Ok(())
}

pub(crate) fn ensure_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.is_dir() && !meta.file_type().is_symlink() {
                check_ancestors(path)
            } else {
                Err(storage_error())
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .ok_or_else(storage_error)?;
            ensure_directory(parent)?;
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match builder.create(path) {
                Ok(()) => {
                    #[cfg(windows)]
                    restrict_windows(path)?;
                    sync_directory(parent)
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => ensure_directory(path),
                Err(_) => Err(storage_error()),
            }
        }
        Err(_) => Err(storage_error()),
    }
}

pub(crate) fn check_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(storage_error()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(storage_error()),
        }
    }
    Ok(())
}

fn nofollow(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
}
fn secure_options(options: &mut OpenOptions) {
    nofollow(options);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
}
fn restrict_file(file: &File, path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = file.metadata().map_err(|_| storage_error())?;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(storage_error());
        }
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| storage_error())?;
        let _ = path;
    }
    #[cfg(windows)]
    {
        let _ = file;
        restrict_windows(path)?;
    }
    Ok(())
}
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|_| storage_error())?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(unix)]
fn os_home() -> Result<PathBuf> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let mut size = 4096;
    loop {
        let mut buffer = vec![0u8; size];
        let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                passwd.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && size < 1024 * 1024 {
            size *= 2;
            continue;
        }
        if status != 0 || result.is_null() {
            return Err(Error::new(
                "home_unavailable",
                "Unable to resolve the operating-system user's home directory",
            ));
        }
        let passwd = unsafe { passwd.assume_init() };
        if passwd.pw_dir.is_null() {
            return Err(storage_error());
        }
        let bytes = unsafe { std::ffi::CStr::from_ptr(passwd.pw_dir) }.to_bytes();
        let home = PathBuf::from(OsString::from_vec(bytes.to_vec()));
        if !home.is_absolute() {
            return Err(storage_error());
        }
        return Ok(home);
    }
}

#[cfg(windows)]
fn os_home() -> Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    #[repr(C)]
    struct Guid {
        a: u32,
        b: u16,
        c: u16,
        d: [u8; 8],
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHGetKnownFolderPath(
            id: *const Guid,
            flags: u32,
            token: *mut std::ffi::c_void,
            path: *mut *mut u16,
        ) -> i32;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoTaskMemFree(ptr: *mut std::ffi::c_void);
    }
    let profile = Guid {
        a: 0x5e6c858f,
        b: 0x0e22,
        c: 0x4760,
        d: [0x9a, 0xfe, 0xea, 0x33, 0x17, 0xb6, 0x71, 0x73],
    };
    let mut raw = std::ptr::null_mut();
    if unsafe { SHGetKnownFolderPath(&profile, 0, std::ptr::null_mut(), &mut raw) } < 0
        || raw.is_null()
    {
        return Err(storage_error());
    }
    let mut len = 0;
    unsafe {
        while *raw.add(len) != 0 {
            len += 1;
        }
    }
    let home = PathBuf::from(std::ffi::OsString::from_wide(unsafe {
        std::slice::from_raw_parts(raw, len)
    }));
    unsafe {
        CoTaskMemFree(raw.cast());
    }
    if !home.is_absolute() {
        return Err(storage_error());
    }
    Ok(home)
}

#[cfg(windows)]
fn restrict_windows(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text: *const u16,
            revision: u32,
            descriptor: *mut *mut std::ffi::c_void,
            size: *mut u32,
        ) -> i32;
        fn SetFileSecurityW(
            path: *const u16,
            information: u32,
            descriptor: *mut std::ffi::c_void,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(ptr: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
    let text: Vec<u16> = "D:P(A;;FA;;;OW)\0".encode_utf16().collect();
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(storage_error());
    }
    let success = unsafe { SetFileSecurityW(name.as_ptr(), 0x80000004, descriptor) };
    unsafe {
        LocalFree(descriptor);
    }
    if success == 0 {
        return Err(storage_error());
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn os_home() -> Result<PathBuf> {
    Err(Error::new(
        "home_unavailable",
        "This operating system has no supported home-directory resolver",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_defaults_but_malformed_does_not() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        assert_eq!(
            store.read_json::<Vec<String>>("state.json").unwrap(),
            Vec::<String>::new()
        );
        assert!(!store.root.exists());
        store
            .write_raw("state.json", b"{not-json: secret}")
            .unwrap();
        let error = store.read_json::<Vec<String>>("state.json").unwrap_err();
        assert!(!error.message.contains("secret"));
    }
    #[test]
    fn replacement_and_locks_are_atomic_resources() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        store.write_json("state.json", &vec![1]).unwrap();
        store.write_json("state.json", &vec![2, 3]).unwrap();
        assert_eq!(store.read_json::<Vec<i32>>("state.json").unwrap(), [2, 3]);
        let guard = store.lock("state").unwrap();
        assert!(store.try_lock("state").unwrap().is_none());
        drop(guard);
        assert!(store.try_lock("state").unwrap().is_some());
        assert!(store.write_raw("../outside", b"no").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(store.root.join("state.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symbolic_link_authorities() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        let outside = home.path().join("outside");
        fs::write(&outside, b"keep").unwrap();
        fs::create_dir(&store.root).unwrap();
        std::os::unix::fs::symlink(&outside, store.root.join("state.json")).unwrap();
        assert!(store.write_raw("state.json", b"replace").is_err());
        assert!(store.read_json::<Vec<String>>("state.json").is_err());
        assert_eq!(fs::read(outside).unwrap(), b"keep");
    }

    #[test]
    fn shared_reads_never_initialize_storage() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        assert!(store.read_lock("routing").unwrap().is_none());
        assert!(!store.root.exists());
        drop(store.lock("routing").unwrap());
        let first = store.read_lock("routing").unwrap().unwrap();
        let second = store.read_lock("routing").unwrap().unwrap();
        assert!(store.try_lock("routing").unwrap().is_none());
        drop(first);
        assert!(store.try_lock("routing").unwrap().is_none());
        drop(second);
        assert!(store.try_lock("routing").unwrap().is_some());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = store.root.join("locks/routing.lock");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            drop(store.read_lock("routing").unwrap().unwrap());
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
    }

    #[test]
    fn acquisition_probe_never_waits_or_initializes_storage() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        assert_eq!(store.is_locked("input-acquisition").unwrap(), None);
        assert!(!store.root.exists());
        let acquisition = store.lock("input-acquisition").unwrap();
        assert_eq!(store.is_locked("input-acquisition").unwrap(), Some(true));
        drop(acquisition);
        assert_eq!(store.is_locked("input-acquisition").unwrap(), Some(false));
        assert!(store.try_lock("input-acquisition").unwrap().is_some());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = store.root.join("locks/input-acquisition.lock");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            assert_eq!(store.is_locked("input-acquisition").unwrap(), Some(false));
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
    }
}
