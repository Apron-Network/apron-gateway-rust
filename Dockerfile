# build apron-gateway
FROM rust:1.57 as builder

ARG GIT_REPO="https://github.com/Apron-Network/apron-gateway-rust.git"
ARG GIT_BRANCH="main"

RUN git clone -b ${GIT_BRANCH} --recursive ${GIT_REPO}
WORKDIR /builds/apron-gateway

RUN rustup default nightly-2021-11-08 && rustup target add wasm32-unknown-unknown
RUN cargo build --release --locked
RUN cp target/release/apron-gateway /apron-gateway

FROM ubuntu
COPY --from=builder /apron-gateway /
ENTRYPOINT ["/apron-gateway"]

