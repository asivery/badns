use std::{collections::HashMap};

use std::convert::Infallible;
use std::net::{SocketAddr, SocketAddrV4};

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Frame};
use hyper::header::HeaderValue;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};

use crate::jsbridge::Address;

type Bindings = HashMap<String, String>;
fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

async fn main_service(
    request: Request<hyper::body::Incoming>,
    peer_address: SocketAddr,
    bindings: &Bindings,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Infallible> {
    macro_rules! send_and_log {
        ($log:expr) => {
            println!("[HTTP]: {}", $log);
            return Ok(Response::new(full($log)));
        };
    }

    let mut host: String = match request.headers().get("Host") {
        Some(e) => e.to_str().unwrap_or("op").to_string(),
        None => {
            send_and_log!(format!("Peer {} omitted host header!", peer_address));
        }
    };

    if let Some(x) = host.find(':') {
        host = host[..x].to_string();
    }

    if !bindings.contains_key(&host) {
        send_and_log!(format!(
            "Peer {} queried a non-bound host {}",
            peer_address, host
        ));
    }
    let rebound_host = bindings.get(&host).unwrap();

    let mut new_headers = request.headers().clone();
    let new_host_value = match HeaderValue::from_str(rebound_host) {
        Ok(e) => e,
        Err(_) => {
            send_and_log!(format!(
                "Invalid host header value: {} rebound from {}",
                &rebound_host, &host
            ));
        }
    };
    new_headers.insert("Host", new_host_value);

    let uri = request.uri().clone();

    if host == "balls" {
        return Ok(Response::new(full(":3")));
    }

    let stream = match TcpStream::connect(rebound_host).await {
        Err(_) => {
            send_and_log!(format!("Cannot connect to uri {}", &rebound_host));
        }
        Ok(e) => e,
    };

    let io = TokioIo::new(stream);

    let (mut sender, conn) = match hyper::client::conn::http1::handshake(io).await {
        Err(e) => {
            send_and_log!(format!(
                "Error - handshake to {} ({})",
                &rebound_host,
                e.to_string()
            ));
        }
        Ok(e) => e,
    };

    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            println!("Connection failed: {:?}", err);
        }
    });
    let original_method = request.method().clone();

    let frame_stream = request.into_body().map_frame(|frame| {
        let frame = if let Ok(data) = frame.into_data() {
            data
        } else {
            Bytes::new()
        };

        Frame::data(frame)
    });

    let mut real_request_builder = Request::builder().uri(uri).method(original_method);
    for (key, val) in new_headers.iter() {
        real_request_builder
            .headers_mut()
            .unwrap()
            .insert(key, val.clone());
    }
    let real_request = real_request_builder.body(frame_stream.boxed()).unwrap();
    let real_result = match sender.send_request(real_request).await {
        Err(e) => {
            send_and_log!(format!("Unknown error: {}", e.to_string()));
        }
        Ok(e) => e,
    };

    let real_status = real_result.status();
    let real_headers = real_result.headers().clone();

    let out_frame_stream = real_result.into_body().map_frame(|frame| {
        let frame = if let Ok(data) = frame.into_data() {
            data
        } else {
            Bytes::new()
        };

        Frame::data(frame)
    });

    let mut final_result = Response::new(out_frame_stream.boxed());
    *final_result.status_mut() = real_status;
    for (key, val) in real_headers.iter() {
        final_result.headers_mut().insert(key, val.clone());
    }

    Ok(final_result)
}

#[tokio::main]
pub async fn run_http_server(address: &Address, bindings: Bindings) -> Result<(), String> {
    let addr: SocketAddrV4 = address
        .to_canonical()
        .parse()
        .expect("Invalid bound address");

    let listener = TcpListener::bind(addr)
        .await
        .expect("Couldn't start the listener");

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = match listener.accept().await {
            Err(_) => continue,
            Ok(a) => a,
        };
        let stream_address = match stream.peer_addr() {
            Err(_) => {
                println!("[HTTP]: Couldn't get the peer address!");
                continue;
            }
            Ok(e) => e,
        };

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently

        let copy_for_service = bindings.clone();
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(
                    io,
                    service_fn(|x| main_service(x, stream_address, &copy_for_service)),
                )
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}
