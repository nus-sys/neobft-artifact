cargo build --release
rsync target/release/replicated nsl-node1.d2:/local/cowsay/artifacts
rsync target/release/replicated nsl-node10.d2:/local/cowsay/artifacts