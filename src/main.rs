// SPDX-License-Identifier: Apache-2.0

//! The Enarx Keep runtime binary.
//!
//! It can be used to run a Wasm file with given command-line
//! arguments and environment variables.
//!
//! Now requires compilation with cargo nightly (`cargo +nightly build`)
//!
//! ## Example invocation
//!
//! ```console
//! $ wat2wasm fixtures/return_1.wat
//! $ RUST_LOG=enarx_wasmldr=info RUST_BACKTRACE=1 cargo run return_1.wasm
//!     Finished dev [unoptimized + debuginfo] target(s) in 0.07s
//!      Running `target/x86_64-unknown-linux-musl/debug/enarx-wasmldr target/x86_64-unknown-linux-musl/debug/build/enarx-wasmldr-c374d181f6abdda0/out/fixtures/return_1.wasm`
//! [2020-09-10T17:56:18Z INFO  enarx_wasmldr] got result: [
//!         I32(
//!             1,
//!         ),
//!     ]
//! ```

//!
//! On Unix platforms, the command can also read the workload from the
//! file descriptor (3):
//! ```console
//! $ RUST_LOG=enarx_wasmldr=info RUST_BACKTRACE=1 cargo run 3< return_1.wasm
//! ```
//!

//#![deny(missing_docs)]
#![deny(clippy::all)]
#![feature(asm)]
//#![feature(proc_macro_hygiene, decl_macro)]

mod attestation;
mod bundle;
mod config;
mod virtfs;
mod workload;

use koine::*;
use log::info;
use openssl::asn1::Asn1Time;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::pkey::Private;
use openssl::rsa::*;
use serde_cbor::{de, to_vec};
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
//use std::thread;
use std::error::Error;
use std::time::*;
//use std::{error::Error, process::exit};
use tokio::sync::mpsc::*;
use tokio::sync::Mutex;
//use tokio::task::*;
//#[cfg(unix)]
//use ciborium::de::from_reader;
//use sys_info::*;
use warp::Filter;

pub const KEY_SOURCE: &str = "generate";
pub type WorkloadPackage = Arc<Mutex<Workload>>;

pub struct Trigger {
    trigger: Sender<()>,
}
impl Trigger {
    async fn do_trig(&self) -> Result<impl warp::Reply, std::convert::Infallible> {
        let _trigger_res = self.trigger.clone().send(()).await;
        Ok(String::from("Trigger to stop HTTPS service"))
    }
}
#[cfg(unix)]
#[tokio::main(basic_scheduler)]
async fn main() {
    //This required when calling from Rust std::process::command.  Recorded
    // to allow debugging.
    //    let args: Vec<String> = std::env::args().skip(1).collect();
    let _args: Vec<String> = std::env::args().collect();

    //TODO - the mechanism for binding to an IP address is currently undefined.
    // It is expected that a new bridge will be created, to which this process
    //  will then bind.

    //FIXME - hard-coding for now
    //    let listen_address: &str = "127.0.0.1";
    //let listen_address: &str = "192.168.1.203";
    //This is the IP address of rome.sev.lab.enarx.dev (2021-01-07)
    let listen_address: &str = "147.75.68.181";
    //    let listen_address: &str = &args[0];
    //FIXME - hard-coding for now
    let listen_port: &str = "3040";
    //    let listen_port: &str = &args[1];

    let listen_socketaddr = SocketAddr::new(
        listen_address.parse::<IpAddr>().unwrap(),
        listen_port.parse().unwrap(),
    );
    let (server_key, server_cert) = get_credentials_bytes(listen_address);

    let workload_package = new_empty_workload_package();
    //println!(
    //    "Current pem array = {}",
    //    std::str::from_utf8(&server_cert).unwrap()
    //);
    let (trigger, mut rx) = channel(1);
    let trigger = Arc::new(Trigger { trigger });
    let workload = warp::post()
        .and(warp::path("workload"))
        .and(warp::body::bytes())
        .and(with_workload_package(workload_package.clone()))
        .and(warp::any().map(move || trigger.clone()))
        .and_then(payload_load);
    let route = workload;
    let (_, service) = warp::serve(route)
        .tls()
        .cert(&server_cert)
        .key(&server_key)
        .bind_with_graceful_shutdown(listen_socketaddr, async move {
            rx.recv().await;
        });
    service.await;

    let wlp = workload_package.lock().await;
    payload_run_sync(&wlp.wasm_binary);
}

pub fn new_empty_workload_package() -> WorkloadPackage {
    Arc::new(Mutex::new(Workload {
        wasm_binary: vec![0],
        human_readable_info: String::from(""),
    }))
}

pub fn with_workload_package(
    workload_package: WorkloadPackage,
) -> impl Filter<Extract = (WorkloadPackage,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || workload_package.clone())
}

fn create_new_runtime(recvd_data: &[u8]) -> Result<bool, String> {
    //println!("About to attempt new runtime creation");
    let _ = env_logger::try_init_from_env(env_logger::Env::default());
    //TODO - get args these from main() if required
    //    let args = std::env::args().skip(1);
    let dummy_arr: [&str; 1] = [""];
    let vars = std::env::vars();

    let result = workload::run(recvd_data, &dummy_arr, vars).expect("Failed to run workload");
    //println!("Got result (println) {:#?}", result);
    info!("got result: {:#?}", result);
    //TODO - some error checking
    Ok(true)
}

fn payload_run_sync(workload_data: &[u8]) -> bool {
    println!("[keepldr] About to run received workload");
    std::process::exit(match create_new_runtime(&workload_data) {
        Ok(_) => {
            //println!("Success - exiting");
            0
        }
        Err(err) => {
            eprintln!("error: {:?}", err);
            1
        }
    });
    #[allow(unreachable_code)]
    true
}

async fn payload_load<B: warp::Buf>(
    bytes: B,
    workload_package: WorkloadPackage,
    trigger: Arc<Trigger>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut wlp = workload_package.lock().await;
    //println!(
    //    "payload_launch bytes.bytes().len() = {}",
    //    bytes.bytes().len()
    //);
    let wbytes: &[u8] = bytes.bytes();
    //println!("payload_launch received {} bytes", wbytes.len());
    let workload_bytes = wbytes;

    //deserialise the Vector into a Payload (and handle errors)
    let workload: Workload;
    match de::from_slice(&workload_bytes) {
        Ok(wl) => {
            workload = wl;
            println!(
                "[keepldr] Received a workload: {}",
                workload.human_readable_info
            );
            /*
            println!(
                "About to spawn a workload {} bytes long",
                &workload.wasm_binary.len()
            );
            */
            *wlp = workload;
            let trigger_res = trigger.do_trig().await;
            match trigger_res {
                Ok(_) => {
                    let comms_complete = CommsComplete::Success;
                    let cbor_reply_body: Vec<u8> = to_vec(&comms_complete).unwrap();
                    Ok(cbor_reply_body)
                }
                Err(_e) => {
                    let comms_complete = CommsComplete::Failure;
                    let cbor_reply_body: Vec<u8> = to_vec(&comms_complete).unwrap();
                    Ok(cbor_reply_body)
                }
            }
        }
        Err(_) => {
            println!("[keepldr] Payload parsing problem");
            let cbore = LocalCborErr::new("Payload parsing problem");
            Err(warp::reject::custom(cbore))
        }
    }
}

fn get_credentials_bytes(listen_addr: &str) -> (Vec<u8>, Vec<u8>) {
    let (key, cert) = match KEY_SOURCE {
        "generate" => (generate_credentials(&listen_addr)),
        //no match!
        _ => panic!("No match for credentials source"),
    };
    (key, cert)
}

fn retrieve_existing_key() -> Option<Rsa<Private>> {
    //This function retrieves an existing key from the pre-launch
    // attestation in the case of AMD SEV
    let input_bytes: &[u8] = &Vec::new();
    let mut output_bytes = vec![0; 0];
    //println!("output_bytes has length {}", output_bytes.len());
    let expected_key_length: usize = match attestation::attest(&input_bytes, &mut output_bytes) {
        Ok(attestation) => {
            //println!("Attestation OK");
            match attestation {
                attestation::Attestation::Sev(expected_key_length) => expected_key_length,
                attestation::Attestation::Sgx(_) => 0,
                attestation::Attestation::None => 0,
            }
        }
        Err(_) => 0,
    };
    //println!("Expected key length = {}", expected_key_length);
    if expected_key_length > 0 {
        let mut cbor_key_bytes: Vec<u8> = vec![0; expected_key_length];
        /*
        println!(
            "Ready to receive key_bytes, which has length {} ({} expected)",
            cbor_key_bytes.len(),
            expected_key_length,
        );
        */
        let _attempted_attestation_result =
            attestation::attest(&input_bytes, &mut cbor_key_bytes).unwrap();
        /*
        println!(
            "Byte array retrieved from attestation, {} bytes",
            cbor_key_bytes.len()
        );
        */
        //println!("Bytes = {:?}", &cbor_key_bytes);

        //TODO - error checking
        let key_bytes_value: ciborium::value::Value =
            ciborium::de::from_reader(cbor_key_bytes.as_slice()).unwrap();

        let key_bytes = match key_bytes_value {
            ciborium::value::Value::Bytes(bytes) => bytes,
            _ => panic!("not bytes"),
        };

        //TODO - move to der?
        let key_result = openssl::rsa::Rsa::private_key_from_pem(&key_bytes);
        let key: Option<Rsa<Private>> = match key_result {
            Ok(key) => Some(key),
            Err(_) => {
                println!("[keepldr] Error creating RSA private key from pem");
                None
            }
        };
        println!("[keepldr] Key retrieved from attestation, RSA key created");
        key
    } else {
        None
    }
}

//TODO - this is vital code, and needs to be carefully audited!
fn generate_credentials(_listen_addr: &str) -> (Vec<u8>, Vec<u8>) {
    //TODO - parameterise key_length?
    let key_length = 2048;
    let key_opt = retrieve_existing_key();
    let key: Rsa<Private> = match key_opt {
        Some(key) => key,
        None => {
            println!("[keepldr] No key available, so generating one");
            Rsa::generate(key_length).unwrap()
        }
    };

    let pkey = PKey::from_rsa(key.clone()).unwrap();

    //let myhostname = hostname().unwrap();
    //FIXME - need to fix this!
    let myhostname = String::from("rome.sev.lab.enarx.dev");
    let mut x509_name = openssl::x509::X509NameBuilder::new().unwrap();
    x509_name.append_entry_by_text("C", "GB").unwrap();
    x509_name.append_entry_by_text("O", "enarx-test").unwrap();

    x509_name.append_entry_by_text("CN", &myhostname).unwrap();
    //TODO - include SGX case, where we're adding public key (?) information
    //       to this cert
    let x509_name = x509_name.build();
    let mut x509_builder = openssl::x509::X509::builder().unwrap();
    //from haraldh
    x509_builder.set_issuer_name(&x509_name).unwrap();

    //from haraldh
    //FIXME - this sets certificate creation to daily granularity - need to deal with
    // occasions when we might straddle the date
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let t = t / (60 * 60 * 24) * 60 * 60 * 24;
    let t_end = t + 60 * 60 * 24 * 7;
    if let Err(e) = x509_builder.set_not_before(&Asn1Time::from_unix(t as _).unwrap()) {
        panic!("Problem creating cert {}", e)
    }
    if let Err(e) = x509_builder.set_not_after(&Asn1Time::from_unix(t_end as _).unwrap()) {
        panic!("Problem creating cert {}", e)
    }
    x509_builder.set_subject_name(&x509_name).unwrap();
    x509_builder.set_pubkey(&pkey).unwrap();
    x509_builder.sign(&pkey, MessageDigest::sha256()).unwrap();
    let certificate = x509_builder.build();
    println!("[keepldr] Created a certificate for {}", &myhostname);
    /*
    println!(
        "Current pem array = {}",
        std::str::from_utf8(&certificate.to_pem().unwrap()).unwrap()
    );

    println!(
        "Private key = {}",
        std::str::from_utf8(&pkey.private_key_to_pem_pkcs8().unwrap()).unwrap()
    );
    */
    (
        //TODO - move to der
        key.private_key_to_pem().unwrap(),
        certificate.to_pem().unwrap(),
    )
}

#[derive(Debug)]
struct LocalCborErr {
    details: String,
}

impl LocalCborErr {
    fn new(msg: &str) -> LocalCborErr {
        LocalCborErr {
            details: msg.to_string(),
        }
    }
}

impl fmt::Display for LocalCborErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl Error for LocalCborErr {
    fn description(&self) -> &str {
        &self.details
    }
}

impl warp::reject::Reject for LocalCborErr {}
