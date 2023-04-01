VERSION 0.6
FROM public.ecr.aws/docker/library/rust:alpine3.17
COPY . .

ci:
  # TODO: figure out how to not repeat adding clippy
  # Since these BUILD commands don't have any other dependent BUILDs, they can run in parallel!
  RUN cargo check
  BUILD +cargo-clippy
  BUILD +cargo-fmt
  BUILD +beta-update

cargo-clippy:
  RUN rustup component add clippy
  RUN cargo check
  # allowing dirty since paths in the container like /usr /share show up as "dirty"
  RUN cargo clippy --fix --allow-dirty

cargo-fmt:
  RUN rustup component add rustfmt
  RUN cargo fmt --check

scheduled-ci:
  BUILD +nightly
  BUILD +beta-update

nightly:
  RUN rustup toolchain install nightly && rustup default nightly

  RUN cargo test --locked --all-features --all-targets

beta-update:
  RUN rustup toolchain install beta && rustup default beta

  # TODO: generate lockfile if one does not exist
  RUN cargo update

  ENV RUSTFLAGS="-D deprecated"
  RUN cargo test --locked --all-features --all-targets
