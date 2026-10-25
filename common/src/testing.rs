//! A local HTTP server with canned answers, for tests of HTTP clients.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// One canned HTTP answer.
#[derive(Debug)]
pub struct CannedResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl CannedResponse {
    /// Create an answer with a status, content type, and body.
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self { status, content_type, body: body.into() }
    }
}

/// Answer one connection per response, in order, and return the server URL and a task that yields the raw requests.
pub async fn serve(responses: Vec<CannedResponse>) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind a local port");
    let url = format!("http://{}", listener.local_addr().expect("read the local address"));
    let server = tokio::spawn(async move {
        let mut requests = Vec::with_capacity(responses.len());
        for response in responses {
            let (mut stream, _) = listener.accept().await.expect("accept a connection");
            requests.push(read_request(&mut stream).await);
            // Closing each connection makes the client open a new one, which the next accept answers.
            let head = format!(
                "HTTP/1.1 {} Canned\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                response.status,
                response.content_type,
                response.body.len(),
            );
            stream.write_all(head.as_bytes()).await.expect("write the response head");
            stream.write_all(&response.body).await.expect("write the response body");
        }
        requests
    });
    (url, server)
}

/// Read one request with a `content-length` body.
async fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let read = stream.read(&mut buffer).await.expect("read the request");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        let Some(head_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&request[..head_end]).to_lowercase();
        let body_length: usize = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .map_or(0, |length| length.trim().parse().expect("a numeric content length"));
        if request.len() >= head_end + 4 + body_length {
            break;
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}
