# HTTP capture

Capture and filter HTTP traffic with pcap

## Cross compilation

Cross compilation is done in a Docker container which has `libpcap-dev`, so in the
target system it needs to have installed.

```
cargo install cross
cargo cross --target x86_64-unknown-linux-gnu
```

On the target host

```
apt install -y libpcap-dev
sudo setcap cap_net_raw,cap_net_admin=eip http-capture
```

## Cross complication with Docker

On Mac M architecture

```
docker build -f Dockerfile.centos7 . -t capture-builder:centos7
docker run -ti --rm --platform linux/amd64 -v $PWD:/rust capture-builder:centos7 bash

cargo build --target-dir target-linux --release
```
