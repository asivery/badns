use quick_js::{Callback, Context, JsValue};
use rustdns::{Question, Record, Resource};
use serde_json::Value;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::time::Duration;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::messages::{SUPPORTED_RR, SUPPORTED_RR_NAMES};

#[derive(Debug, Clone)]
pub struct Address {
    pub address: String,
    pub port: u16,
}

impl Address {
    pub fn to_canonical(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

pub struct JSBridge {
    pub bound_addresses: Arc<Mutex<Vec<Address>>>,
    pub upstreams: Arc<Mutex<Vec<Address>>>,
    pub http_redirects: Arc<Mutex<HashMap<String, String>>>,
    context: Context,
}

impl JSBridge {
    pub fn new() -> JSBridge {
        let context = Context::new().unwrap();

        let mut this = JSBridge {
            context,
            bound_addresses: Arc::new(Mutex::new(Vec::new())),
            http_redirects: Arc::new(Mutex::new(HashMap::new())),
            upstreams: Arc::new(Mutex::new(Vec::new())),
        };

        let addresses_ref = this.bound_addresses.clone();
        this.context
            .add_callback(
                "badns_bindAddress",
                move |address: String, port: i32| -> i32 {
                    let real_port: u16 = match port.try_into() {
                        Ok(e) => e,
                        Err(_x) => panic!("Cannot bind on port that's out of bounds!"),
                    };
                    addresses_ref.lock().unwrap().push(Address {
                        address,
                        port: real_port,
                    });
                    0
                },
            )
            .unwrap();
        let upstreams_ref = this.upstreams.clone();
        this.context
            .add_callback("badns_upstream", move |address: String, port: i32| -> i32 {
                let real_port: u16 = match port.try_into() {
                    Ok(e) => e,
                    Err(_x) => panic!("Cannot use a port that"),
                };
                upstreams_ref.lock().unwrap().push(Address {
                    address,
                    port: real_port,
                });

                0
            })
            .unwrap();
        let http_redirects_ref = this.http_redirects.clone();
        this.context
            .add_callback(
                "badns_setHTTPRedirect",
                move |domain: String, target: String| -> i32 {
                    http_redirects_ref.lock().unwrap().insert(domain, target);
                    0
                },
            )
            .unwrap();

        this.context
            .add_callback("badns_log", |x: String| -> i32 {
                x.split("\n").for_each(|x| println!("[JS]: {}", x));
                0
            })
            .unwrap();

        let mut rrs: HashMap<String, i32> = HashMap::new();
        let mut reverse_rrs: HashMap<String, String> = HashMap::new();

        for i in 0..SUPPORTED_RR.len() {
            this.context
                .set_global(SUPPORTED_RR_NAMES[i], SUPPORTED_RR[i])
                .unwrap();
            rrs.insert(SUPPORTED_RR_NAMES[i].to_string(), SUPPORTED_RR[i].into());
            reverse_rrs.insert(
                SUPPORTED_RR[i].to_string(),
                SUPPORTED_RR_NAMES[i].to_string(),
            );
        }

        this.context.set_global("RRs", rrs).unwrap();
        this.context.set_global("RRrevs", reverse_rrs).unwrap();

        this.evaulate_file("init.js");

        this
    }

    pub fn mark_http_as_frozen(&mut self){
        self.context.set_global("badns_httpFrozen", true).unwrap();
    }

    pub fn add_extension<F>(&mut self, name: &str, callback: impl Callback<F> + 'static) {
        self.context.add_callback(name, callback).unwrap();
    }

    pub fn evaulate_file(&mut self, file_name: &str) {
        let mut init_file: File = match File::open(Path::new(file_name)) {
            Err(reason) => panic!("Couldn't open {}! ({})", file_name, reason),
            Ok(file) => file,
        };
        let mut init_contents = String::new();
        match init_file.read_to_string(&mut init_contents) {
            Err(reason) => panic!("Couldn't read {}! ({})", file_name, reason),
            Ok(e) => e,
        };

        match self.context.eval(&init_contents) {
            Err(reason) => panic!("Error while executing file! ({})", reason),
            Ok(e) => e,
        };
    }

    pub fn eval(&mut self, data: &str) -> JsValue {
        self.context.eval(data).unwrap()
    }

    pub fn get_response(
        &mut self,
        message: &Question,
        addr: &String,
        bind_addr: &String,
    ) -> Vec<Record> {
        let args: Vec<JsValue> = vec![
            JsValue::String({
                let mut name = message.name.chars();
                name.next_back();
                name.as_str().to_string()
            }),
            JsValue::Int(message.r#type as i32),
            JsValue::Int(message.class as i32),
            JsValue::String(addr.clone()),
            JsValue::String(bind_addr.clone()),
        ];
        let json = match self.context.call_function("badns_getResponse", args) {
            Ok(JsValue::String(str)) => match serde_json::from_str(&str) {
                Ok(e) => e,
                Err(e) => {
                    println!(
                        "[JS->RS]: Cannot deserialize data received from getResponse ({})",
                        e
                    );
                    Value::Null
                }
            },
            Err(e) => {
                println!("[JS]: Failed to run function! ({})", e);
                return Vec::default();
            }
            _ => Value::Null,
        };

        if !json.is_array() {
            println!("[JS->RS]: Received value is not an array!");
            return Vec::default();
        }

        let mut out: Vec<Record> = Vec::default();

        for resp in json.as_array().unwrap() {
            let name = match &resp["name"] {
                Value::String(e) => e,
                _ => &message.name,
            }
            .clone();
            let ttl = Duration::from_secs(resp["ttl"].as_u64().unwrap());
            let class = message.class;

            let type_name = resp["type"].as_str().unwrap();
            let resource: Option<Resource> = match type_name {
                "A" => Some(Resource::A(resp["ip"].as_str().unwrap().parse().unwrap())),
                "AAAA" => Some(Resource::AAAA(
                    resp["ip"].as_str().unwrap().parse().unwrap(),
                )),
                "CNAME" => Some(Resource::CNAME(
                    resp["target"].as_str().unwrap().to_string(),
                )),
                _ => {
                    println!("[JS->RS]: Unrecognized type: {}", type_name);
                    None
                }
            };

            if let Some(resource) = resource {
                out.push(Record {
                    name,
                    class,
                    ttl,
                    resource,
                });
            }
        }

        out
    }
}
