// SPDX-License-Identifier: Apache-2.0

mod bundle;

use clap::{App, Arg};
use std::io::{BufRead, BufReader, Read, Result};
use std::path::PathBuf;

fn add_paths(builder: &mut bundle::Builder, reader: &mut impl Read) -> Result<()> {
    let mut reader = BufReader::new(reader);

    loop {
        let mut buf = String::new();
        let size = reader.read_line(&mut buf)?;
        if size == 0 {
            break;
        }

        let path: PathBuf = buf.trim_end().into();
        builder.path(path);
    }

    Ok(())
}

fn main() {
    let matches = App::new("enarx-wasmres")
        .about("Bundle resource files into a Wasm file")
        .arg(
            Arg::with_name("INPUT")
                .help("Sets the input Wasm file")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("OUTPUT")
                .help("Sets the output Wasm file")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::with_name("prefix")
                .help("Sets the path prefix to be removed")
                .short("-p")
                .long("prefix")
                .takes_value(true)
                .default_value(""),
        )
        .arg(
            Arg::with_name("section")
                .help("Sets the section name")
                .short("-j")
                .long("section")
                .takes_value(true)
                .default_value(bundle::RESOURCES_SECTION),
        )
        .usage("find dir -type f | enarx-wasmres INPUT OUTPUT")
        .get_matches();

    let input_path = matches.value_of("INPUT").unwrap();
    let output_path = matches.value_of("OUTPUT").unwrap();

    let mut builder = bundle::Builder::new();
    let mut reader = std::io::stdin();
    add_paths(&mut builder, &mut reader).expect("couldn't read file list");

    let prefix = matches.value_of("prefix").unwrap();
    let section = matches.value_of("section").unwrap();

    builder.prefix(prefix).section(section);

    let input = std::fs::read(&input_path).expect("couldn't open input file");
    let mut output = std::fs::File::create(&output_path).expect("couldn't create output file");

    builder
        .build(input.as_slice(), &mut output)
        .expect("couldn't append custom section");
}
