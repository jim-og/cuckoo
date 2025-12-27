use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::Request;
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use tokio::io::{self, AsyncWriteExt as _};
use tokio::net::TcpStream;

pub async fn fetch_url() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = "http://httpbin.org/ip".parse::<hyper::Uri>()?;
    let host = url.host().expect("URI has no host");
    let port = url.port_u16().unwrap_or(80);
    let addr = format!("{}:{}", host, port);

    // Open a TCP connection to the remote host.
    let stream = TcpStream::connect(addr).await?;

    let io = TokioIo::new(stream);

    // Create the Hyper client
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    // Spawn a task to poll the connection, driving HTTP state.
    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            eprintln!("Connection failed: {:?}", err)
        }
    });

    // The authority of our URL will be the hostname of the httpbin remote.
    let authority = url.authority().unwrap().clone();

    let req = Request::builder()
        .uri(url)
        .header(hyper::header::HOST, authority.as_str())
        .body(Empty::<Bytes>::new())?;

    // Send the request and await the response.
    let mut res = sender.send_request(req).await?;

    println!("Response status: {}", res.status());

    // Stream the body, writing each frame to stdout as it arrives.
    while let Some(next_frame) = res.frame().await {
        let frame = next_frame?;
        if let Some(chunk) = frame.data_ref() {
            io::stdout().write_all(chunk).await?;
        }
    }

    Ok(())
}
