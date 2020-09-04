// SPDX-License-Identifier: Apache-2.0

//! The Enarx Keep runtime binary.
//!
//! It can be used to run a Wasm file with given command-line
//! arguments and environment variables.
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

#![deny(missing_docs)]
#![deny(clippy::all)]
//#![feature(proc_macro_hygiene, decl_macro)]

mod bundle;
mod config;
mod virtfs;
mod workload;

use cfg_if::cfg_if;

#[macro_use]
extern crate serde_derive;

use openssl::asn1::Asn1Time;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;

use std::path::Path;
use warp::Filter;
#[derive(Serialize, Deserialize)]
struct Payload {
    encoding: String,
    contents: Vec<u8>,
}

use log::info;
/// Source of the key to use for TLS
//pub const KEY_SOURCE: &str = "file-system";
use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::io::FromRawFd;

pub const KEY_SOURCE: &str = "generate";
#[cfg(unix)]
const FD: std::os::unix::io::RawFd = 3;
/*
fn main() {
    let _ = env_logger::try_init_from_env(env_logger::Env::default());

    // Skip the program name by default, but also skip the enarx-keepldr image
    // name if it happens to precede the regular command line arguments.
    let nskip = 1 + std::env::args()
        .take(1)
        .filter(|s| s.ends_with("enarx-keepldr"))
        .count();
    let mut args = std::env::args().skip(nskip);
    let vars = std::env::vars();

    let mut reader = if let Some(path) = args.next() {
        File::open(&path).expect("Unable to open file")
    } else {
        cfg_if! {
            if #[cfg(unix)] {
                unsafe { File::from_raw_fd(FD) }
            } else {
                unreachable!();
            }
        }
    };

    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .expect("Failed to load workload");
*/

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let listen_port: u16 = args[0].parse().unwrap();
    let (server_key, server_cert) = get_credentials_bytes();

    // POST /payload
    let workload = warp::post()
        .and(warp::path("payload"))
        .and(warp::body::json())
        .and_then(payload_launch);

    let routes = workload;
    warp::serve(routes)
        .tls()
        .cert(&server_cert)
        .key(&server_key)
        //TODO - fix this so that we can bind to other IP addresses
        .run(([127, 0, 0, 1], listen_port))
        .await;
}

fn create_new_runtime(recvd_data: &[u8]) {
    format!("About to attempt new runtime creation");
    let _ = env_logger::try_init_from_env(env_logger::Env::default());
    //TODO - get args these from main() if required
    //    let args = std::env::args().skip(1);
    let dummy_arr: [&str; 1] = [""];
    let vars = std::env::vars();

    let result = workload::run(recvd_data, &dummy_arr, vars).expect("Failed to run workload");
    println!("Got result (println) {:#?}", result);
    info!("got result: {:#?}", result);
}

async fn payload_launch(payload: Payload) -> Result<impl warp::Reply, warp::Rejection> {
    format!("Received a {} file", payload.encoding);
    println!("Received a {} file", payload.encoding);
    create_new_runtime(&payload.contents);
    Ok(warp::reply::with_status(
        "Payload received",
        warp::http::StatusCode::OK,
    ))
}

fn get_credentials_bytes() -> (Vec<u8>, Vec<u8>) {
    let (key, cert) = match KEY_SOURCE {
        "file-system" => (get_key_bytes_fs(), get_cert_bytes_fs()),
        "generate" => (generate_credentials()),
        //no match!
        _ => panic!("No match for credentials source"),
    };
    (key, cert)
}

//implementation for file system
fn get_cert_bytes_fs() -> Vec<u8> {
    let in_path = Path::new("key-material/server.crt");

    let in_contents = match std::fs::read(in_path) {
        Ok(in_contents) => {
            println!("Contents = of {} bytes", &in_contents.len());
            in_contents
        }
        Err(_) => {
            println!("Failed to read from file");
            panic!("We have no data to use");
        }
    };
    in_contents
}

//implementation for file system
fn get_key_bytes_fs() -> Vec<u8> {
    println!("Generating server key (PEM)");
    let in_path = Path::new("key-material/server.key");

    let in_contents = match std::fs::read(in_path) {
        Ok(in_contents) => {
            println!("Contents = of {} bytes", &in_contents.len());
            in_contents
        }
        Err(_) => {
            println!("Failed to read from file");
            panic!("We have no data to use");
        }
    };
    in_contents
}

//TODO - this is vital code, and needs to be carefully audited!
fn generate_credentials() -> (Vec<u8>, Vec<u8>) {
    let key = Rsa::generate(2048).unwrap();
    let pkey = PKey::from_rsa(key.clone()).unwrap();

    let mut x509_name = openssl::x509::X509NameBuilder::new().unwrap();
    x509_name.append_entry_by_text("C", "GB").unwrap();
    x509_name.append_entry_by_text("O", "enarx-test").unwrap();
    x509_name.append_entry_by_text("CN", "127.0.0.1").unwrap();
    let x509_name = x509_name.build();

    let mut x509_builder = openssl::x509::X509::builder().unwrap();
    match x509_builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()) {
        Err(e) => panic!("Problem creating cert {}", e),
        Ok(_) => {}
    };
    match x509_builder.set_not_after(&Asn1Time::days_from_now(7).unwrap()) {
        Err(e) => panic!("Problem creating cert {}", e),
        Ok(_) => {}
    };
    x509_builder.set_subject_name(&x509_name).unwrap();
    x509_builder.set_pubkey(&pkey).unwrap();
    x509_builder.sign(&pkey, MessageDigest::sha256()).unwrap();
    let certificate = x509_builder.build();

    (
        key.private_key_to_pem().unwrap(),
        certificate.to_pem().unwrap(),
    )
}
