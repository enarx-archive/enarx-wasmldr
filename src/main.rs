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

mod bundle;
mod config;
mod virtfs;
mod workload;

use cfg_if::cfg_if;
use log::info;

use openssl::asn1::Asn1Time;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::io::FromRawFd;

#[cfg(unix)]
const FD: std::os::unix::io::RawFd = 3;
/// Source of the key to use for TLS
pub const KEY_SOURCE: &str = "generate";

fn main() {
    let _ = env_logger::try_init_from_env(env_logger::Env::default());

    let mut args = std::env::args().skip(1);
    let vars = std::env::vars();

    //TODO - need to pass this in (e.g. as args).  Use sensible defaults for now
    //let listen_address: &str = &args[0];
    let _listen_address: &str = "127.0.0.1";
    //NOTE - these are currently unused
    let (_public_key, _private_key, _server_cert) = get_credentials_bytes(_listen_address);

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

    let result = workload::run(&bytes, args, vars).expect("Failed to run workload");

    info!("got result: {:#?}", result);
}

fn get_credentials_bytes(listen_addr: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (public_key, private_key, cert) = match KEY_SOURCE {
        "generate" => (generate_credentials(&listen_addr)),
        //no match!
        _ => panic!("No match for credentials source"),
    };
    (public_key, private_key, cert)
}

//TODO - this is vital code, and needs to be carefully audited!
fn generate_credentials(listen_addr: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let key = Rsa::generate(2048).unwrap();
    let pkey = PKey::from_rsa(key.clone()).unwrap();

    println!(
        "Should create a certificate for {}, but using hard-coded 127.0.0.1 instead",
        &listen_addr
    );

    let mut x509_name = openssl::x509::X509NameBuilder::new().unwrap();
    x509_name.append_entry_by_text("C", "GB").unwrap();
    x509_name.append_entry_by_text("O", "enarx-test").unwrap();
    //FIXME - problems when client parses some addresses need investigation
    x509_name.append_entry_by_text("CN", &listen_addr).unwrap();
    let x509_name = x509_name.build();

    let mut x509_builder = openssl::x509::X509::builder().unwrap();
    if let Err(e) = x509_builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()) {
        panic!("Problem creating cert {}", e)
    }
    if let Err(e) = x509_builder.set_not_after(&Asn1Time::days_from_now(7).unwrap()) {
        panic!("Problem creating cert {}", e)
    }

    x509_builder.set_subject_name(&x509_name).unwrap();
    x509_builder.set_pubkey(&pkey).unwrap();
    x509_builder.sign(&pkey, MessageDigest::sha256()).unwrap();
    let certificate = x509_builder.build();

    (
        key.public_key_to_pem().unwrap(),
        key.private_key_to_pem().unwrap(),
        certificate.to_pem().unwrap(),
    )
}
