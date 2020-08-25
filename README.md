[![Workflow Status](https://github.com/enarx/enarx-wasmldr/workflows/test/badge.svg)](https://github.com/enarx/enarx-wasmldr/actions?query=workflow%3A%22test%22)
[![Average time to resolve an issue](https://isitmaintained.com/badge/resolution/enarx/enarx-wasmldr.svg)](https://isitmaintained.com/project/enarx/enarx-wasmldr "Average time to resolve an issue")
[![Percentage of issues still open](https://isitmaintained.com/badge/open/enarx/enarx-wasmldr.svg)](https://isitmaintained.com/project/enarx/enarx-wasmldr "Percentage of issues still open")
![Maintenance](https://img.shields.io/badge/maintenance-activly--developed-brightgreen.svg)

# enarx-wasmldr

The Enarx Keep runtime binary.

It can be used to run a Wasm file with given command-line
arguments and environment variables.

### Example invocation

```console
$ RUST_LOG=keep_runtime=info RUST_BACKTRACE=1 cargo run target/debug/fixtures/return_1.wasm
   Compiling keep-runtime v0.1.0 (/home/steveej/src/job-redhat/enarx/github_enarx_enarx/keep-runtime)
    Finished dev [unoptimized + debuginfo] target(s) in 4.36s
     Running `target/debug/keep-runtime`
[2020-01-23T21:58:16Z INFO  keep_runtime] got result: [
        I32(
            1,
        ),
    ]
```

License: Apache-2.0
