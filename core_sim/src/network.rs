use std::io::{self, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{unbounded, Sender};

use crate::snapshot::SnapshotHistory;

pub struct SnapshotServer {
    sender: Sender<Vec<u8>>,
}

impl SnapshotServer {
    pub fn broadcast(&self, bytes: &[u8]) {
        if let Err(err) = self.sender.send(bytes.to_vec()) {
            log::error!("Failed to queue snapshot delta: {}", err);
        }
    }
}

pub fn start_snapshot_server(bind_addr: std::net::SocketAddr) -> Option<SnapshotServer> {
    let listener = match TcpListener::bind(bind_addr) {
        Ok(listener) => listener,
        Err(err) => {
            log::warn!(
                "Snapshot server bind failed at {}: {}. Broadcasting disabled.",
                bind_addr,
                err
            );
            return None;
        }
    };

    let (sender, receiver) = unbounded::<Vec<u8>>();
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

    Some(SnapshotServer { sender })
}

pub fn broadcast_latest(server: Option<&SnapshotServer>, history: &SnapshotHistory) {
    if let (Some(server), Some(bytes)) = (server, history.encoded_delta.clone()) {
        server.broadcast(&bytes);
    }
}

fn broadcast_frame(clients: &Arc<Mutex<Vec<TcpStream>>>, frame: &[u8]) {
    let mut guard = clients.lock().expect("clients mutex poisoned");
    guard.retain_mut(|stream| {
        let len = frame.len() as u32;
        let mut buffer = Vec::with_capacity(4 + frame.len());
        buffer.extend_from_slice(&len.to_le_bytes());
        buffer.extend_from_slice(frame);
        match stream.write_all(&buffer) {
            Ok(_) => true,
            Err(err) => {
                log::warn!("Dropping snapshot client: {}", err);
                false
            }
        }
    });
}
