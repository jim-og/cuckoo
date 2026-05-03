use crate::utils::full;
use http::StatusCode;
use http::Uri;
use hyper::Request;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

pub async fn notify(
    uri: &Uri,
    body: impl Into<String>,
) -> Result<StatusCode, Box<dyn std::error::Error + Send + Sync>> {
    let host = uri.host().expect("URI has no host");
    let port = uri.port_u16().unwrap_or(80);
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

    let authority = uri.authority().unwrap().clone();

    let req = Request::builder()
        .uri(uri)
        .header(hyper::header::HOST, authority.as_str())
        .body(full(body.into()))?;

    // Send the request and await the response.
    let res = sender.send_request(req).await?;

    // // Stream the body, writing each frame to stdout as it arrives.
    // while let Some(next_frame) = res.frame().await {
    //     let frame = next_frame?;
    //     if let Some(chunk) = frame.data_ref() {
    //         io::stdout().write_all(chunk).await?;
    //     }
    // }

    Ok(res.status())
}
