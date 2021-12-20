# build apron-gateway
FROM paritytech/ci-linux:production as builder

ARG GIT_REPO="https://github.com/Apron-Network/apron-gateway-rust.git"
ARG GIT_BRANCH="main"

RUN git clone -b ${GIT_BRANCH} --recursive ${GIT_REPO}
WORKDIR /builds/apron-gateway-rust

RUN rustup default nightly-2021-11-08 && rustup target add wasm32-unknown-unknown
RUN cargo build --release --locked
RUN cp target/release/apron-gateway /apron-gateway
RUN cp -fr release /release

FROM ubuntu
RUN  apt update && apt install -y libssl-dev
COPY --from=builder /apron-gateway /
COPY --from=builder /release /release
#ENTRYPOINT ["/apron-gateway"]

