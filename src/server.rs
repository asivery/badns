use rustdns::Opcode;
use rustdns::QR;
use rustdns::Question;
use rustdns::Record;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::SystemTime;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::sync::OnceCell;
use tokio::time::{timeout, Duration};

use crate::jsbridge::Address;
use crate::jsbridge::JSBridge;
use crate::ttldict::TTLDict;

use rustdns::Message;

static OUTBOUND: OnceCell<UdpSocket> = OnceCell::const_new();

struct CacheEntry {
    entry: Vec<Record>,
    authoritative: bool,
    init_time: SystemTime,
}

static CACHE: OnceCell<Mutex<TTLDict<u64, CacheEntry>>> = OnceCell::const_new();

async fn query_upstream(question: &Question, bridge: &JSBridge) -> Vec<Record> {
    let outbound = OUTBOUND
        .get_or_init(|| async { UdpSocket::bind("0.0.0.0:0").await.unwrap() })
        .await;
    let mut message = Message::default();
    message.questions.push(question.clone());
    let serialized = match message.to_vec() {
        Ok(e) => e,
        Err(_) => {
            println!("Failed to serialize message, this should never happen!");
            return Vec::default();
        }
    };

    let timeout_duration = Duration::from_secs(5);
    let upstreams_clone = bridge.upstreams.lock().unwrap().clone();
    for upstream in upstreams_clone {
        let canonical = upstream.to_canonical();
        println!(
            "[Upstream] Querying upstream {} for domain {}",
            canonical, question.name
        );
        outbound.connect(upstream.to_canonical()).await.unwrap();
        match outbound.send(&serialized).await {
            Ok(_) => 0,
            Err(err) => {
                println!(
                    "[Upstream]: Failed to send data to upstream {} ({})",
                    canonical, err
                );
                continue;
            }
        };
        let mut buffer = vec![0; 4096];
        let outbound_future = outbound.recv(&mut buffer);
        let outbound_result = match timeout(timeout_duration, outbound_future).await {
            Ok(res) => res,
            Err(_) => {
                println!(
                    "[Upstream]: Timed out while waiting for upstream {}'s response!",
                    canonical
                );
                continue;
            }
        };
        let len = match outbound_result {
            Ok(e) => e,
            Err(err) => {
                println!(
                    "[Upstream]: Failed to receive data from upstream {} ({})",
                    canonical, err
                );
                continue;
            }
        };
        let answer = match Message::from_slice(&buffer[0..len]) {
            Ok(e) => e,
            Err(err) => {
                println!(
                    "[Upstream]: Received malformed data from upstream {} ({})",
                    canonical, err
                );
                continue;
            }
        };
        if !answer.answers.is_empty() {
            return answer.answers.clone();
        }
    }
    if message.answers.is_empty() {
        println!("[Upstream]: Upstream had no results for {}", question.name);
    }
    Vec::default()
}

fn hash_question(question: &Question) -> u64 {
    let mut hasher = DefaultHasher::new();
    question.name.hash(&mut hasher);
    question.r#type.hash(&mut hasher);
    question.class.hash(&mut hasher);
    hasher.finish()
}

async fn handle_packet(
    buffer: &[u8],
    peer: &SocketAddr,
    bridge: &Rc<Mutex<JSBridge>>,
    socket: &UdpSocket,
) {
    let mut cache = CACHE
        .get_or_init(|| async { Mutex::new(TTLDict::new()) })
        .await
        .lock()
        .await;
    let peer_address = peer.to_string();
    let own_address = &socket.local_addr().unwrap().to_string();
    let message = match Message::from_slice(buffer) {
        Ok(e) => e,
        Err(err) => {
            println!(
                "[DNS]: Malformed incoming message from {} ({})",
                peer_address, err
            );
            return;
        }
    };
    let mut instance = bridge.lock().await;

    let mut outbound_response = message.clone();
    outbound_response.qr = QR::Response;
    outbound_response.opcode = Opcode::Query;
    outbound_response.answers = Vec::new();
    for question in &message.questions {
        let hashed_question = hash_question(question);
        println!(
            "[DNS]: Incoming query for {} from {}",
            question.name, peer_address
        );
        if let Some(cached_entry) = cache.get(&hashed_question) {
            println!(
                "[Cache]: Reading response from cache: {} (hash={})",
                question.name, hashed_question
            );
            let mut answers = cached_entry.entry.clone();
            let ttl_offset = SystemTime::now()
                .duration_since(cached_entry.init_time)
                .unwrap();
            for ans in &mut answers {
                ans.ttl -= ttl_offset;
            }
            outbound_response.aa = cached_entry.authoritative;
            outbound_response.answers.extend(answers);
        } else {
            let js_answer = instance.get_response(question, &peer_address, own_address);
            let mut answers = js_answer.records;

            outbound_response.aa = js_answer.authoritative;

            if answers.is_empty() {
                answers = query_upstream(question, &instance).await;
            }
            if !answers.is_empty() {
                let min_ttl: Duration = answers
                    .iter()
                    .map(|x| x.ttl)
                    .min()
                    .unwrap_or(Duration::from_secs(0));
                println!(
                    "[Cache]: Writing answer to cache: {} (hash={} min_ttl={}s)",
                    question.name,
                    hashed_question,
                    min_ttl.as_secs()
                );
                cache.set(
                    hashed_question,
                    CacheEntry {
                        entry: answers.clone(),
                        authoritative: js_answer.authoritative,
                        init_time: SystemTime::now(),
                    },
                    min_ttl,
                );
            }

            outbound_response.answers.extend(answers);
        }
    }

    let as_bytes = match outbound_response.to_vec() {
        Ok(e) => e,
        Err(err) => {
            println!("[DNS]: Malformed internal data ({})", err);
            return;
        }
    };
    if socket.send_to(&as_bytes, peer).await.is_err() {        
        println!("[DNS]: Failed sending response to {}", peer_address);
    }
}

pub async fn run_server(address: Address, bridge: Rc<Mutex<JSBridge>>) {
    let full_address = address.to_canonical();
    let socket = match UdpSocket::bind(&full_address).await {
        Ok(socket) => socket,
        Err(error) => panic!("Couldn't bind server: {}", error),
    };

    let mut buf = vec![0; 4096];

    loop {
        let (n, peer) = match socket.recv_from(&mut buf).await {
            Ok(e) => e,
            Err(x) => {
                println!(
                    "[UDP]: Error while receiving data on {}: {}",
                    full_address, x
                );
                continue;
            }
        };

        handle_packet(&buf[..n], &peer, &bridge, &socket).await;
    }
}
