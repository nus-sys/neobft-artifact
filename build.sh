#!/bin/bash -ex

ARTIFACT=$(pwd)/artifact

rm -rf ${ARTIFACT}
mkdir ${ARTIFACT}

pushd neo4
cargo build --release --bin spec
cargo build --release --bin matrix
cp target/release/spec ${ARTIFACT}/neo4-run
cp target/release/matrix ${ARTIFACT}/neo4
popd

pushd neo100
cargo build --release --bin neo-client
cargo build --release --bin neo-replica
cargo build --release --bin neo-seq
cargo build --release --bin neo-relay
cp target/release/neo-client ${ARTIFACT}/neo100-client
cp target/release/neo-replica ${ARTIFACT}/neo100-replica
cp target/release/neo-seq ${ARTIFACT}/neo100-seq
cp target/release/neo-relay ${ARTIFACT}/neo100-relay
popd
