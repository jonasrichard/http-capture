FROM centos:7

RUN yum -y update && \
    yum -y install \
       ca-certificates \
       curl \
       gcc \
       glibc-devel

RUN mkdir /rust
WORKDIR /rust

RUN sh -c "curl https://sh.rustup.rs -sSf | sh -s -- -y"
