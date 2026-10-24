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
