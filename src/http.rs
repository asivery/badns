use std::collections::HashMap;

use std::convert::Infallible;
use std::net::SocketAddr;

use hyper::body::Bytes;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server};
use hyper_reverse_proxy::call;

use crate::jsbridge::Address;

type Bindings = HashMap<String, String>;
fn full<T: Into<Bytes>>(chunk: T) -> Body where Body: From<T>{
    Body::from(chunk)
}

async fn main_service(
    request: Request<hyper::Body>,
    peer_address: SocketAddr,
    bindings: &'static Bindings,
) -> Result<Response<hyper::Body>, Infallible> {
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

    match call(peer_address.ip(), rebound_host, request, &Client::new()).await {
        Ok(e) => Ok(e),
        Err(z) => {
            send_and_log!(format!(
                "Proxy error: {:?}",
                z
            ));
        }
    }
}

#[tokio::main]
pub async fn run_http_server(address: &Address, bindings: Bindings) {
    let addr: SocketAddr = address
        .to_canonical()
        .parse()
        .expect("Invalid bound address");

    let global_bindings: &'static Bindings = Box::leak(Box::new(bindings));
    let make_service = make_service_fn(|conn: &AddrStream| {
        let rem_addr = conn.remote_addr().clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| main_service(req, rem_addr, global_bindings)))
        }
    });

    let server = Server::bind(&addr).serve(make_service);


    println!("Running http server on {:?}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
