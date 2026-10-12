use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::server::{RecordedRequest, ResponseScript, ScriptedServer};

pub struct ProxyTrap {
    server: ScriptedServer,
}

/// A loopback HTTP CONNECT proxy that tunnels every accepted destination to
/// one explicitly supplied loopback origin.
///
/// This is test infrastructure for fixed-HTTPS contract declarations. It
/// never resolves or connects to the requested public host.
pub struct ConnectProxy {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ConnectProxy {
    /// Starts a proxy forwarding CONNECT payloads to `upstream`.
    pub fn start(upstream: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("concord-connect-proxy".into())
            .spawn(move || connect_accept_loop(listener, upstream, worker_stop))?;
        Ok(Self {
            address,
            stop,
            worker: Some(worker),
        })
    }

    /// Returns the safe HTTP proxy origin.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for ConnectProxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("CONNECT proxy worker panicked");
        }
    }
}

fn connect_accept_loop(listener: TcpListener, upstream: SocketAddr, stop: Arc<AtomicBool>) {
    let mut workers = Vec::new();
    while !stop.load(Ordering::Acquire) {
        reap_finished_tunnels(&mut workers);
        match listener.accept() {
            Ok((client, _)) => {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let tunnel_stop = Arc::clone(&stop);
                workers.push(
                    thread::Builder::new()
                        .name("concord-connect-tunnel".into())
                        .spawn(move || {
                            if let Err(error) =
                                tunnel_connect(client, upstream, Arc::clone(&tunnel_stop))
                            {
                                // A bounded response can be dropped while the
                                // peer still writes TLS close-notify or a tail.
                                // Peer disconnects are normal fixture teardown;
                                // unexpected I/O and protocol errors still fail.
                                if !tunnel_stop.load(Ordering::Acquire) && !closed_peer(&error) {
                                    panic!("CONNECT proxy tunnel failed: {error}");
                                }
                            }
                        })
                        .expect("failed to spawn CONNECT tunnel worker"),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("CONNECT proxy accept failed: {error}"),
        }
    }
    for worker in workers {
        worker.join().expect("CONNECT tunnel worker panicked");
    }
}

fn reap_finished_tunnels(workers: &mut Vec<JoinHandle<()>>) {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            workers
                .swap_remove(index)
                .join()
                .expect("CONNECT tunnel worker panicked");
        } else {
            index += 1;
        }
    }
}

fn tunnel_connect(
    mut client: TcpStream,
    upstream: SocketAddr,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    const MAX_CONNECT_HEAD: usize = 16 * 1024;

    client.set_nonblocking(false)?;
    client.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if head.len() == MAX_CONNECT_HEAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CONNECT request head exceeded test bound",
            ));
        }
        client.read_exact(&mut byte)?;
        head.push(byte[0]);
    }
    let first_line = head
        .split(|byte| *byte == b'\n')
        .next()
        .and_then(|line| std::str::from_utf8(line).ok())
        .unwrap_or_default();
    if !first_line.starts_with("CONNECT ") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "test proxy accepts CONNECT only",
        ));
    }

    let mut origin = TcpStream::connect_timeout(&upstream, Duration::from_secs(3))?;
    // Relay small writes immediately in both directions (no Nagle stall).
    client.set_nodelay(true)?;
    origin.set_nodelay(true)?;
    client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
    client.flush()?;
    let poll = Duration::from_millis(25);
    client.set_read_timeout(Some(poll))?;
    origin.set_read_timeout(Some(poll))?;
    client.set_write_timeout(Some(Duration::from_secs(3)))?;
    origin.set_write_timeout(Some(Duration::from_secs(3)))?;

    let mut client_open = true;
    let mut origin_open = true;
    let mut last_progress = Instant::now();
    let mut buffer = [0_u8; 16 * 1024];
    while client_open || origin_open {
        if stop.load(Ordering::Acquire) {
            let _ = client.shutdown(Shutdown::Both);
            let _ = origin.shutdown(Shutdown::Both);
            return Ok(());
        }
        let mut progressed = false;
        if client_open {
            match client.read(&mut buffer) {
                Ok(0) => {
                    client_open = false;
                    let _ = origin.shutdown(Shutdown::Write);
                }
                Ok(count) => {
                    origin.write_all(&buffer[..count])?;
                    progressed = true;
                }
                Err(error) if transient_read(&error) => {}
                Err(error) if closed_peer(&error) => client_open = false,
                Err(error) => return Err(error),
            }
        }
        if origin_open {
            match origin.read(&mut buffer) {
                Ok(0) => {
                    origin_open = false;
                    let _ = client.shutdown(Shutdown::Write);
                }
                Ok(count) => {
                    client.write_all(&buffer[..count])?;
                    progressed = true;
                }
                Err(error) if transient_read(&error) => {}
                Err(error) if closed_peer(&error) => origin_open = false,
                Err(error) => return Err(error),
            }
        }
        if progressed {
            last_progress = Instant::now();
        } else if last_progress.elapsed() >= Duration::from_secs(5) {
            let _ = client.shutdown(Shutdown::Both);
            let _ = origin.shutdown(Shutdown::Both);
            return Ok(());
        }
    }
    Ok(())
}

fn transient_read(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

fn closed_peer(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
    )
}

impl ProxyTrap {
    pub fn start(responses: impl IntoIterator<Item = ResponseScript>) -> io::Result<Self> {
        Ok(Self {
            server: ScriptedServer::start(responses)?,
        })
    }

    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}", self.server.address())
    }

    #[must_use]
    pub fn request_count(&self) -> usize {
        self.server.request_count()
    }

    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.server.requests()
    }

    pub fn wait_for_requests(&self, count: usize, timeout: Duration) -> Result<(), String> {
        self.server.wait_for_requests(count, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_a_second_tunnel_while_the_first_remains_open() {
        let upstream =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("loopback upstream");
        let proxy = ConnectProxy::start(upstream.local_addr().expect("upstream address"))
            .expect("CONNECT proxy");

        let first = open_tunnel(proxy.address).expect("first CONNECT tunnel");
        let second = open_tunnel(proxy.address);

        drop(first);
        drop(proxy);

        second.expect("second CONNECT tunnel must not wait for the first tunnel to close");
    }

    fn open_tunnel(proxy: SocketAddr) -> io::Result<TcpStream> {
        let mut stream = TcpStream::connect(proxy)?;
        stream.set_read_timeout(Some(Duration::from_millis(750)))?;
        stream.write_all(b"CONNECT example.test:443 HTTP/1.1\r\nHost: example.test:443\r\n\r\n")?;
        stream.flush()?;

        let mut response = [0_u8; 64];
        let count = stream.read(&mut response)?;
        if !response[..count].starts_with(b"HTTP/1.1 200 ") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CONNECT proxy did not return a success response",
            ));
        }
        Ok(stream)
    }
}
