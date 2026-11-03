
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub struct Observation<T> {
    pub body: Vec<u8>,
    pub content_types: Vec<String>,
    pub decoded: Result<T, String>,
}

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<Result<(Vec<u8>, Vec<String>), String>>>,
}
impl Server {
    fn start(reply: &[u8]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let reply = reply.to_vec();
        let task = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                if flag.load(Ordering::SeqCst) || Instant::now() > deadline {
                    return Err("listener stopped or timed out".into());
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(2)),
                    Err(e) => return Err(e.to_string()),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
            stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            let mut chunk = [0; 4096];
            let head_end = loop {
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") { break i; }
                let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
                if n == 0 { return Err("EOF before request headers".into()); }
                buf.extend_from_slice(&chunk[..n]);
            };
            let head = String::from_utf8(buf[..head_end].to_vec()).map_err(|e| e.to_string())?;
            let headers: Vec<_> = head.lines().skip(1).filter_map(|l| l.split_once(':'))
                .map(|(n,v)| (n.to_ascii_lowercase(), v.trim().to_owned())).collect();
            let len: usize = headers.iter().find(|(n,_)| n == "content-length")
                .ok_or("missing content-length")?.1.parse().map_err(|_| "bad content-length")?;
            while buf.len() < head_end + 4 + len {
                let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
                if n == 0 { return Err("EOF before request body".into()); }
                buf.extend_from_slice(&chunk[..n]);
            }
            let body = buf[head_end+4..head_end+4+len].to_vec();
            let cts = headers.into_iter().filter(|(n,_)| n == "content-type").map(|(_,v)|v).collect();
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.len());
            stream.write_all(response.as_bytes()).map_err(|e| e.to_string())?;
            stream.write_all(&reply).map_err(|e| e.to_string())?;
            stream.flush().map_err(|e| e.to_string())?;
            Ok((body, cts))
        });
        Self {url, stop, task: Some(task)}
    }
    fn finish(&mut self) -> (Vec<u8>, Vec<String>) {
        self.stop.store(true, Ordering::SeqCst);
        self.task.take().unwrap().join().expect("server thread panicked").expect("server I/O failed")
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(task) = self.task.take() { let _ = task.join(); }
    }
}
pub fn http() -> crate::c4::reqwest::Client {
    crate::c4::reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap()
}
pub async fn exchange<T>(make: impl FnOnce(String) -> crate::c4::Prepared<T>, reply: &[u8]) -> Observation<T> {
    let mut server = Server::start(reply);
    let result = make(server.url.clone()).send().await.map_err(|e| format!("{e:?}"));
    let (body, content_types) = server.finish();
    Observation {body, content_types, decoded: result}
}
