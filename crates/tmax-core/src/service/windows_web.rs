//! Observe actual management-listener release separately from response draining.
use std::{io, net::SocketAddr};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
};

pub(super) struct Listener {
    inner: Option<TcpListener>,
    closed: watch::Sender<bool>,
}
impl Listener {
    pub(super) fn new(inner: TcpListener) -> (Self, watch::Receiver<bool>) {
        let (closed, receiver) = watch::channel(false);
        (
            Self {
                inner: Some(inner),
                closed,
            },
            receiver,
        )
    }
}
impl axum::serve::Listener for Listener {
    type Io = TcpStream;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        axum::serve::Listener::accept(self.inner.as_mut().expect("Web listener")).await
    }
    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.as_ref().expect("Web listener").local_addr()
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        drop(self.inner.take());
        let _ = self.closed.send_replace(true);
    }
}

pub(super) fn open_browser(url: &str) -> bool {
    let Some(authority) = url.strip_prefix("http://") else {
        return false;
    };
    let Ok(address) = authority.parse::<SocketAddr>() else {
        return false;
    };
    if !address.ip().is_loopback() || address.port() == 0 || authority != address.to_string() {
        return false;
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> *mut std::ffi::c_void;
    }
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        ) as isize
            > 32
    }
}
