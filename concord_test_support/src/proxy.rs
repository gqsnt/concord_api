use std::io;
use std::time::Duration;

use crate::server::{RecordedRequest, ResponseScript, ScriptedServer};

pub struct ProxyTrap {
    server: ScriptedServer,
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
