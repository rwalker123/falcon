use std::io::{self, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{unbounded, Sender};

use crate::snapshot::FrameSink;

pub struct SnapshotServer {
    /// Frames ride as the `Arc` the encoder produced. They used to be copied into a fresh `Vec`
    /// per broadcast — a whole frame memcpy'd on the turn thread for nothing, since the sender
    /// already owned a shared, immutable buffer.
    sender: Sender<Arc<Vec<u8>>>,
}

impl SnapshotServer {
    pub fn broadcast(&self, bytes: &Arc<Vec<u8>>) {
        if let Err(err) = self.sender.send(Arc::clone(bytes)) {
            log::error!("Failed to queue snapshot delta: {}", err);
        }
    }
}

/// The snapshot socket is where the publisher thread's frames go (`snapshot::publish`).
impl FrameSink for SnapshotServer {
    fn publish_frame(&self, frame: &Arc<Vec<u8>>) {
        self.broadcast(frame);
    }
}

/// Starts the snapshot broadcaster on an already-bound listener.
///
/// The listener is bound up front by `port_alloc::allocate`, so binding can no
/// longer fail here — a busy port is caught before the server starts rather
/// than silently disabling broadcasting on a running server.
///
/// A newly accepted client is deliberately sent **nothing** until the next
/// broadcast: any cached frame belongs to whatever world existed at accept time,
/// which is not necessarily the world the connecting client asked for, and the
/// client cannot tell the two apart — so the server must not offer the guess.
pub fn start_snapshot_server(listener: TcpListener) -> SnapshotServer {
    let (sender, receiver) = unbounded::<Arc<Vec<u8>>>();
    listener
        .set_nonblocking(true)
        .expect("set nonblocking failed");
    let clients: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    let accept_clients = Arc::clone(&clients);

    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                log::info!("Snapshot client connected: {}", addr);
                if let Err(err) = stream.set_nodelay(true) {
                    log::warn!("Failed to set TCP_NODELAY: {}", err);
                }
                if let Err(err) = stream.set_nonblocking(false) {
                    log::warn!(
                        "Failed to set blocking mode for snapshot client {}: {}",
                        addr,
                        err
                    );
                }
                accept_clients
                    .lock()
                    .expect("clients mutex poisoned")
                    .push(stream);
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(err) => {
                log::error!("Error accepting snapshot client: {}", err);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        }

        while let Ok(frame) = receiver.try_recv() {
            broadcast_frame(&clients, &frame);
        }
    });

    SnapshotServer { sender }
}

fn write_frame(stream: &mut TcpStream, frame: &[u8]) -> io::Result<()> {
    let len = frame.len() as u32;
    let mut buffer = Vec::with_capacity(4 + frame.len());
    buffer.extend_from_slice(&len.to_le_bytes());
    buffer.extend_from_slice(frame);
    stream.write_all(&buffer)
}

fn broadcast_frame(clients: &Arc<Mutex<Vec<TcpStream>>>, frame: &[u8]) {
    let mut guard = clients.lock().expect("clients mutex poisoned");
    guard.retain_mut(|stream| match write_frame(stream, frame) {
        Ok(_) => true,
        Err(err) => {
            log::warn!("Dropping snapshot client: {}", err);
            false
        }
    });
}
