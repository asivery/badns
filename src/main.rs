mod http;
mod jsbridge;
mod messages;
mod server;
mod ttldict;

use std::{collections::HashMap, env, fs::File, io::Read, path::Path, thread};

use http::run_http_server;
use jsbridge::{Address, JSBridge};
use quick_js::JsValue;
use server::run_server;
use std::sync::Arc;
use tokio::sync::Mutex;

fn read_file(file_name: String) -> String {
    let mut str = String::new();
    let mut file = match File::open(Path::new(&file_name)) {
        Ok(file) => file,
        Err(error) => {
            println!("Error while opening file {}: {}", file_name, error);
            return "".to_string();
        }
    };
    match file.read_to_string(&mut str) {
        Err(error) => {
            println!("Error while reading file {}: {}", file_name, error);
            return "".to_string();
        }
        Ok(_) => 0,
    };

    str
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: {} <config.js file location>", args[0]);
        return;
    }
    let config_file = &args[1];

    let bridge = Arc::new(Mutex::new(JSBridge::new()));
    let mut addresses = Vec::new();

    let http_host: String;
    let http_port: u16;
    let http_bindings: HashMap<String, String>;

    {
        let mut initial_reference = bridge.lock().await;
        initial_reference.add_extension("readFile", read_file);
        initial_reference.evaulate_file(config_file);
        addresses.clone_from(&initial_reference.bound_addresses.lock().unwrap());
        http_host = initial_reference
            .eval("badns_httpRedirectHost")
            .as_str()
            .unwrap()
            .to_string();
        http_port = match initial_reference.eval("badns_httpRedirectPort") {
            JsValue::Int(e) => e as u16,
            _ => 0,
        };
        http_bindings = initial_reference.http_redirects.lock().unwrap().clone();
        initial_reference.mark_http_as_frozen();
    }

    if http_port != 0 {
        println!("[HTTP]: Spawning HTTP Redirection Proxy");
        thread::spawn(move || {
            run_http_server(
                &Address {
                    address: http_host,
                    port: http_port,
                },
                http_bindings
            ).unwrap();
        });
    }

    // Start all servers
    let local = tokio::task::LocalSet::new();

    for address in &addresses {
        let bridge_reference = bridge.clone();
        let cloned_address = address.clone();
        local.spawn_local(async move {
            run_server(cloned_address, bridge_reference).await;
        });
    }
    local.await;
}
