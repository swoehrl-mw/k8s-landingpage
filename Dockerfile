FROM clux/muslrust:1.91.0-stable AS builder
RUN mkdir /build
WORKDIR /build
COPY Cargo.toml Cargo.lock /build/
COPY src /build/src
RUN --mount=type=cache,target=/build/target \
    cargo build --release && \
    cp /build/target/x86_64-unknown-linux-musl/release/landingpage /build/landingpage

FROM alpine:3.22
RUN mkdir -p /app/static/
WORKDIR /app
COPY template.html /app/
COPY --from=builder /build/landingpage /app/
USER 65532:65532
CMD [ "/app/landingpage" ]
