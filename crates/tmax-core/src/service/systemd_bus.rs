//! Minimal bindings to the systemd manager's native D-Bus client.
use super::super::unavailable;
use crate::Result;
use std::{
    ffi::{CStr, c_char, c_int, c_void},
    ptr::{self, NonNull},
    sync::LazyLock,
};

const MANAGER: &CStr = c"org.freedesktop.systemd1";
const CALL_TIMEOUT_MICROS: u64 = 10_000_000;

// Load only when using the Linux manager. Offline commands work without libsystemd.
macro_rules! native_api {
    ($($name:ident: $ty:ty),+ $(,)?) => {
        struct Api { $($name: $ty),+ }
        impl Api {
            fn get() -> Result<&'static Self> {
                static API: LazyLock<Option<Api>> = LazyLock::new(|| {
                    let library = unsafe { libc::dlopen(c"libsystemd.so.0".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
                    if library.is_null() {
                        return None;
                    }
                    let api = (|| Some(Api {
                        $($name: {
                            let symbol = unsafe { libc::dlsym(library, concat!(stringify!($name), "\0").as_ptr().cast()) };
                            if symbol.is_null() {
                                return None;
                            }
                            // The symbol signature is declared from sd-bus.h below. Keep the library
                            // open for the process lifetime on success.
                            unsafe { std::mem::transmute::<*mut c_void, $ty>(symbol) }
                        }),+
                    }))();
                    if api.is_none() {
                        unsafe { libc::dlclose(library) };
                    }
                    api
                });
                API.as_ref().ok_or_else(|| unavailable("The systemd client library is unavailable"))
            }
        }
    };
}

native_api! {
    sd_bus_new: unsafe extern "C" fn(*mut *mut c_void) -> c_int,
    sd_bus_unref: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    sd_bus_set_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    sd_bus_start: unsafe extern "C" fn(*mut c_void) -> c_int,
    sd_bus_get_fd: unsafe extern "C" fn(*mut c_void) -> c_int,
    sd_bus_call: unsafe extern "C" fn(*mut c_void, *mut c_void, u64, *mut c_void, *mut *mut c_void) -> c_int,
    sd_bus_message_new_method_call: unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *const c_char, *const c_char, *const c_char, *const c_char) -> c_int,
    sd_bus_message_unref: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    sd_bus_message_append_basic: unsafe extern "C" fn(*mut c_void, c_char, *const c_void) -> c_int,
    sd_bus_message_read_basic: unsafe extern "C" fn(*mut c_void, c_char, *mut c_void) -> c_int,
    sd_bus_message_enter_container: unsafe extern "C" fn(*mut c_void, c_char, *const c_char) -> c_int,
    sd_bus_message_exit_container: unsafe extern "C" fn(*mut c_void) -> c_int,
    sd_bus_message_skip: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
}

fn nonnegative(value: c_int, stage: &str) -> Result<c_int> {
    if value < 0 {
        Err(unavailable(stage))
    } else {
        Ok(value)
    }
}

pub(super) struct Bus {
    api: &'static Api,
    raw: NonNull<c_void>,
}
impl Drop for Bus {
    fn drop(&mut self) {
        unsafe { (self.api.sd_bus_unref)(self.raw.as_ptr()) };
    }
}
impl Bus {
    pub(super) fn connect(address: &CStr) -> Result<Self> {
        let stage = "The systemd private user socket could not be reached";
        let api = Api::get()?;
        let mut raw = ptr::null_mut();
        nonnegative(unsafe { (api.sd_bus_new)(&mut raw) }, stage)?;
        let bus = Self {
            api,
            raw: NonNull::new(raw).ok_or_else(|| unavailable(stage))?,
        };
        // An explicit address on a fresh sd_bus is peer-to-peer, not a message-bus client.
        nonnegative(
            unsafe { (api.sd_bus_set_address)(raw, address.as_ptr()) },
            stage,
        )?;
        nonnegative(unsafe { (api.sd_bus_start)(raw) }, stage)?;
        Ok(bus)
    }

    pub(super) fn fd(&self) -> Result<c_int> {
        nonnegative(
            unsafe { (self.api.sd_bus_get_fd)(self.raw.as_ptr()) },
            "The systemd private socket could not be identified",
        )
    }

    pub(super) fn call(
        &self,
        path: &CStr,
        interface: &CStr,
        method: &CStr,
        args: &[&CStr],
        stage: &'static str,
    ) -> Result<Message> {
        let api = self.api;
        let mut request = ptr::null_mut();
        nonnegative(
            unsafe {
                (api.sd_bus_message_new_method_call)(
                    self.raw.as_ptr(),
                    &mut request,
                    MANAGER.as_ptr(),
                    path.as_ptr(),
                    interface.as_ptr(),
                    method.as_ptr(),
                )
            },
            stage,
        )?;
        let request = Message::new(api, request, stage)?;
        for arg in args {
            nonnegative(
                unsafe {
                    (api.sd_bus_message_append_basic)(
                        request.raw.as_ptr(),
                        b's' as c_char,
                        arg.as_ptr().cast(),
                    )
                },
                stage,
            )?;
        }
        let mut reply = ptr::null_mut();
        nonnegative(
            unsafe {
                (api.sd_bus_call)(
                    self.raw.as_ptr(),
                    request.raw.as_ptr(),
                    CALL_TIMEOUT_MICROS,
                    ptr::null_mut(),
                    &mut reply,
                )
            },
            stage,
        )?;
        Message::new(api, reply, stage)
    }
}

pub(super) struct Message {
    api: &'static Api,
    raw: NonNull<c_void>,
    stage: &'static str,
}
impl Drop for Message {
    fn drop(&mut self) {
        unsafe { (self.api.sd_bus_message_unref)(self.raw.as_ptr()) };
    }
}
impl Message {
    fn new(api: &'static Api, raw: *mut c_void, stage: &'static str) -> Result<Self> {
        Ok(Self {
            api,
            raw: NonNull::new(raw).ok_or_else(|| unavailable(stage))?,
            stage,
        })
    }

    pub(super) fn enter(&self, kind: u8, signature: &CStr) -> Result<bool> {
        Ok(nonnegative(
            unsafe {
                (self.api.sd_bus_message_enter_container)(
                    self.raw.as_ptr(),
                    kind as c_char,
                    signature.as_ptr(),
                )
            },
            self.stage,
        )? != 0)
    }

    pub(super) fn begin(&self, kind: u8, signature: &CStr) -> Result<()> {
        if self.enter(kind, signature)? {
            Ok(())
        } else {
            Err(unavailable(self.stage))
        }
    }

    pub(super) fn end(&self) -> Result<()> {
        nonnegative(
            unsafe { (self.api.sd_bus_message_exit_container)(self.raw.as_ptr()) },
            self.stage,
        )
        .map(|_| ())
    }

    pub(super) fn skip(&self, signature: &CStr) -> Result<()> {
        if nonnegative(
            unsafe { (self.api.sd_bus_message_skip)(self.raw.as_ptr(), signature.as_ptr()) },
            self.stage,
        )? == 0
        {
            return Err(unavailable(self.stage));
        }
        Ok(())
    }

    // SAFETY: out must point to the representation of the specified D-Bus basic type.
    unsafe fn basic(&self, kind: u8, out: *mut c_void) -> Result<bool> {
        Ok(nonnegative(
            unsafe { (self.api.sd_bus_message_read_basic)(self.raw.as_ptr(), kind as c_char, out) },
            self.stage,
        )? != 0)
    }

    fn string(&self, kind: u8) -> Result<Option<&CStr>> {
        let mut value: *const c_char = ptr::null();
        if !unsafe { self.basic(kind, (&raw mut value).cast()) }? {
            return Ok(None);
        }
        if value.is_null() {
            return Err(unavailable(self.stage));
        }
        // sd-bus returns a NUL-terminated string owned by this message.
        Ok(Some(unsafe { CStr::from_ptr(value) }))
    }

    pub(super) fn path(&self) -> Result<&CStr> {
        self.string(b'o')?.ok_or_else(|| unavailable(self.stage))
    }

    pub(super) fn text(&self) -> Result<&str> {
        self.string(b's')?
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| unavailable(self.stage))
    }

    pub(super) fn strings(&self) -> Result<Vec<String>> {
        self.begin(b'a', c"s")?;
        let mut values = Vec::new();
        while let Some(value) = self.string(b's')? {
            values.push(
                value
                    .to_str()
                    .map_err(|_| unavailable(self.stage))?
                    .to_owned(),
            );
        }
        self.end()?;
        Ok(values)
    }

    pub(super) fn unsigned(&self) -> Result<u32> {
        let mut value = 0_u32;
        if !unsafe { self.basic(b'u', (&raw mut value).cast()) }? {
            return Err(unavailable(self.stage));
        }
        Ok(value)
    }

    fn signed_type(&self, kind: u8) -> Result<i32> {
        let mut value = 0_i32;
        if !unsafe { self.basic(kind, (&raw mut value).cast()) }? {
            return Err(unavailable(self.stage));
        }
        Ok(value)
    }

    pub(super) fn signed(&self) -> Result<i32> {
        self.signed_type(b'i')
    }
    pub(super) fn boolean(&self) -> Result<bool> {
        self.signed_type(b'b').map(|value| value != 0)
    }
}
